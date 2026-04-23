use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::io::Read;

/// Allowed directories for VBIOS ROM files
const VBIOS_ALLOWED_DIRS: &[&str] = &[
    "/usr/share/vgabios/",
    "/usr/share/kvm/",
    "/var/lib/libvirt/vbios/",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub domain: String,
    pub bus: String,
    pub slot: String,
    pub function: String,
    pub pci_address: String,
    pub vendor_id: String,
    pub device_id: String,
    pub vendor_name: String,
    pub device_name: String,
    pub class_code: String,
    pub driver: String,
    pub iommu_group: String,
    pub can_passthrough: bool,
    pub passthrough_reason: String,
    pub is_boot_gpu: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct IommuGroup {
    pub group_number: String,
    pub devices: Vec<GpuDevice>,
}

pub fn detect_gpus() -> Vec<GpuDevice> {
    let mut gpus = Vec::new();

    // Parse lspci -nn -k output for VGA/3D controllers
    if let Ok(output) = Command::new("lspci")
        .args(["-nn", "-k", "-D"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let mut current_device: Option<GpuDevice> = None;
        let mut current_is_gpu = false;

        for raw_line in text.lines() {
            // Tab-indented lines are sub-info for the previous device
            if raw_line.starts_with('\t') {
                if current_is_gpu {
                    if let Some(ref mut dev) = current_device {
                        let content = raw_line.trim();
                        if let Some(rest) = content.strip_prefix("Kernel driver in use:") {
                            dev.driver = rest.trim().to_string();
                        }
                    }
                }
                continue;
            }

            let line = raw_line.trim();

            // Parse device header line: "0000:00:02.0 VGA compatible controller [0300]: ..."
            if let Some(addr_end) = line.find(' ') {
                let pci_addr = &line[..addr_end];
                // Real PCI addresses always contain ':' and '.'
                let looks_like_pci_addr = pci_addr.contains(':') && pci_addr.contains('.');
                let rest = &line[addr_end..];

                // Check if this is a GPU device (VGA or 3D controller)
                let is_vga = looks_like_pci_addr && (rest.contains("[0300]") || rest.contains("[0302]"));
                let is_3d = looks_like_pci_addr && rest.contains("3D controller");

                if is_vga || is_3d {
                    // Save previous GPU if exists
                    if let Some(dev) = current_device.take() {
                        if current_is_gpu {
                            gpus.push(dev);
                        }
                    }
                    let parts: Vec<&str> = pci_addr.split(':').collect();
                    let (domain, bus, slot_func) = if parts.len() == 3 {
                        (parts[0].to_string(), parts[1].to_string(), parts[2])
                    } else if parts.len() == 2 {
                        ("0000".to_string(), parts[0].to_string(), parts[1])
                    } else {
                        current_is_gpu = false;
                        continue;
                    };

                    let slot_func_parts: Vec<&str> = slot_func.split('.').collect();
                    let (slot, function) = if slot_func_parts.len() == 2 {
                        (slot_func_parts[0].to_string(), slot_func_parts[1].to_string())
                    } else {
                        (slot_func.to_string(), "0".to_string())
                    };

                    // Parse vendor/device IDs from [vendor:device] format
                    let mut vendor_id = String::new();
                    let mut device_id = String::new();
                    let mut class_code = String::new();
                    let mut vendor_name = String::new();
                    let mut device_name = String::new();

                    // Extract class code
                    if let Some(start) = rest.find('[') {
                        if let Some(end) = rest[start + 1..].find(']') {
                            class_code = rest[start + 1..start + 1 + end].to_string();
                        }
                    }

                    // Extract vendor:device IDs
                    // lspci -nn outputs: ... [class]: Vendor Device [vendor:device] (rev) [subsys:subsys]
                    // We need the FIRST [xxxx:xxxx] of length 9 that contains ':',
                    // which is the vendor:device ID. The subsystem ID comes later.
                    let bytes = rest.as_bytes();
                    let mut vendor_device_pos = None;
                    for i in 0..bytes.len() {
                        if bytes[i] == b'[' {
                            if let Some(end) = rest[i + 1..].find(']') {
                                let content = &rest[i + 1..i + 1 + end];
                                if content.contains(':') && content.len() == 9 {
                                    vendor_device_pos = Some(i);
                                    break;
                                }
                            }
                        }
                    }

                    if let Some(pos) = vendor_device_pos {
                        if let Some(end) = rest[pos + 1..].find(']') {
                            let ids = &rest[pos + 1..pos + 1 + end];
                            let id_parts: Vec<&str> = ids.split(':').collect();
                            if id_parts.len() == 2 {
                                vendor_id = id_parts[0].to_string();
                                device_id = id_parts[1].to_string();
                            }
                        }
                    }

                    // Extract vendor and device names
                    // Format: "...: VendorName Corporation DeviceName [vendor:device]"
                    if let Some(pos) = vendor_device_pos {
                        let name_part = rest[..pos].trim();
                        // Remove class code part: "VGA compatible controller [0300]: "
                        let name_str = if let Some(colon_pos) = name_part.find(": ") {
                            &name_part[colon_pos + 2..]
                        } else {
                            name_part
                        };
                        // Strip trailing "(rev XX)" if present
                        let name_clean = if let Some(rev_pos) = name_str.rfind("(rev ") {
                            name_str[..rev_pos].trim()
                        } else {
                            name_str.trim()
                        };
                        // Known vendor prefixes to split on
                        let known_vendors = [
                            "NVIDIA Corporation ",
                            "Intel Corporation ",
                            "Advanced Micro Devices, Inc. ",
                            "AMD ",
                            "ASPEED Technology, Inc. ",
                            "Matrox Electronics Systems Ltd. ",
                        ];
                        let mut found_vendor = false;
                        for prefix in &known_vendors {
                            if let Some(stripped) = name_clean.strip_prefix(prefix) {
                                vendor_name = prefix.trim().to_string();
                                device_name = stripped.trim().to_string();
                                found_vendor = true;
                                break;
                            }
                        }
                        if !found_vendor {
                            // Fallback: use first word as vendor, rest as device
                            let name_parts: Vec<&str> = name_clean.splitn(2, ' ').collect();
                            if name_parts.len() >= 2 {
                                vendor_name = name_parts[0].to_string();
                                device_name = name_parts[1..].join(" ");
                            } else if !name_parts.is_empty() {
                                device_name = name_parts[0].to_string();
                            }
                        }
                    }

                    let full_pci = format!(
                        "{:04x}:{:02x}:{:02x}.{}",
                        u16::from_str_radix(&domain, 16).unwrap_or(0),
                        u8::from_str_radix(&bus, 16).unwrap_or(0),
                        u8::from_str_radix(&slot, 16).unwrap_or(0),
                        function
                    );

                    current_device = Some(GpuDevice {
                        domain,
                        bus,
                        slot,
                        function,
                        pci_address: full_pci,
                        vendor_id,
                        device_id,
                        vendor_name,
                        device_name,
                        class_code,
                        driver: String::new(),
                        iommu_group: String::new(),
                        can_passthrough: false,
                        passthrough_reason: String::new(),
                        is_boot_gpu: false,
                    });
                    current_is_gpu = true;
                } else {
                    // Not a GPU device, save previous if exists
                    if let Some(dev) = current_device.take() {
                        if current_is_gpu {
                            gpus.push(dev);
                        }
                    }
                    current_is_gpu = false;
                }
            }
        }

        // Don't forget the last device
        if let Some(dev) = current_device.take() {
            if current_is_gpu {
                gpus.push(dev);
            }
        }
    }

    // Enrich with IOMMU group info
    enrich_iommu_groups(&mut gpus);

    // Check passthrough feasibility
    check_passthrough_feasibility(&mut gpus);

    // Determine boot GPU
    determine_boot_gpu(&mut gpus);

    gpus
}

fn enrich_iommu_groups(gpus: &mut [GpuDevice]) {
    for gpu in gpus.iter_mut() {
        let path = format!(
            "/sys/bus/pci/devices/{}/iommu_group",
            gpu.pci_address
        );
        if let Ok(link) = std::fs::read_link(&path) {
            gpu.iommu_group = link
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
        }
    }
}

fn check_passthrough_feasibility(gpus: &mut [GpuDevice]) {
    let iommu_enabled = check_iommu_enabled();

    for gpu in gpus.iter_mut() {
        let mut reasons = Vec::new();
        let mut can_do = true;

        // Check IOMMU support
        if !iommu_enabled {
            can_do = false;
            reasons.push("IOMMU not enabled in kernel");
        }

        // Check if already bound to vfio-pci
        if gpu.driver == "vfio-pci" {
            gpu.can_passthrough = true;
            gpu.passthrough_reason = "Already bound to vfio-pci".to_string();
            continue;
        }

        // Check IOMMU group isolation
        if gpu.iommu_group.is_empty() {
            can_do = false;
            reasons.push("No IOMMU group found");
        }

        // Check if this is the boot GPU (primary display)
        if gpu.is_boot_gpu {
            can_do = false;
            reasons.push("Boot GPU - passing through will lose host display");
        }

        // Check if driver is removable (nvidia proprietary can be tricky)
        if gpu.driver == "nvidia" || gpu.driver == "nvidia_drm" {
            reasons.push("Using proprietary NVIDIA driver - needs unbind before passthrough");
        }

        if can_do {
            if reasons.is_empty() {
                gpu.passthrough_reason = "Ready for passthrough".to_string();
            } else {
                gpu.passthrough_reason = reasons.join("; ");
            }
        } else {
            gpu.passthrough_reason = reasons.join("; ");
        }

        gpu.can_passthrough = can_do;
    }
}

pub fn check_iommu_enabled() -> bool {
    // Check kernel command line for iommu parameters
    if let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline") {
        let has_intel = cmdline.contains("intel_iommu=on");
        let has_amd = cmdline.contains("amd_iommu=on") || cmdline.contains("iommu=pt");
        if has_intel || has_amd {
            return true;
        }
    }

    // Check dmesg for IOMMU
    if let Ok(output) = Command::new("dmesg")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("DMAR: IOMMU enabled") || text.contains("AMD-Vi: AMD-Vi enabled") {
            return true;
        }
    }

    false
}

