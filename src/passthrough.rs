use crate::gpu::{self, GpuDevice};
use crate::vm;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::fs::File;
use std::io::Write;

/// Result of a passthrough operation
#[derive(Debug)]
pub enum PassthroughResult {
    Success(String),
    #[allow(dead_code)]
    Warning(String),
    #[allow(dead_code)]
    Error(String),
    NeedsReboot(String),
}

/// Log file path for audit trail
const LOG_PATH: &str = "/var/log/gpupass.log";

/// Global lock to prevent concurrent operations
static OPERATION_LOCK: Mutex<()> = Mutex::new(());

/// Acquire exclusive lock for GPU operations.
/// Returns an error if another operation is in progress.
fn acquire_operation_lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    OPERATION_LOCK.lock().map_err(|e| format!("Failed to acquire operation lock: {}", e))
}

/// Log an operation to the audit log.
fn log_operation(action: &str, gpu_address: &str, vm_name: &str, success: bool, detail: &str) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let log_line = format!(
        "{} [{}] gpu={} vm={} success={} detail={}\n",
        timestamp, action, gpu_address, vm_name, success, detail
    );

    // Try to append to log, but don't fail operations if logging fails
    if let Ok(mut file) = File::options().create(true).append(true).open(LOG_PATH) {
        let _ = file.write_all(log_line.as_bytes());
    }
}

