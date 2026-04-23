use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;
use std::sync::Mutex;
use std::fs::File;
use std::io::Write;

/// Log file path for audit trail
const LOG_PATH: &str = "/var/log/gpupass.log";

/// Global lock to prevent concurrent VM operations
static VM_OPERATION_LOCK: Mutex<()> = Mutex::new(());

/// Acquire exclusive lock for VM operations.
fn acquire_vm_lock() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    VM_OPERATION_LOCK.lock().map_err(|e| format!("Failed to acquire VM operation lock: {}", e))
}

/// Log VM operations to audit log
fn log_vm_operation(action: &str, vm_name: &str, gpu_address: &str, success: bool, detail: &str) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let log_line = format!(
        "{} [{}] vm={} gpu={} success={} detail={}\n",
        timestamp, action, vm_name, gpu_address, success, detail
    );

    if let Ok(mut file) = File::options().create(true).append(true).open(LOG_PATH) {
        let _ = file.write_all(log_line.as_bytes());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub id: String,
    pub name: String,
    pub state: VmState,
    pub assigned_gpus: Vec<String>, // PCI addresses of assigned GPUs
    pub xml_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmState {
    Running,
    ShutOff,
    Paused,
    Saved,
    Other(String),
}

impl std::fmt::Display for VmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmState::Running => write!(f, "running"),
            VmState::ShutOff => write!(f, "shut off"),
            VmState::Paused => write!(f, "paused"),
            VmState::Saved => write!(f, "saved"),
            VmState::Other(s) => write!(f, "{}", s),
        }
    }
}

impl VmInfo {
    pub fn is_running(&self) -> bool {
        self.state == VmState::Running
    }
}

/// List all VMs: first via virsh, then scan XML files as fallback.
/// Also auto-registers any unregistered VMs found via XML scan.
pub fn detect_vms() -> Vec<VmInfo> {
    let mut vms = Vec::new();
    let mut known_names = std::collections::HashSet::new();

    // Method 1: Get VMs from virsh list --all
    if let Ok(output) = Command::new("virsh")
        .args(["list", "--all"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('-') || line.starts_with("Id") || line.starts_with("状态") {
                continue;
            }

            let parts: Vec<&str> = line.splitn(3, ' ').flat_map(|s| s.split_whitespace()).collect();
            if parts.len() >= 3 {
                let id = parts[0].to_string();
                let name = parts[1].to_string();
                let state_str = parts[2..].join(" ");

                let state = match state_str.as_str() {
                    "running" => VmState::Running,
                    "shut off" => VmState::ShutOff,
                    "paused" => VmState::Paused,
                    "saved" => VmState::Saved,
                    other => VmState::Other(other.to_string()),
                };

                known_names.insert(name.clone());
                vms.push(VmInfo {
                    id,
                    name,
                    state,
                    assigned_gpus: Vec::new(),
                    xml_path: None,
                });
            } else if parts.len() == 2 {
                let id = parts[0].to_string();
                let name = parts[1].to_string();
                known_names.insert(name.clone());
                vms.push(VmInfo {
                    id,
                    name,
                    state: VmState::Other("unknown".to_string()),
                    assigned_gpus: Vec::new(),
                    xml_path: None,
                });
            }
        }
    }

    // Method 2: Scan /etc/libvirt/qemu/*.xml for unregistered VMs
    let xml_dir = "/etc/libvirt/qemu";
    let mut unregistered: Vec<(String, String)> = Vec::new(); // (name, xml_path)

    if let Ok(entries) = std::fs::read_dir(xml_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("xml") {
                continue;
            }
            // Skip backup files
            if path.to_string_lossy().ends_with(".bak") || path.to_string_lossy().ends_with(".gpupass.bak") {
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(vm_name) = extract_vm_name_from_xml(&content) {
                    if !known_names.contains(&vm_name) {
                        known_names.insert(vm_name.clone());
                        unregistered.push((vm_name, path.to_string_lossy().to_string()));
                    }
                }
            }
        }
    }

    // Auto-register unregistered VMs via virsh define
    for (vm_name, xml_path) in &unregistered {
        let registered = auto_define_vm(xml_path);
        let state = if registered {
            VmState::ShutOff
        } else {
            VmState::ShutOff // Still show it even if define failed
        };
        vms.push(VmInfo {
            id: "-".to_string(),
            name: vm_name.clone(),
            state,
            assigned_gpus: Vec::new(),
            xml_path: Some(xml_path.clone()),
        });
    }

    // Enrich with GPU assignment info
    for vm in vms.iter_mut() {
        vm.assigned_gpus = get_vm_assigned_gpus(&vm.name, vm.xml_path.as_deref());
        if vm.xml_path.is_none() {
            vm.xml_path = get_vm_xml_path(&vm.name);
        }
    }

    vms
}