fn determine_boot_gpu(gpus: &mut [GpuDevice]) {
    // Use the kernel's boot_vga sysfs attribute for accurate detection.
    // boot_vga == 1 means this is the GPU initialized by the firmware/BIOS
    // and used for the host console.
    for gpu in gpus.iter_mut() {
        let boot_vga_path = format!("/sys/bus/pci/devices/{}/boot_vga", gpu.pci_address);
        if let Ok(content) = std::fs::read_to_string(&boot_vga_path) {
            gpu.is_boot_gpu = content.trim() == "1";
        } else {
            // Fallback: VGA class [0300] is likely the boot GPU
            gpu.is_boot_gpu = gpu.class_code == "0300";
        }
    }
}

/// Check if a GPU currently has an active display output (connected monitor).
/// Used to prevent unbinding a GPU that the host is actively using.
pub fn is_gpu_display_active(gpu: &GpuDevice) -> bool {
    let drm_path = format!("/sys/bus/pci/devices/{}/drm", gpu.pci_address);
    let Ok(entries) = std::fs::read_dir(&drm_path) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("card") {
            let status_path = format!("/sys/class/drm/{}/status", name);
            if let Ok(status) = std::fs::read_to_string(&status_path) {
                if status.trim() == "connected" {
                    return true;
                }
            }
        }
    }
    false
}