/// Clean up old backup files that exceed the specified age.
fn cleanup_old_backups(max_age_secs: u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Clean vfio.conf backups
    if let Ok(entries) = std::fs::read_dir("/etc/modprobe.d") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("vfio.conf.bak.") {
                    if let Some(ts_str) = name.strip_prefix("vfio.conf.bak.") {
                        if let Ok(ts) = ts_str.parse::<u64>() {
                            if now.saturating_sub(ts) > max_age_secs {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    // Clean VM XML backups
    if let Ok(entries) = std::fs::read_dir("/etc/libvirt/qemu") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.contains(".gpupass.bak.") {
                    // Extract timestamp from filename
                    if let Some(pos) = name.rfind(".gpupass.bak.") {
                        let ts_str = &name[pos + ".gpupass.bak.".len()..];
                        if let Ok(ts) = ts_str.parse::<u64>() {
                            if now.saturating_sub(ts) > max_age_secs {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Bind a GPU to vfio-pci driver for passthrough
pub fn bind_to_vfio(gpu: &GpuDevice) -> Result<PassthroughResult, String> {
    // Acquire exclusive lock
    let _lock = acquire_operation_lock()?;

    if gpu.driver == "vfio-pci" {
        log_operation("bind_vfio", &gpu.pci_address, "", true, "already bound");
        return Ok(PassthroughResult::Success(
            "GPU already bound to vfio-pci".to_string(),
        ));
    }

    // Safety check: if this is the active boot GPU, runtime unbind will blackscreen the host.
    if gpu.is_boot_gpu && gpu::is_gpu_display_active(gpu) {
        log_operation("bind_vfio", &gpu.pci_address, "", false, "boot GPU with active display");
        return Err(
            "This is the active boot GPU with a connected display. \
             Unbinding it now will blackscreen the host. \
             Please use 'Apply Config' to set up vfio-pci at boot time instead, then reboot."
                .to_string(),
        );
    }

    // Safety check: verify GPU is not in use by host processes
    if gpu::is_gpu_in_use(&gpu.pci_address) {
        log_operation("bind_vfio", &gpu.pci_address, "", false, "GPU in use by host process");
        return Err(format!(
            "GPU {} appears to be in use by a host process. \
             Close all applications using the GPU before binding to vfio-pci.",
            gpu.pci_address
        ));
    }

    // Check if vfio-pci module is loaded
    if !gpu::is_vfio_loaded() {
        // Try to load vfio-pci
        let output = Command::new("modprobe")
            .args(["vfio-pci"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to load vfio-pci: {}", e))?;

        if !output.status.success() {
            log_operation("bind_vfio", &gpu.pci_address, "", false, "modprobe vfio-pci failed");
            return Err(format!(
                "Failed to load vfio-pci module: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    let pci_addr = &gpu.pci_address;

    // Get all devices in the same IOMMU group (we need to bind them all)
    let mut devices_to_bind = vec![pci_addr.clone()];
    let group_devices = gpu::get_iommu_group_devices(pci_addr);

    // Safety check: verify IOMMU group isolation
    if !gpu.iommu_group.is_empty() {
        if let Err(e) = gpu::verify_iommu_group_isolation(&gpu.iommu_group) {
            log_operation("bind_vfio", &gpu.pci_address, "", false, &e);
            return Err(format!(
                "IOMMU group isolation check failed: {}. \
                 This indicates your motherboard's IOMMU implementation may not properly \
                 isolate this GPU. Consider enabling ACS override in kernel parameters.",
                e
            ));
        }
    }

    // Safety check: warn about critical devices in IOMMU group
    let mut critical_devices = Vec::new();
    for device_addr in &group_devices {
        if gpu::is_critical_device(device_addr) {
            critical_devices.push(device_addr.clone());
        }
    }

    if !critical_devices.is_empty() {
        log_operation("bind_vfio", &gpu.pci_address, "", false, "critical devices in IOMMU group");
        return Err(format!(
            "Critical system devices found in IOMMU group: {}. \
             Binding these to vfio-pci may make the host unusable. \
             Consider using ACS override or a different GPU.",
            critical_devices.join(", ")
        ));
    }

    devices_to_bind.extend(group_devices);

    // Unbind from current driver and bind to vfio-pci
    for device_addr in &devices_to_bind {
        let new_id = get_device_ids(device_addr)?;

        // Unbind from current driver
        let unbind_path = format!("/sys/bus/pci/devices/{}/driver/unbind", device_addr);
        if std::path::Path::new(&unbind_path).exists() {
            std::fs::write(&unbind_path, device_addr.as_bytes())
                .map_err(|e| format!("Failed to unbind {}: {}", device_addr, e))?;
        }

        // Attempt PCI reset to clear lingering state (helps with NVIDIA reset bug)
        // Now we properly handle the error instead of ignoring it
        match gpu::reset_pci_device(device_addr) {
            Ok(()) => {},
            Err(e) => {
                log_operation("bind_vfio", device_addr, "", false, &format!("PCI reset failed: {}", e));
                // Continue anyway - some devices don't support FLR
                // but log it for debugging purposes
            }
        }

        // Add new ID to vfio-pci
        let new_id_path = "/sys/bus/pci/drivers/vfio-pci/new_id";
        std::fs::write(new_id_path, new_id.as_bytes())
            .map_err(|e| format!("Failed to add {} to vfio-pci: {}", new_id, e))?;
    }

    log_operation("bind_vfio", pci_addr, "", true, "bound to vfio-pci");

    // Clean up old backups (older than 7 days)
    cleanup_old_backups(7 * 24 * 3600);

    Ok(PassthroughResult::Success(format!(
        "GPU {} bound to vfio-pci successfully",
        pci_addr
    )))
}

/// Unbind a GPU from vfio-pci and return to original driver
pub fn unbind_from_vfio(gpu: &GpuDevice, original_driver: &str) -> Result<PassthroughResult, String> {
    // Acquire exclusive lock
    let _lock = acquire_operation_lock()?;

    let pci_addr = &gpu.pci_address;

    if gpu.driver != "vfio-pci" {
        log_operation("unbind_vfio", pci_addr, "", true, "not bound to vfio-pci");
        return Ok(PassthroughResult::Success(
            "GPU not bound to vfio-pci, nothing to do".to_string(),
        ));
    }

    // Remove from vfio-pci
    let remove_path = format!(
        "/sys/bus/pci/drivers/vfio-pci/{}/unbind",
        pci_addr
    );
    if std::path::Path::new(&remove_path).exists() {
        std::fs::write(&remove_path, pci_addr.as_bytes())
            .map_err(|e| {
                log_operation("unbind_vfio", pci_addr, "", false, &format!("unbind failed: {}", e));
                format!("Failed to unbind from vfio-pci: {}", e)
            })?;
    }

    // Bind back to original driver
    let bind_path = format!(
        "/sys/bus/pci/drivers/{}/bind",
        original_driver
    );
    if std::path::Path::new(&bind_path).exists() {
        std::fs::write(&bind_path, pci_addr.as_bytes())
            .map_err(|e| {
                log_operation("unbind_vfio", pci_addr, "", false, &format!("bind to {} failed: {}", original_driver, e));
                format!("Failed to bind to {}: {}", original_driver, e)
            })?;
    }

    log_operation("unbind_vfio", pci_addr, "", true, &format!("returned to {}", original_driver));
    cleanup_old_backups(7 * 24 * 3600);

    Ok(PassthroughResult::Success(format!(
        "GPU {} returned to {} driver",
        pci_addr, original_driver
    )))
}

fn get_device_ids(pci_addr: &str) -> Result<String, String> {
    let vendor_path = format!("/sys/bus/pci/devices/{}/vendor", pci_addr);
    let device_path = format!("/sys/bus/pci/devices/{}/device", pci_addr);

    let vendor = std::fs::read_to_string(&vendor_path)
        .map_err(|e| format!("Failed to read vendor ID: {}", e))?
        .trim()
        .to_string();
    let device = std::fs::read_to_string(&device_path)
        .map_err(|e| format!("Failed to read device ID: {}", e))?
        .trim()
        .to_string();

    Ok(format!("{} {}", vendor, device))
}

/// Generate the modprobe configuration for VFIO binding
pub fn generate_vfio_conf(gpus: &[&GpuDevice]) -> String {
    let mut options = String::new();

    // Check if any NVIDIA GPUs are present
    let has_nvidia = gpus.iter().any(|gpu| {
        gpu.vendor_name.to_lowercase().contains("nvidia")
            || gpu.driver.contains("nvidia")
            || gpu.device_name.to_lowercase().contains("nvidia")
    });

    for gpu in gpus {
        if !options.is_empty() {
            options.push(',');
        }
        options.push_str(&format!("{}:{}", gpu.vendor_id, gpu.device_id));
    }

    let mut conf = String::from(
        "# Generated by gpupass - GPU Passthrough Manager\n"
    );

    // Only add NVIDIA softdeps if NVIDIA hardware is present
    if has_nvidia {
        conf.push_str("softdep nvidia pre: vfio-pci\n");
        conf.push_str("softdep nouveau pre: vfio-pci\n");
        conf.push_str("softdep nvidia_drm pre: vfio-pci\n");
        conf.push_str("softdep nvidia_modeset pre: vfio-pci\n");
    }

    conf.push_str(&format!("options vfio-pci ids={}\n", options));
    conf
}

/// Apply VFIO configuration to modprobe
pub fn apply_vfio_conf(gpus: &[&GpuDevice]) -> Result<PassthroughResult, String> {
    // Acquire exclusive lock
    let _lock = acquire_operation_lock()?;

    let conf = generate_vfio_conf(gpus);
    let conf_path = "/etc/modprobe.d/vfio.conf";

    // Backup existing config with timestamp
    if std::path::Path::new(conf_path).exists() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup_path = format!("/etc/modprobe.d/vfio.conf.bak.{}", timestamp);
        std::fs::copy(conf_path, &backup_path)
            .map_err(|e| format!("Failed to backup existing config: {}", e))?;
    }

    std::fs::write(conf_path, &conf)
        .map_err(|e| {
            log_operation("apply_vfio_conf", "", "", false, &format!("write failed: {}", e));
            format!("Failed to write VFIO config: {}", e)
        })?;

    let gpu_addrs: Vec<&str> = gpus.iter().map(|g| g.pci_address.as_str()).collect();
    log_operation("apply_vfio_conf", &gpu_addrs.join(","), "", true, "vfio.conf updated");
    cleanup_old_backups(7 * 24 * 3600);

    Ok(PassthroughResult::NeedsReboot(
        "VFIO configuration applied. Reboot required for changes to take effect.".to_string(),
    ))
}

/// Generate initramfs update command
pub fn update_initramfs() -> Result<String, String> {
    // Detect which initramfs tool to use
    if std::path::Path::new("/usr/bin/update-initramfs").exists() {
        let output = Command::new("update-initramfs")
            .args(["-u", "-k", "all"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to update initramfs: {}", e))?;
        if output.status.success() {
            Ok("initramfs updated successfully".to_string())
        } else {
            Err(format!(
                "Failed to update initramfs: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    } else if std::path::Path::new("/usr/bin/dracut").exists() {
        let output = Command::new("dracut")
            .args(["--force"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run dracut: {}", e))?;
        if output.status.success() {
            Ok("initramfs updated with dracut".to_string())
        } else {
            Err(format!(
                "Failed to update initramfs with dracut: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    } else if std::path::Path::new("/usr/bin/mkinitcpio").exists() {
        let output = Command::new("mkinitcpio")
            .args(["-P"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to run mkinitcpio: {}", e))?;
        if output.status.success() {
            Ok("initramfs updated with mkinitcpio".to_string())
        } else {
            Err(format!(
                "Failed to update initramfs with mkinitcpio: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    } else {
        Err("No supported initramfs tool found (update-initramfs, dracut, mkinitcpio)".to_string())
    }
}

/// Full auto-passthrough: detect GPUs, configure VFIO, assign to VM
#[allow(dead_code)]
pub fn auto_passthrough(vm_name: &str) -> Vec<PassthroughResult> {
    let mut results = Vec::new();

    // Detect GPUs
    let gpus = gpu::detect_gpus();
    let passable_gpus: Vec<&GpuDevice> = gpus
        .iter()
        .filter(|g| g.can_passthrough)
        .collect();

    if passable_gpus.is_empty() {
        results.push(PassthroughResult::Error(
            "No GPUs available for passthrough".to_string(),
        ));
        return results;
    }

    // Check IOMMU
    if !gpu::check_iommu_enabled() {
        results.push(PassthroughResult::Error(
            "IOMMU is not enabled. Please enable IOMMU in your BIOS and kernel parameters.".to_string(),
        ));
        return results;
    }

    // Apply VFIO config
    match apply_vfio_conf(&passable_gpus) {
        Ok(r) => results.push(r),
        Err(e) => {
            results.push(PassthroughResult::Error(format!(
                "Failed to apply VFIO config: {}",
                e
            )));
            return results;
        }
    }

    // Update initramfs
    match update_initramfs() {
        Ok(msg) => results.push(PassthroughResult::Success(msg)),
        Err(e) => {
            results.push(PassthroughResult::Warning(format!(
                "Initramfs update failed: {}. You may need to run it manually.",
                e
            )));
        }
    }

    // Detect VMs for conflict checking
    let vms = vm::detect_vms();

    // Assign GPUs to VM
    for gpu in &passable_gpus {
        match vm::attach_gpu_to_vm(vm_name, gpu, &vms) {
            Ok(msg) => results.push(PassthroughResult::Success(format!(
                "GPU {} assigned to {}: {}",
                gpu.pci_address, vm_name, msg
            ))),
            Err(e) => results.push(PassthroughResult::Error(format!(
                "Failed to assign GPU {} to {}: {}",
                gpu.pci_address, vm_name, e
            ))),
        }
    }

    results
}