/// Auto-register a VM XML file with virsh define
fn auto_define_vm(xml_path: &str) -> bool {
    if let Ok(output) = Command::new("virsh")
        .args(["define", xml_path])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        output.status.success()
    } else {
        false
    }
}

/// Extract <name>...</name> from libvirt XML
fn extract_vm_name_from_xml(xml: &str) -> Option<String> {
    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<name>") && trimmed.ends_with("</name>") {
            let name = trimmed
                .strip_prefix("<name>")?
                .strip_suffix("</name>")?;
            return Some(name.trim().to_string());
        }
    }
    None
}

/// Get PCI devices currently assigned to a VM from its XML definition
fn get_vm_assigned_gpus(vm_name: &str, xml_path: Option<&str>) -> Vec<String> {
    let mut gpus = Vec::new();

    // Try reading from XML file first, then fall back to virsh dumpxml
    let xml = if let Some(path) = xml_path {
        std::fs::read_to_string(path).unwrap_or_default()
    } else if let Ok(output) = Command::new("virsh")
        .args(["dumpxml", vm_name])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        return gpus;
    };

        // Look for <hostdev> entries with PCI passthrough.
        // Only collect <address> elements that are inside a <hostdev type='pci'> block.
        let mut in_hostdev = false;
        let mut hostdev_is_pci = false;
        for line in xml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<hostdev") {
                in_hostdev = true;
                hostdev_is_pci = trimmed.contains("type='pci'") || trimmed.contains("type=\"pci\"");
                continue;
            }
            if trimmed == "</hostdev>" {
                in_hostdev = false;
                hostdev_is_pci = false;
                continue;
            }
            if in_hostdev
                && hostdev_is_pci
                && trimmed.starts_with("<address")
                && trimmed.contains("domain=")
                && trimmed.contains("bus=")
            {
                let domain = extract_xml_attr(&line, "domain");
                let bus = extract_xml_attr(&line, "bus");
                let slot = extract_xml_attr(&line, "slot");
                let function = extract_xml_attr(&line, "function");

                if let (Some(d), Some(b), Some(s), Some(f)) = (&domain, &bus, &slot, &function) {
                    let d_dec = d.trim_start_matches("0x").parse::<u16>().unwrap_or(0);
                    let b_dec = b.trim_start_matches("0x").parse::<u8>().unwrap_or(0);
                    let s_dec = s.trim_start_matches("0x").parse::<u8>().unwrap_or(0);
                    let f_dec = f.trim_start_matches("0x").parse::<u8>().unwrap_or(0);

                    let pci_addr = format!("{:04x}:{:02x}:{:02x}.{}", d_dec, b_dec, s_dec, f_dec);
                    gpus.push(pci_addr);
                }
            }
        }

    gpus
}

fn extract_xml_attr(line: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}='", attr);
    if let Some(start) = line.find(&pattern) {
        let value_start = start + pattern.len();
        if let Some(end) = line[value_start..].find('\'') {
            return Some(line[value_start..value_start + end].to_string());
        }
    }
    // Also try double quotes
    let pattern2 = format!("{}=\"", attr);
    if let Some(start) = line.find(&pattern2) {
        let value_start = start + pattern2.len();
        if let Some(end) = line[value_start..].find('"') {
            return Some(line[value_start..value_start + end].to_string());
        }
    }
    None
}