/// Search common paths for a dumped VBIOS ROM for this PCI device.
/// Returns a validated and sanitized path if found.
pub fn find_vbios_rom(pci_address: &str) -> Option<String> {
    let vendor_path = format!("/sys/bus/pci/devices/{}/vendor", pci_address);
    let device_path = format!("/sys/bus/pci/devices/{}/device", pci_address);
    let vendor = std::fs::read_to_string(&vendor_path).ok()?.trim().to_string();
    let device = std::fs::read_to_string(&device_path).ok()?.trim().to_string();
    let vendor_clean = vendor.trim_start_matches("0x");
    let device_clean = device.trim_start_matches("0x");

    let candidates = [
        format!("/usr/share/vgabios/{}_{}.rom", vendor_clean, device_clean),
        format!("/usr/share/kvm/{}_{}.rom", vendor_clean, device_clean),
        format!("/var/lib/libvirt/vbios/{}_{}.rom", vendor_clean, device_clean),
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            // Validate the ROM file before returning the path
            if verify_vbios_rom(path).is_ok() {
                return Some(path.clone());
            }
        }
    }
    None
}

/// Validate a VBIOS ROM file for integrity and safety.
/// Checks file size, header signature, and path traversal.
pub fn verify_vbios_rom(path: &str) -> Result<(), String> {
    // Resolve symlinks and get canonical path
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve VBIOS path: {}", e))?;

    // Path traversal check: ensure file is in allowed directories
    let canon_str = canonical.to_string_lossy();
    if !VBIOS_ALLOWED_DIRS.iter().any(|d| canon_str.starts_with(d)) {
        return Err("VBIOS file outside allowed directories".to_string());
    }

    // Check file size (VBIOS typically 64KB-1MB)
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Cannot stat VBIOS file: {}", e))?;
    let size = metadata.len();
    if size < 65536 || size > 1048576 {
        return Err(format!(
            "VBIOS size suspicious: {} bytes (expected 64KB-1MB)",
            size
        ));
    }

    // Check ROM header signature ($VBIOS or $POST or UEFI signature)
    let mut file = std::fs::File::open(&canonical)
        .map_err(|e| format!("Cannot open VBIOS file: {}", e))?;
    let mut header = [0u8; 32];
    file.read_exact(&mut header)
        .map_err(|e| format!("Cannot read VBIOS header: {}", e))?;

    // Legacy BIOS ROM starts with 0x55 0xAA followed by "$BIOS" or "$VBIOS"
    // UEFI ROM has different signature
    let has_legacy_sig = header[0] == 0x55 && header[1] == 0xAA;
    let has_vbios_str = header.get(2..8).map_or(false, |s| s == b"$VBIOS" || s == b"$POST " || s == b"$POST\0");
    let has_pcie_sig = header.get(0..4).map_or(false, |s| s == b"PCIR");

    if !has_legacy_sig && !has_vbios_str && !has_pcie_sig {
        return Err("Invalid VBIOS header signature".to_string());
    }

    Ok(())
}

