use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Lang {
    En,
    Zh,
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lang::En => write!(f, "English"),
            Lang::Zh => write!(f, "中文"),
        }
    }
}

static LANG: RwLock<Lang> = RwLock::new(Lang::En);

pub fn set_lang(lang: Lang) {
    if let Ok(mut lock) = LANG.write() {
        *lock = lang;
    }
}

pub fn lang() -> Lang {
    match LANG.read() {
        Ok(lock) => *lock,
        Err(_) => Lang::En,
    }
}

/// Get translated text by key
pub fn t(key: &str) -> String {
    match lang() {
        Lang::Zh => t_zh(key),
        Lang::En => t_en(key),
    }
}

fn t_en(key: &str) -> String {
    match key {
        // Header
        "app.title" => " gpupass - GPU Passthrough Manager ".to_string(),
        "iommu.enabled" => " IOMMU: ENABLED ".to_string(),
        "iommu.disabled" => " IOMMU: DISABLED ".to_string(),

        // GPU panel
        "gpu.panel_title" => " GPU List ".to_string(),
        "gpu.boot" => " [BOOT]".to_string(),
        "gpu.no_driver" => "no driver".to_string(),

        // VM panel
        "vm.panel_title" => " Virtual Machines ".to_string(),
        "vm.no_gpu" => "No GPU".to_string(),
        "vm.gpu_label" => "GPU: ".to_string(),

        // Actions
        "action.bind_vfio" => "Bind GPU to vfio-pci".to_string(),
        "action.unbind_vfio" => "Unbind GPU from vfio-pci".to_string(),
        "action.attach_vm" => "Attach GPU to VM".to_string(),
        "action.detach_vm" => "Detach GPU from VM".to_string(),
        "action.auto_passthrough" => "Auto Passthrough (full setup)".to_string(),
        "action.apply_config" => "Apply VFIO config to system".to_string(),
        "action.back" => "Back".to_string(),
        "action.title" => " Actions ".to_string(),

        // Assign GPU
        "assign.title" => " Assign GPU to VM: ".to_string(),
        "assign.none" => "None (remove GPU assignment)".to_string(),
        "assign.select_title" => " Select GPU ".to_string(),

        // Confirm
        "confirm.title" => " Confirm ".to_string(),
        "confirm.yes" => " [y] Yes ".to_string(),
        "confirm.no" => " [n] No ".to_string(),
        "confirm.bind_vfio" => "Bind {} ({}) to vfio-pci?\nThis will detach it from the {} driver.".to_string(),
        "confirm.unbind_vfio" => "Unbind {} ({}) from vfio-pci?\nIt will be returned to the {} driver.".to_string(),
        "confirm.attach_gpu" => "Attach {} ({}) to VM \"{}\"?".to_string(),
        "confirm.detach_gpu" => "Detach {} ({}) from VM \"{}\"?".to_string(),
        "confirm.apply_config" => "Apply VFIO configuration to /etc/modprobe.d/vfio.conf?\nThis will update initramfs and require a reboot.".to_string(),

        // Messages
        "msg.title_ok" => " Result ".to_string(),
        "msg.title_err" => " Error ".to_string(),
        "msg.close" => " [Enter/Esc] Close ".to_string(),
        "msg.no_gpu_vm" => "No GPUs or VMs available".to_string(),
        "msg.no_vms" => "No VMs found. Create a VM first.".to_string(),
        "msg.removed_gpu" => "Removed GPU assignment from {}".to_string(),
        "msg.success" => "Success: {}".to_string(),
        "msg.error" => "Error: {}".to_string(),
        "msg.ok" => "[OK] {}".to_string(),
        "msg.warn" => "[WARN] {}".to_string(),
        "msg.err" => "[ERR] {}".to_string(),
        "msg.reboot" => "[REBOOT] {}".to_string(),
        "msg.initramfs_warn" => "{}\nWarning: initramfs update failed: {}".to_string(),
        "msg.initramfs_ok" => "{}\n{}".to_string(),

        // Footer
        "footer.navigate" => " Navigate ".to_string(),
        "footer.select" => " Select ".to_string(),
        "footer.tab" => " Switch ".to_string(),
        "footer.assign" => " Assign ".to_string(),
        "footer.refresh" => " Refresh ".to_string(),
        "footer.lang" => " Lang ".to_string(),
        "footer.quit" => " Quit ".to_string(),

        // VM auto-register
        "vm.auto_registered" => "Auto-registered {} VM(s) to libvirt".to_string(),
        "vm.auto_register_failed" => "Failed to register VM '{}': {}".to_string(),

        // Language select
        "lang.title" => " Select Language / 选择语言 ".to_string(),
        "lang.prompt" => "Press Enter to confirm".to_string(),

        // Status labels
        "status.driver" => "Driver".to_string(),
        "status.iommu_group" => "IOMMU Group".to_string(),

        // Warnings (GPU assignment)
        "warn.host_display_gpu" => "GPU {} is the host's primary display GPU - passing through may lose host display".to_string(),
        "warn.vm_conflict" => "GPU {} is already assigned to VM(s): {}".to_string(),
        "warn.iommu_host_display" => "Device {} (same IOMMU group) is the host's primary display GPU".to_string(),
        "warn.iommu_vm_conflict" => "Device {} (same IOMMU group) is already assigned to VM(s): {}".to_string(),

        _ => key.to_string(),
    }
}