/// Get the XML definition file path for a VM
fn get_vm_xml_path(vm_name: &str) -> Option<String> {
    // Default libvirt path
    let default_path = format!("/etc/libvirt/qemu/{}.xml", vm_name);
    if std::path::Path::new(&default_path).exists() {
        return Some(default_path);
    }

    // Try virsh dumpxml to see if VM exists
    if let Ok(output) = Command::new("virsh")
        .args(["dumpxml", vm_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        if output.status.success() {
            return None; // VM exists but we don't know the file path
        }
    }

    None
}

/// Generate hostdev XML fragment for GPU passthrough
/// Validates and sanitizes the VBIOS ROM path before use.
pub fn generate_hostdev_xml(pci_address: &str, rom_path: Option<&str>) -> String {
    let parts: Vec<&str> = pci_address.split(&[':', '.'][..]).collect();
    if parts.len() != 4 {
        return String::new();
    }

    // Parse and convert to hex format for libvirt
    let domain = u16::from_str_radix(parts[0], 16).unwrap_or(0);
    let bus = u8::from_str_radix(parts[1], 16).unwrap_or(0);
    let slot = u8::from_str_radix(parts[2], 16).unwrap_or(0);
    let function = u8::from_str_radix(parts[3], 16).unwrap_or(0);

    // Sanitize VBIOS path if provided
    let sanitized_rom_path = rom_path.and_then(|p| {
        match crate::gpu::sanitize_vbios_path(p) {
            Ok(path) => Some(path),
            Err(e) => {
                eprintln!("Warning: VBIOS path sanitization failed: {}", e);
                None
            }
        }
    });

    let rom_xml = sanitized_rom_path
        .as_ref()
        .map(|p| format!("\n      <rom bar='on' file='{}'/>", p))
        .unwrap_or_default();

    format!(
        r#"<hostdev mode='subsystem' type='pci' managed='yes'>
  <source>
    <address domain='0x{:04x}' bus='0x{:02x}' slot='0x{:02x}' function='0x{:02x}'/>
  </source>{}
</hostdev>"#,
        domain, bus, slot, function, rom_xml
    )
}

/// Ensure the VM XML contains features required for NVIDIA GPUs to avoid Error 43.
fn ensure_nvidia_vm_features(vm_name: &str) -> Result<(), String> {
    let output = Command::new("virsh")
        .args(["dumpxml", vm_name])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("virsh dumpxml failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "virsh dumpxml: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mut xml = String::from_utf8_lossy(&output.stdout).to_string();

    // Check if features already exist properly
    let has_hyperv_vendor = xml.contains("vendor_id") && xml.contains("hyperv");
    let has_kvm_hidden = xml.contains("<kvm>") && xml.contains("hidden");

    if has_hyperv_vendor && has_kvm_hidden {
        return Ok(());
    }

    // Check if <features> section exists
    if !xml.contains("<features>") || !xml.contains("</features>") {
        return Err("VM XML does not have a <features> section - cannot inject NVIDIA workarounds".to_string());
    }

    // Inject <kvm hidden> if missing - check if kvm section already exists
    if !has_kvm_hidden {
        // Check if there's already a <kvm> section without hidden
        if xml.contains("<kvm>") && xml.contains("</kvm>") {
            // Replace existing kvm section
            if let Some(kvm_start) = xml.find("<kvm>") {
                if let Some(kvm_end) = xml[kvm_start..].find("</kvm>") {
                    let kvm_end_pos = kvm_start + kvm_end + "</kvm>".len();
                    xml.replace_range(kvm_start..kvm_end_pos, "<kvm>\n    <hidden state='on'/>\n  </kvm>");
                }
            }
        } else if let Some(pos) = xml.find("</features>") {
            xml.insert_str(pos, "  <kvm>\n    <hidden state='on'/>\n  </kvm>\n  ");
        }
    }

    // Inject <hyperv vendor_id> if missing - check if hyperv section already exists
    if !has_hyperv_vendor {
        // Check if there's already a <hyperv> section without vendor_id
        if xml.contains("<hyperv>") && xml.contains("</hyperv>") && !xml.contains("vendor_id") {
            // Replace existing hyperv section
            if let Some(hyperv_start) = xml.find("<hyperv>") {
                if let Some(hyperv_end) = xml[hyperv_start..].find("</hyperv>") {
                    let hyperv_end_pos = hyperv_start + hyperv_end + "</hyperv>".len();
                    xml.replace_range(hyperv_start..hyperv_end_pos,
                        "<hyperv>\n    <vendor_id state='on' value='1234567890ab'/>\n  </hyperv>");
                }
            }
        } else if let Some(pos) = xml.find("</features>") {
            xml.insert_str(
                pos,
                "  <hyperv>\n    <vendor_id state='on' value='1234567890ab'/>\n  </hyperv>\n  ",
            );
        }
    }

    let temp_path = format!("/tmp/gpupass_nvidia_{}.xml", vm_name);
    std::fs::write(&temp_path, &xml)
        .map_err(|e| format!("Failed to write temp XML: {}", e))?;

    let output = Command::new("virsh")
        .args(["define", &temp_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("virsh define failed: {}", e))?;

    let _ = std::fs::remove_file(&temp_path);

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "virsh define: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn attach_hostdev_xml_to_vm(vm_name: &str, hostdev_xml: &str) -> Result<String, String> {
    let mut temp_file = NamedTempFile::new()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::Write::write_all(&mut temp_file, hostdev_xml.as_bytes())
        .map_err(|e| format!("Failed to write temp XML: {}", e))?;
    let temp_path = temp_file.path().to_str()
        .ok_or("Failed to get temp file path")?;

    let output = Command::new("virsh")
        .args(["attach-device", vm_name, temp_path, "--persistent"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run virsh: {}", e))?;

    // Temp file is automatically cleaned up when dropped
    drop(temp_file);

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "virsh attach-device: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn detach_hostdev_xml_from_vm(vm_name: &str, hostdev_xml: &str) -> Result<String, String> {
    let mut temp_file = NamedTempFile::new()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    std::io::Write::write_all(&mut temp_file, hostdev_xml.as_bytes())
        .map_err(|e| format!("Failed to write temp XML: {}", e))?;
    let temp_path = temp_file.path().to_str()
        .ok_or("Failed to get temp file path")?;

    let output = Command::new("virsh")
        .args(["detach-device", vm_name, temp_path, "--persistent"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run virsh: {}", e))?;

    // Temp file is automatically cleaned up when dropped
    drop(temp_file);

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "virsh detach-device: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Apply GPU passthrough to a VM by attaching the device.
/// Attaches the GPU and all other devices in the same IOMMU group.
/// If the VM is registered in libvirt, use virsh attach-device.
/// If not, inject the hostdev XML directly into the VM's XML file.
pub fn attach_gpu_to_vm(vm_name: &str, gpu: &crate::gpu::GpuDevice, all_vms: &[VmInfo]) -> Result<String, String> {
    // Acquire exclusive lock
    let _lock = acquire_vm_lock()?;

    // Collect all PCI devices in the same IOMMU group
    let mut all_devices = vec![gpu.pci_address.clone()];
    let group_devices = crate::gpu::get_iommu_group_devices(&gpu.pci_address);
    all_devices.extend(group_devices);

    // Collect warnings for the user
    let mut warnings = Vec::new();

    // Check for GPU conflicts with other VMs (warning only, user decides)
    let conflict_info = check_gpu_conflicts(&gpu.pci_address, all_vms);
    if !conflict_info.conflicting_vms.is_empty() {
        warnings.push(crate::lang::t("warn.vm_conflict")
            .replacen("{}", &gpu.pci_address, 1)
            .replacen("{}", &conflict_info.conflicting_vms.join(", "), 1));
    }

    // Check if this GPU is used for host display (warning only, user decides)
    if conflict_info.is_host_display_gpu {
        warnings.push(crate::lang::t("warn.host_display_gpu").replace("{}", &gpu.pci_address));
    }

    // Check conflict and host display for all IOMMU group devices
    for pci_addr in &all_devices {
        if pci_addr == &gpu.pci_address {
            continue; // Already checked above
        }
        let device_conflict = check_gpu_conflicts(pci_addr, all_vms);
        if !device_conflict.conflicting_vms.is_empty() {
            warnings.push(crate::lang::t("warn.iommu_vm_conflict")
                .replacen("{}", pci_addr, 1)
                .replacen("{}", &device_conflict.conflicting_vms.join(", "), 1));
        }
        if device_conflict.is_host_display_gpu {
            warnings.push(crate::lang::t("warn.iommu_host_display").replace("{}", pci_addr));
        }
    }

    // Log warnings
    for warning in &warnings {
        log_vm_operation("attach_gpu", vm_name, &gpu.pci_address, true, warning);
    }

    // For NVIDIA GPUs, ensure Error 43 workarounds are configured
    let is_nvidia = gpu.vendor_name.to_lowercase().contains("nvidia")
        || gpu.driver.contains("nvidia")
        || gpu.device_name.to_lowercase().contains("nvidia");
    if is_nvidia {
        if let Err(e) = ensure_nvidia_vm_features(vm_name) {
            log_vm_operation("attach_gpu", vm_name, &gpu.pci_address, false, &format!("NVIDIA features: {}", e));
            return Err(format!("Failed to configure NVIDIA VM features: {}", e));
        }
    }

    // Check VM state to prevent unsafe operations
    let vm_running = is_vm_running(vm_name);

    let mut attached = Vec::new();
    let mut use_xml_injection = false;

    for pci_addr in &all_devices {
        let rom_path = crate::gpu::find_vbios_rom(pci_addr);
        let hostdev_xml = generate_hostdev_xml(pci_addr, rom_path.as_deref());

        if use_xml_injection {
            // Safety: refuse XML injection on running VMs
            if vm_running {
                log_vm_operation("attach_gpu", vm_name, pci_addr, false, "XML injection on running VM");
                return Err(format!(
                    "Cannot inject XML for {} while VM '{}' is running. \
                     Shut off the VM first to modify persistent configuration.",
                    pci_addr, vm_name
                ));
            }

            let vm_xml_path = format!("/etc/libvirt/qemu/{}.xml", vm_name);
            if std::path::Path::new(&vm_xml_path).exists() {
                inject_hostdev_into_xml(&vm_xml_path, &hostdev_xml)?;
                attached.push(format!("{}: injected into XML", pci_addr));
            } else {
                log_vm_operation("attach_gpu", vm_name, pci_addr, false, "no XML file found");
                return Err(format!(
                    "Cannot inject {}: no XML found for VM {}",
                    pci_addr, vm_name
                ));
            }
        } else {
            match attach_hostdev_xml_to_vm(vm_name, &hostdev_xml) {
                Ok(msg) => attached.push(format!("{}: {}", pci_addr, msg)),
                Err(_) => {
                    // virsh failed - check if safe to use XML injection
                    if vm_running {
                        log_vm_operation("attach_gpu", vm_name, pci_addr, false, "virsh failed on running VM");
                        return Err(format!(
                            "Failed to attach {} via virsh and VM '{}' is running. \
                             Shut off the VM to use XML injection.",
                            pci_addr, vm_name
                        ));
                    }

                    let vm_xml_path = format!("/etc/libvirt/qemu/{}.xml", vm_name);
                    if std::path::Path::new(&vm_xml_path).exists() {
                        inject_hostdev_into_xml(&vm_xml_path, &hostdev_xml)?;
                        attached.push(format!("{}: injected into XML", pci_addr));
                        use_xml_injection = true;
                    } else {
                        log_vm_operation("attach_gpu", vm_name, pci_addr, false, "virsh failed and no XML");
                        return Err(format!(
                            "Failed to attach {} via virsh and no XML file found for VM '{}'",
                            pci_addr, vm_name
                        ));
                    }
                }
            }
        }
    }

    log_vm_operation("attach_gpu", vm_name, &gpu.pci_address, true, &format!("attached {} devices", attached.len()));

    let mut result_msg = format!(
        "Attached {} device(s): {}",
        attached.len(),
        attached.join(", ")
    );

    // Append warnings to the result message so user sees them
    if !warnings.is_empty() {
        result_msg.push_str("\n\n--- Warnings ---\n");
        for w in &warnings {
            result_msg.push_str(w);
            result_msg.push('\n');
        }
    }

    Ok(result_msg)
}

/// Check if a VM is currently running
fn is_vm_running(vm_name: &str) -> bool {
    if let Ok(output) = Command::new("virsh")
        .args(["domstate", vm_name])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        let state = String::from_utf8_lossy(&output.stdout).to_string();
        state.trim() == "running"
    } else {
        false
    }
}

/// Inject a hostdev XML block into a VM's XML file, before </devices>
fn inject_hostdev_into_xml(xml_path: &str, hostdev_xml: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(xml_path)
        .map_err(|e| format!("Failed to read {}: {}", xml_path, e))?;

    // Check if this specific PCI device is already attached by looking for its address
    // More robust check: verify the address is inside a <hostdev> block
    if let Some(addr_line) = hostdev_xml.lines().find(|l| l.contains("<address domain=")) {
        let domain = extract_xml_attr(addr_line, "domain");
        let bus = extract_xml_attr(addr_line, "bus");
        let slot = extract_xml_attr(addr_line, "slot");
        let function = extract_xml_attr(addr_line, "function");
        if let (Some(d), Some(b), Some(s), Some(f)) = (&domain, &bus, &slot, &function) {
            // Check if this address exists in any hostdev block
            let mut in_hostdev = false;
            let mut hostdev_block = String::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("<hostdev") {
                    in_hostdev = true;
                    hostdev_block.clear();
                }
                if in_hostdev {
                    hostdev_block.push_str(trimmed);
                    if trimmed == "</hostdev>" {
                        // Check if this hostdev block contains our PCI address
                        if hostdev_block.contains(&format!("domain='{}'", d))
                            && hostdev_block.contains(&format!("bus='{}'", b))
                            && hostdev_block.contains(&format!("slot='{}'", s))
                            && hostdev_block.contains(&format!("function='{}'", f))
                        {
                            return Err(format!(
                                "Device with address domain={} bus={} slot={} func={} already exists in this VM's XML",
                                d, b, s, f
                            ));
                        }
                        in_hostdev = false;
                    }
                }
            }
        }
    }

    // Insert before </devices>
    let new_content = if let Some(pos) = content.find("</devices>") {
        format!("{}    {}\n</devices>{}", &content[..pos], hostdev_xml, &content[pos + "</devices>".len()..])
    } else {
        return Err("Could not find </devices> in VM XML".to_string());
    };

    // Backup original with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = format!("{}.gpupass.bak.{}", xml_path, timestamp);
    std::fs::copy(xml_path, &backup_path)
        .map_err(|e| format!("Failed to backup XML: {}", e))?;

    std::fs::write(xml_path, &new_content)
        .map_err(|e| format!("Failed to write VM XML: {}", e))?;

    Ok(())
}

/// Detach a GPU and all devices in its IOMMU group from a VM
pub fn detach_gpu_from_vm(vm_name: &str, gpu: &crate::gpu::GpuDevice, _all_vms: &[VmInfo]) -> Result<String, String> {
    // Acquire exclusive lock
    let _lock = acquire_vm_lock()?;

    let mut all_devices = vec![gpu.pci_address.clone()];
    let group_devices = crate::gpu::get_iommu_group_devices(&gpu.pci_address);
    all_devices.extend(group_devices);

    // Check VM state to prevent unsafe operations
    let vm_running = is_vm_running(vm_name);

    let mut detached = Vec::new();
    let mut use_xml_removal = false;

    for pci_addr in &all_devices {
        let hostdev_xml = generate_hostdev_xml(pci_addr, None);

        if use_xml_removal {
            // Safety: refuse XML modification on running VMs
            if vm_running {
                log_vm_operation("detach_gpu", vm_name, pci_addr, false, "XML removal on running VM");
                return Err(format!(
                    "Cannot remove XML for {} while VM '{}' is running. \
                     Shut off the VM first to modify persistent configuration.",
                    pci_addr, vm_name
                ));
            }

            let vm_xml_path = format!("/etc/libvirt/qemu/{}.xml", vm_name);
            if std::path::Path::new(&vm_xml_path).exists() {
                remove_hostdev_from_xml(&vm_xml_path, pci_addr)?;
                detached.push(format!("{}: removed from XML", pci_addr));
            } else {
                log_vm_operation("detach_gpu", vm_name, pci_addr, false, "no XML file found");
                return Err(format!(
                    "Cannot remove {}: no XML found for VM {}",
                    pci_addr, vm_name
                ));
            }
        } else {
            match detach_hostdev_xml_from_vm(vm_name, &hostdev_xml) {
                Ok(msg) => detached.push(format!("{}: {}", pci_addr, msg)),
                Err(_) => {
                    let vm_xml_path = format!("/etc/libvirt/qemu/{}.xml", vm_name);
                    if std::path::Path::new(&vm_xml_path).exists() {
                        // Check if safe to use XML removal
                        if vm_running {
                            log_vm_operation("detach_gpu", vm_name, pci_addr, false, "virsh failed on running VM");
                            return Err(format!(
                                "Failed to detach {} via virsh and VM '{}' is running. \
                                 Shut off the VM to use XML modification.",
                                pci_addr, vm_name
                            ));
                        }

                        remove_hostdev_from_xml(&vm_xml_path, pci_addr)?;
                        detached.push(format!("{}: removed from XML", pci_addr));
                        use_xml_removal = true;
                    } else {
                        log_vm_operation("detach_gpu", vm_name, pci_addr, false, "virsh failed and no XML");
                        return Err(format!(
                            "Failed to detach {} from {} and no XML found",
                            pci_addr, vm_name
                        ));
                    }
                }
            }
        }
    }

    log_vm_operation("detach_gpu", vm_name, &gpu.pci_address, true, &format!("detached {} devices", detached.len()));

    Ok(format!("Detached {} device(s): {}", detached.len(), detached.join(", ")))
}

/// Remove a hostdev entry from a VM's XML file by PCI address
fn remove_hostdev_from_xml(xml_path: &str, pci_address: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(xml_path)
        .map_err(|e| format!("Failed to read {}: {}", xml_path, e))?;

    let parts: Vec<&str> = pci_address.split(&[':', '.'][..]).collect();
    if parts.len() != 4 {
        return Err(format!("Invalid PCI address: {}", pci_address));
    }

    // Convert to the format used in XML attributes (must match generate_hostdev_xml)
    let domain_val = format!("0x{:04x}", u16::from_str_radix(parts[0], 16).unwrap_or(0));
    let bus_val = format!("0x{:02x}", u8::from_str_radix(parts[1], 16).unwrap_or(0));
    let slot_val = format!("0x{:02x}", u8::from_str_radix(parts[2], 16).unwrap_or(0));
    let func_val = format!("0x{:02x}", u8::from_str_radix(parts[3], 16).unwrap_or(0));

    // Find and remove the hostdev block containing this PCI address
    let mut new_content = String::new();
    let mut in_hostdev = false;
    let mut hostdev_block = String::new();
    let mut found = false;

    for line in content.lines() {
        if line.trim().starts_with("<hostdev") {
            in_hostdev = true;
            hostdev_block.clear();
        }

        if in_hostdev {
            hostdev_block.push_str(line);
            hostdev_block.push('\n');

            if line.trim() == "</hostdev>" {
                in_hostdev = false;
                // Check if this block contains our PCI address
                if hostdev_block.contains(&domain_val)
                    && hostdev_block.contains(&bus_val)
                    && hostdev_block.contains(&slot_val)
                    && hostdev_block.contains(&func_val)
                {
                    found = true;
                    // Skip this block (don't add to new_content)
                } else {
                    new_content.push_str(&hostdev_block);
                }
                hostdev_block.clear();
            }
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    if !found {
        return Err(format!("GPU {} not found in {}", pci_address, xml_path));
    }

    // Backup and write with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = format!("{}.gpupass.bak.{}", xml_path, timestamp);
    std::fs::copy(xml_path, &backup_path)
        .map_err(|e| format!("Failed to backup XML: {}", e))?;

    std::fs::write(xml_path, new_content.trim_end())
        .map_err(|e| format!("Failed to write VM XML: {}", e))?;

    Ok(())
}

/// Result of GPU conflict check
#[derive(Debug)]
#[allow(dead_code)]
pub struct GpuConflictInfo {
    pub pci_address: String,
    pub conflicting_vms: Vec<String>, // VMs that have this GPU assigned (running or configured)
    pub is_host_display_gpu: bool,    // Whether this GPU is used for host display
    pub warnings: Vec<String>,
}

/// Check if a GPU is being used by any other VM (running or configured).
/// This helps prevent users from accidentally assigning the same GPU to multiple VMs.
pub fn check_gpu_conflicts(gpu_address: &str, all_vms: &[VmInfo]) -> GpuConflictInfo {
    let mut conflicting_vms = Vec::new();
    let mut warnings = Vec::new();

    for vm in all_vms {
        // Check if GPU is in this VM's assigned list
        if vm.assigned_gpus.iter().any(|addr| {
            normalize_pci_address(addr) == normalize_pci_address(gpu_address)
        }) {
            if vm.is_running() {
                warnings.push(format!(
                    "GPU {} is actively assigned to running VM '{}'",
                    gpu_address, vm.name
                ));
            }
            conflicting_vms.push(vm.name.clone());
        }
    }

    // Check if this GPU is the host's display GPU
    let is_host_display_gpu = is_host_display_gpu(gpu_address);
    if is_host_display_gpu {
        warnings.push(format!(
            "GPU {} is the host's primary display GPU - passing through will lose host display",
            gpu_address
        ));
    }

    GpuConflictInfo {
        pci_address: gpu_address.to_string(),
        conflicting_vms,
        is_host_display_gpu,
        warnings,
    }
}

/// Check if a GPU is being used as the host's primary display.
/// This is a more comprehensive check than just boot_vga.
fn is_host_display_gpu(pci_address: &str) -> bool {
    // Check 1: boot_vga attribute (most reliable indicator)
    let boot_vga_path = format!("/sys/bus/pci/devices/{}/boot_vga", pci_address);
    if let Ok(content) = std::fs::read_to_string(&boot_vga_path) {
        if content.trim() == "1" {
            return true;
        }
    }

    // Check 2: Check if this specific GPU has an active DRM connector with status "connected"
    let drm_path = format!("/sys/bus/pci/devices/{}/drm", pci_address);
    if let Ok(entries) = std::fs::read_dir(&drm_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("card") && !name.contains("renderD") {
                let status_path = format!("/sys/class/drm/{}/status", name);
                if let Ok(status) = std::fs::read_to_string(&status_path) {
                    if status.trim() == "connected" {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Normalize PCI address for comparison (handles different formatting)
fn normalize_pci_address(addr: &str) -> String {
    let parts: Vec<&str> = addr.split(&[':', '.'][..]).collect();
    if parts.len() == 4 {
        if let (Ok(d), Ok(b), Ok(s), Ok(f)) = (
            u16::from_str_radix(parts[0], 16),
            u8::from_str_radix(parts[1], 16),
            u8::from_str_radix(parts[2], 16),
            parts[3].parse::<u8>(),
        ) {
            return format!("{:04x}:{:02x}:{:02x}.{}", d, b, s, f);
        }
    }
    addr.to_string()
}

/// Get a summary of all GPU assignments across all VMs
#[allow(dead_code)]
pub fn get_gpu_assignment_summary(all_vms: &[VmInfo]) -> std::collections::HashMap<String, Vec<String>> {
    let mut summary = std::collections::HashMap::new();

    for vm in all_vms {
        for gpu_addr in &vm.assigned_gpus {
            let normalized = normalize_pci_address(gpu_addr);
            summary
                .entry(normalized)
                .or_insert_with(Vec::new)
                .push(vm.name.clone());
        }
    }

    summary
}