/// Sanitize a VBIOS ROM path - resolves symlinks and validates location.
pub fn sanitize_vbios_path(path: &str) -> Result<String, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve VBIOS path: {}", e))?;

    let canon_str = canonical.to_string_lossy().to_string();
    if !VBIOS_ALLOWED_DIRS.iter().any(|d| canon_str.starts_with(d)) {
        return Err("VBIOS path outside allowed directories".to_string());
    }

    Ok(canon_str)
}

/// Get all devices in the same IOMMU group as the given GPU
pub fn get_iommu_group_devices(pci_address: &str) -> Vec<String> {
    let mut devices = Vec::new();

    let iommu_link = format!("/sys/bus/pci/devices/{}/iommu_group", pci_address);
    let group_path = match std::fs::read_link(&iommu_link) {
        Ok(link) => link,
        Err(_) => return devices,
    };

    let devices_path = format!(
        "/sys/kernel/iommu_groups/{}/devices",
        group_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default()
    );

    if let Ok(entries) = std::fs::read_dir(&devices_path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                // Skip the GPU itself
                if name != pci_address {
                    devices.push(name.to_string());
                }
            }
        }
    }

    devices
}

/// Get the PCI class code for a device
pub fn get_pci_class_code(pci_address: &str) -> Option<String> {
    let class_path = format!("/sys/bus/pci/devices/{}/class", pci_address);
    std::fs::read_to_string(&class_path).ok().map(|c| c.trim().to_string())
}

/// Check if a device class code represents a critical system device
/// that should not be bound to vfio-pci
pub fn is_critical_device(pci_address: &str) -> bool {
    if let Some(class_code) = get_pci_class_code(pci_address) {
        // USB controllers: 0x0c03
        if class_code.starts_with("0x0c03") {
            return true;
        }
        // SATA/AHCI controllers: 0x0106
        if class_code.starts_with("0x0106") {
            return true;
        }
        // NVMe controllers: 0x0108
        if class_code.starts_with("0x0108") {
            return true;
        }
        // Network controllers: 0x0200
        if class_code.starts_with("0x0200") {
            return true;
        }
        // ISA bridge: 0x0601 (often critical for boot)
        if class_code.starts_with("0x0601") {
            return true;
        }
    }
    false
}

/// Get the IOMMU group number for a PCI device
#[allow(dead_code)]
pub fn get_iommu_group_number(pci_address: &str) -> Option<String> {
    let iommu_link = format!("/sys/bus/pci/devices/{}/iommu_group", pci_address);
    std::fs::read_link(&iommu_link)
        .ok()
        .and_then(|link| link.file_name().map(|f| f.to_string_lossy().to_string()))
}

/// Check if vfio-pci driver is loaded
pub fn is_vfio_loaded() -> bool {
    if let Ok(modules) = std::fs::read_to_string("/proc/modules") {
        modules.contains("vfio_pci")
    } else {
        false
    }
}