fn t_zh(key: &str) -> String {
    match key {
        // Header
        "app.title" => " gpupass - 显卡直通管理器 ".to_string(),
        "iommu.enabled" => " IOMMU: 已启用 ".to_string(),
        "iommu.disabled" => " IOMMU: 未启用 ".to_string(),

        // GPU panel
        "gpu.panel_title" => " 显卡列表 ".to_string(),
        "gpu.boot" => " [主显]".to_string(),
        "gpu.no_driver" => "无驱动".to_string(),

        // VM panel
        "vm.panel_title" => " 虚拟机 ".to_string(),
        "vm.no_gpu" => "无显卡".to_string(),
        "vm.gpu_label" => "显卡: ".to_string(),

        // Actions
        "action.bind_vfio" => "绑定显卡到 vfio-pci".to_string(),
        "action.unbind_vfio" => "从 vfio-pci 解绑显卡".to_string(),
        "action.attach_vm" => "将显卡挂载到虚拟机".to_string(),
        "action.detach_vm" => "从虚拟机卸载显卡".to_string(),
        "action.auto_passthrough" => "自动直通（完整流程）".to_string(),
        "action.apply_config" => "应用VFIO配置到系统".to_string(),
        "action.back" => "返回".to_string(),
        "action.title" => " 操作 ".to_string(),

        // Assign GPU
        "assign.title" => " 分配显卡给虚拟机: ".to_string(),
        "assign.none" => "无（移除显卡分配）".to_string(),
        "assign.select_title" => " 选择显卡 ".to_string(),

        // Confirm
        "confirm.title" => " 确认 ".to_string(),
        "confirm.yes" => " [y] 是 ".to_string(),
        "confirm.no" => " [n] 否 ".to_string(),
        "confirm.bind_vfio" => "将 {} ({}) 绑定到 vfio-pci？\n这会将其从 {} 驱动上解绑。".to_string(),
        "confirm.unbind_vfio" => "将 {} ({}) 从 vfio-pci 解绑？\n它会恢复到 {} 驱动。".to_string(),
        "confirm.attach_gpu" => "将 {} ({}) 挂载到虚拟机 \"{}\"？".to_string(),
        "confirm.detach_gpu" => "将 {} ({}) 从虚拟机 \"{}\" 卸载？".to_string(),
        "confirm.apply_config" => "将VFIO配置写入 /etc/modprobe.d/vfio.conf？\n这会更新initramfs并需要重启。".to_string(),

        // Messages
        "msg.title_ok" => " 结果 ".to_string(),
        "msg.title_err" => " 错误 ".to_string(),
        "msg.close" => " [Enter/Esc] 关闭 ".to_string(),
        "msg.no_gpu_vm" => "没有可用的显卡或虚拟机".to_string(),
        "msg.no_vms" => "未找到虚拟机，请先创建虚拟机".to_string(),
        "msg.removed_gpu" => "已移除 {} 的显卡分配".to_string(),
        "msg.success" => "成功: {}".to_string(),
        "msg.error" => "错误: {}".to_string(),
        "msg.ok" => "[成功] {}".to_string(),
        "msg.warn" => "[警告] {}".to_string(),
        "msg.err" => "[错误] {}".to_string(),
        "msg.reboot" => "[需重启] {}".to_string(),
        "msg.initramfs_warn" => "{}\n警告: initramfs更新失败: {}".to_string(),
        "msg.initramfs_ok" => "{}\n{}".to_string(),

        // Footer
        "footer.navigate" => " 导航 ".to_string(),
        "footer.select" => " 选择 ".to_string(),
        "footer.tab" => " 切换 ".to_string(),
        "footer.assign" => " 分配 ".to_string(),
        "footer.refresh" => " 刷新 ".to_string(),
        "footer.lang" => " 语言 ".to_string(),
        "footer.quit" => " 退出 ".to_string(),

        // VM auto-register
        "vm.auto_registered" => "已自动注册 {} 个虚拟机到libvirt".to_string(),
        "vm.auto_register_failed" => "注册虚拟机 '{}' 失败: {}".to_string(),

        // Language select
        "lang.title" => " 选择语言 / Select Language ".to_string(),
        "lang.prompt" => "按回车确认 / Press Enter to confirm".to_string(),

        // Status labels
        "status.driver" => "驱动".to_string(),
        "status.iommu_group" => "IOMMU组".to_string(),

        // Warnings (GPU assignment)
        "warn.host_display_gpu" => "GPU {} 是宿主主显示显卡 - 直通可能导致宿主失去显示输出".to_string(),
        "warn.vm_conflict" => "GPU {} 已分配给虚拟机: {}".to_string(),
        "warn.iommu_host_display" => "设备 {}（同一IOMMU组）是宿主主显示显卡".to_string(),
        "warn.iommu_vm_conflict" => "设备 {}（同一IOMMU组）已分配给虚拟机: {}".to_string(),

        _ => key.to_string(),
    }
}