/// Check if a GPU is currently in use by any process on the host.
/// This prevents unbinding a GPU that has active processes, which could
/// cause system crashes or data loss.
pub fn is_gpu_in_use(pci_address: &str) -> bool {
    // Check for active DRM device usage
    let drm_path = format!("/sys/bus/pci/devices/{}/drm", pci_address);
    if let Ok(entries) = std::fs::read_dir(&drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("card") || name.starts_with("renderD") {
                let dev_path = format!("/dev/dri/{}", name);
                // Check if any process has this device open via /proc
                if is_device_open(&dev_path) {
                    return true;
                }
            }
        }
    }

    // Check for active NVIDIA usage (if nvidia-smi is available)
    if let Ok(output) = Command::new("nvidia-smi")
        .args(["-L"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains(pci_address) {
            // GPU is listed by nvidia-smi, check for active processes
            if let Ok(proc_output) = Command::new("nvidia-smi")
                .args(["--query-compute-apps=pid", "--format=csv,noheader"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
            {
                let proc_text = String::from_utf8_lossy(&proc_output.stdout);
                if !proc_text.trim().is_empty() {
                    return true;
                }
            }
        }
    }

    // Check for active IRQ on this device
    let irq_path = format!("/sys/bus/pci/devices/{}/msi_irqs", pci_address);
    if let Ok(entries) = std::fs::read_dir(&irq_path) {
        // If IRQs exist, device might be in use
        if entries.count() > 0 {
            return true;
        }
    }

    false
}

/// Check if a device file has any open file descriptors.
fn is_device_open(dev_path: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let proc_name = entry.file_name().to_string_lossy().to_string();
            // Only check numeric PIDs
            if proc_name.chars().all(|c| c.is_ascii_digit()) {
                let fd_path = format!("/proc/{}/fd", proc_name);
                if let Ok(fd_entries) = std::fs::read_dir(&fd_path) {
                    for fd_entry in fd_entries.flatten() {
                        if let Ok(target) = std::fs::read_link(&fd_entry.path()) {
                            if target.to_string_lossy() == dev_path {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Verify IOMMU group isolation - check if devices in the same group
/// are truly isolated or if there are unexpected devices sharing the group.
/// This helps detect faulty IOMMU implementations on some motherboards.
pub fn verify_iommu_group_isolation(iommu_group: &str) -> Result<Vec<String>, String> {
    let devices_path = format!("/sys/kernel/iommu_groups/{}/devices", iommu_group);
    let mut suspicious_devices = Vec::new();

    let entries = std::fs::read_dir(&devices_path)
        .map_err(|e| format!("Cannot read IOMMU group devices: {}", e))?;

    // Known device classes that should NOT share an IOMMU group with a GPU
    // If they do, the IOMMU isolation is broken
    let critical_classes = [
        "0x0c03", // USB controllers
        "0x0106", // SATA controllers
        "0x0108", // NVMe controllers
        "0x0200", // Network controllers
        "0x0601", // ISA bridges
        "0x0604", // PCI bridges (can indicate ACS not working)
        "0x0c05", // SMBus controllers
        "0x0600", // Host bridges
    ];

    for entry in entries.flatten() {
        if let Some(addr) = entry.file_name().to_str() {
            let class_path = format!("{}/{}/class", devices_path, addr);
            if let Ok(class_code) = std::fs::read_to_string(&class_path) {
                let class_clean = class_code.trim().to_string();
                for critical in &critical_classes {
                    if class_clean.starts_with(critical) {
                        suspicious_devices.push(addr.to_string());
                        break;
                    }
                }
            }
        }
    }

    if suspicious_devices.is_empty() {
        Ok(Vec::new())
    } else {
        Err(format!(
            "IOMMU group {} contains critical devices: {}. \
             IOMMU isolation may be broken. Consider enabling ACS override.",
            iommu_group,
            suspicious_devices.join(", ")
        ))
    }
}

/// Attempt a PCI Function Level Reset on a device.
/// Useful for recovering GPUs that refuse to initialize after a VM shutdown (NVIDIA reset bug).
/// Returns an error if reset fails, so callers can handle the situation appropriately.
pub fn reset_pci_device(pci_address: &str) -> Result<(), String> {
    let reset_path = format!("/sys/bus/pci/devices/{}/reset", pci_address);
    if !std::path::Path::new(&reset_path).exists() {
        return Err(format!("No reset sysfs node for {}", pci_address));
    }
    std::fs::write(&reset_path, "1")
        .map_err(|e| format!("Failed to reset {}: {}", pci_address, e))?;

    // Verify device is still accessible after reset
    let vendor_path = format!("/sys/bus/pci/devices/{}/vendor", pci_address);
    if std::fs::read_to_string(&vendor_path).is_err() {
        return Err(format!("Device {} became inaccessible after reset", pci_address));
    }

    Ok(())
}

/// Check if required tools are available
pub fn check_dependencies() -> Vec<String> {
    let mut missing = Vec::new();
    let tools = ["lspci", "virsh"];

    for tool in &tools {
        if which::which(tool).is_err() {
            missing.push(tool.to_string());
        }
    }

    missing
}
