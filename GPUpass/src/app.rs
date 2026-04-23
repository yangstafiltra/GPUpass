use crate::config::AppConfig;
use crate::event::{Action, AppMode, KeyEvent_, PanelFocus};
use crate::gpu::GpuDevice;
use crate::lang::{self, Lang};
use crate::passthrough::PassthroughResult;
use crate::vm::VmInfo;
use crossterm::event::KeyCode;

pub struct App {
    pub mode: AppMode,
    pub panel_focus: PanelFocus,
    pub gpus: Vec<GpuDevice>,
    pub vms: Vec<VmInfo>,
    pub config: AppConfig,
    pub gpu_list_state: ListState,
    pub vm_list_state: ListState,
    pub action_list_state: ListState,
    pub assign_gpu_state: ListState,
    pub actions: Vec<Action>,
    pub message: String,
    pub message_is_error: bool,
    pub confirm_message: String,
    pub confirm_action: Option<ConfirmAction>,
    pub selected_gpu_index: Option<usize>,
    pub selected_vm_for_assign: Option<usize>,
    pub iommu_enabled: bool,
    pub should_quit: bool,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    BindVfio(usize),
    UnbindVfio(usize),
    #[allow(dead_code)]
    AttachGpuToVm { gpu_idx: usize, vm_idx: usize, warnings: Vec<String> },
    DetachGpuFromVm { gpu_idx: usize, vm_idx: usize },
    ApplyConfig,
}

pub struct ListState {
    pub selected: usize,
    pub offset: usize,
}

impl ListState {
    pub fn new() -> Self {
        ListState {
            selected: 0,
            offset: 0,
        }
    }

    pub fn select(&mut self, index: usize, total: usize) {
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(total - 1);
        if self.selected < self.offset {
            self.offset = self.selected;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.offset {
                self.offset = self.selected;
            }
        }
    }

    pub fn move_down(&mut self, total: usize, visible_count: usize) {
        if total > 0 && self.selected < total - 1 {
            self.selected += 1;
            if self.selected >= self.offset + visible_count {
                self.offset = self.selected - visible_count + 1;
            }
        }
    }
}

impl App {
    pub fn new() -> Self {
        let config = AppConfig::load();

        // Set the language globally
        lang::set_lang(config.get_lang());

        let gpus = crate::gpu::detect_gpus();
        let vms = crate::vm::detect_vms();
        let iommu_enabled = crate::gpu::check_iommu_enabled();

        App {
            mode: AppMode::Main,
            panel_focus: PanelFocus::VmList,
            gpus,
            vms,
            config,
            gpu_list_state: ListState::new(),
            vm_list_state: ListState::new(),
            action_list_state: ListState::new(),
            assign_gpu_state: ListState::new(),
            actions: Vec::new(),
            message: String::new(),
            message_is_error: false,
            confirm_message: String::new(),
            confirm_action: None,
            selected_gpu_index: None,
            selected_vm_for_assign: None,
            iommu_enabled,
            should_quit: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent_) {
        match self.mode {
            AppMode::Main => self.handle_main_key(key),
            AppMode::ActionMenu => self.handle_action_key(key),
            AppMode::GpuAssign => self.handle_assign_key(key),
            AppMode::ConfirmDialog => self.handle_confirm_key(key),
            AppMode::MessageDialog => self.handle_message_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent_) {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.panel_focus {
                    PanelFocus::GpuList => self.gpu_list_state.move_up(),
                    PanelFocus::VmList => self.vm_list_state.move_up(),
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.panel_focus {
                    PanelFocus::GpuList => self.gpu_list_state.move_down(self.gpus.len(), 10),
                    PanelFocus::VmList => self.vm_list_state.move_down(self.vms.len(), 10),
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.panel_focus = PanelFocus::GpuList;
            }
            KeyCode::Right | KeyCode::Tab => {
                self.panel_focus = PanelFocus::VmList;
            }
            KeyCode::Enter => {
                match self.panel_focus {
                    PanelFocus::GpuList => {
                        if !self.gpus.is_empty() {
                            self.selected_gpu_index = Some(self.gpu_list_state.selected);
                            self.build_action_menu();
                            self.mode = AppMode::ActionMenu;
                        }
                    }
                    PanelFocus::VmList => {
                        if !self.vms.is_empty() && !self.gpus.is_empty() {
                            self.selected_vm_for_assign = Some(self.vm_list_state.selected);
                            // Use currently selected GPU (or first)
                            if self.selected_gpu_index.is_none() && !self.gpus.is_empty() {
                                self.selected_gpu_index = Some(0);
                            }
                            self.mode = AppMode::GpuAssign;
                            self.assign_gpu_state = ListState::new();
                        } else {
                            self.show_message(&lang::t("msg.no_gpu_vm"), true);
                        }
                    }
                }
            }
            KeyCode::Char('a') => {
                if !self.gpus.is_empty() && !self.vms.is_empty() {
                    self.selected_gpu_index = Some(self.gpu_list_state.selected);
                    self.mode = AppMode::GpuAssign;
                    self.assign_gpu_state = ListState::new();
                } else {
                    self.show_message(&lang::t("msg.no_gpu_vm"), true);
                }
            }
            KeyCode::Char('r') => {
                self.refresh();
            }
            KeyCode::Char('L') => {
                self.cycle_language();
            }
            _ => {}
        }
    }

    fn handle_assign_key(&mut self, key: KeyEvent_) {
        match key.code {
            KeyCode::Esc => {
                self.mode = AppMode::Main;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.assign_gpu_state.move_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let total = self.gpus.len() + 1;
                self.assign_gpu_state.move_down(total, 10);
            }
            KeyCode::Enter => {
                let total = self.gpus.len();
                // Resolve the target VM: prefer explicitly selected, fallback to highlighted VM
                let vm_idx = self
                    .selected_vm_for_assign
                    .or(if !self.vms.is_empty() { Some(self.vm_list_state.selected) } else { None });

                if self.assign_gpu_state.selected == total {
                    // ── "None" selected: detach all GPUs from this VM ──
                    if let Some(vm_idx) = vm_idx {
                        let vm_name = match self.vms.get(vm_idx) {
                            Some(v) => v.name.clone(),
                            None => {
                                self.show_message("Error: VM index out of bounds", true);
                                self.mode = AppMode::Main;
                                return;
                            }
                        };
                        let mut assigned: Vec<String> = self
                            .config
                            .vm_gpu_assignments
                            .get(&vm_name)
                            .cloned()
                            .unwrap_or_default();
                        for addr in &self.vms[vm_idx].assigned_gpus {
                            if !assigned.contains(addr) {
                                assigned.push(addr.clone());
                            }
                        }
                        if assigned.is_empty() {
                            self.show_message(
                                &lang::t("msg.removed_gpu").replace("{}", &vm_name),
                                false,
                            );
                            self.mode = AppMode::Main;
                            return;
                        }
                        let mut errors = Vec::new();
                        for gpu_addr in &assigned {
                            if let Some(gpu) = self.gpus.iter().find(|g| g.pci_address == *gpu_addr) {
                                if let Err(e) = crate::vm::detach_gpu_from_vm(&vm_name, gpu, &self.vms) {
                                    errors.push(format!("{}: {}", gpu_addr, e));
                                }
                            }
                            self.config.remove_gpu_from_vm(&vm_name, gpu_addr);
                        }
                        if let Err(e) = self.config.save() {
                            self.show_message(&format!("Config save failed: {}", e), true);
                        }
                        self.refresh();
                        if errors.is_empty() {
                            self.show_message(
                                &lang::t("msg.removed_gpu").replace("{}", &vm_name),
                                false,
                            );
                        } else {
                            self.show_message(
                                &format!("Removed from config. libvirt errors: {}", errors.join("; ")),
                                true,
                            );
                        }
                    } else {
                        self.show_message(&lang::t("msg.no_vms"), true);
                    }
                } else {
                    // ── A GPU was selected: attach it to the target VM ──
                    if let Some(vm_idx) = vm_idx {
                        let gpu = match self.gpus.get(self.assign_gpu_state.selected) {
                            Some(g) => g,
                            None => {
                                self.show_message("Error: GPU index out of bounds", true);
                                self.mode = AppMode::Main;
                                return;
                            }
                        };
                        let vm_name = match self.vms.get(vm_idx) {
                            Some(v) => v.name.clone(),
                            None => {
                                self.show_message("Error: VM index out of bounds", true);
                                self.mode = AppMode::Main;
                                return;
                            }
                        };

                        // Collect warnings for the confirm dialog
                        let warnings = self.collect_attach_warnings(gpu);

                        self.confirm_action = Some(ConfirmAction::AttachGpuToVm {
                            gpu_idx: self.assign_gpu_state.selected,
                            vm_idx,
                            warnings: warnings.clone(),
                        });

                        let mut confirm_msg = lang::t("confirm.attach_gpu")
                            .replacen("{}", &gpu.device_name, 1)
                            .replacen("{}", &gpu.pci_address, 1)
                            .replacen("{}", &vm_name, 1);

                        if !warnings.is_empty() {
                            confirm_msg.push_str("\n\n--- Warnings ---");
                            for w in &warnings {
                                confirm_msg.push_str("\n");
                                confirm_msg.push_str(w);
                            }
                        }

                        self.confirm_message = confirm_msg;
                        self.mode = AppMode::ConfirmDialog;
                    } else {
                        self.show_message(&lang::t("msg.no_vms"), true);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_action_key(&mut self, key: KeyEvent_) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') => {
                self.mode = AppMode::Main;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.action_list_state.move_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.action_list_state.move_down(self.actions.len(), 10);
            }
            KeyCode::Enter => {
                if let Some(gpu_idx) = self.selected_gpu_index {
                    if self.action_list_state.selected < self.actions.len() {
                        let action = self.actions[self.action_list_state.selected];
                        self.execute_action(action, gpu_idx);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_confirm_key(&mut self, key: KeyEvent_) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(action) = self.confirm_action.take() {
                    self.execute_confirm(action);
                }
                self.mode = AppMode::Main;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.confirm_action = None;
                self.mode = AppMode::Main;
            }
            _ => {}
        }
    }

    fn handle_message_key(&mut self, key: KeyEvent_) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                self.message.clear();
                self.mode = AppMode::Main;
            }
            _ => {}
        }
    }

    fn build_action_menu(&mut self) {
        self.actions.clear();

        if let Some(idx) = self.selected_gpu_index {
            let gpu = &self.gpus[idx];

            if gpu.driver == "vfio-pci" {
                self.actions.push(Action::UnbindVfio);
            } else if gpu.can_passthrough {
                self.actions.push(Action::BindVfio);
            }

            if gpu.can_passthrough || gpu.driver == "vfio-pci" {
                self.actions.push(Action::AttachToVm);
                self.actions.push(Action::DetachFromVm);
            }

            self.actions.push(Action::AutoPassthrough);
            self.actions.push(Action::ApplyConfig);
        }

        self.actions.push(Action::Back);
        self.action_list_state = ListState::new();
    }

    fn execute_action(&mut self, action: Action, gpu_idx: usize) {
        match action {
            Action::BindVfio => {
                let gpu = &self.gpus[gpu_idx];
                self.confirm_action = Some(ConfirmAction::BindVfio(gpu_idx));
                self.confirm_message = lang::t("confirm.bind_vfio")
                    .replacen("{}", &gpu.device_name, 1)
                    .replacen("{}", &gpu.pci_address, 1)
                    .replacen("{}", &gpu.driver, 1);
                self.mode = AppMode::ConfirmDialog;
            }
            Action::UnbindVfio => {
                let gpu = &self.gpus[gpu_idx];
                let original = self
                    .config
                    .get_original_driver(&gpu.pci_address)
                    .unwrap_or("nouveau")
                    .to_string();
                self.confirm_action = Some(ConfirmAction::UnbindVfio(gpu_idx));
                self.confirm_message = lang::t("confirm.unbind_vfio")
                    .replacen("{}", &gpu.device_name, 1)
                    .replacen("{}", &gpu.pci_address, 1)
                    .replacen("{}", &original, 1);
                self.mode = AppMode::ConfirmDialog;
            }
            Action::AttachToVm => {
                if self.vms.is_empty() {
                    self.show_message(&lang::t("msg.no_vms"), true);
                    return;
                }
                self.selected_vm_for_assign = None;
                self.mode = AppMode::GpuAssign;
                self.assign_gpu_state = ListState::new();
            }
            Action::DetachFromVm => {
                if let Some(gpu_idx) = self.selected_gpu_index {
                    let gpu = &self.gpus[gpu_idx];
                    // Find which VM this GPU is assigned to in our config
                    let mut assigned_vm = None;
                    for (vm_name, gpus) in &self.config.vm_gpu_assignments {
                        if gpus.contains(&gpu.pci_address) {
                            assigned_vm = Some(vm_name.clone());
                            break;
                        }
                    }
                    if let Some(vm_name) = assigned_vm {
                        if let Some(vm_idx) = self.vms.iter().position(|v| v.name == vm_name) {
                            self.confirm_action = Some(ConfirmAction::DetachGpuFromVm {
                                gpu_idx,
                                vm_idx,
                            });
                            self.confirm_message = lang::t("confirm.detach_gpu")
                                .replace("{}", &gpu.device_name)
                                .replacen("{}", &gpu.pci_address, 1)
                                .replacen("{}", &vm_name, 1);
                            self.mode = AppMode::ConfirmDialog;
                        } else {
                            self.show_message(
                                &lang::t("msg.error").replace("{}", "Assigned VM not found in list"),
                                true,
                            );
                        }
                    } else {
                        self.show_message(
                            &lang::t("msg.error").replace("{}", "This GPU is not assigned to any VM"),
                            true,
                        );
                    }
                }
            }
            Action::AutoPassthrough => {
                if self.vms.is_empty() {
                    self.show_message(&lang::t("msg.no_vms"), true);
                    return;
                }
                self.selected_gpu_index = Some(gpu_idx);
                self.mode = AppMode::GpuAssign;
                self.assign_gpu_state = ListState::new();
            }
            Action::ApplyConfig => {
                self.confirm_action = Some(ConfirmAction::ApplyConfig);
                self.confirm_message = lang::t("confirm.apply_config");
                self.mode = AppMode::ConfirmDialog;
            }
            Action::Back => {
                self.mode = AppMode::Main;
            }
        }
    }

    fn execute_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::BindVfio(gpu_idx) => {
                let gpu = match self.gpus.get(gpu_idx) {
                    Some(g) => g,
                    None => {
                        self.show_message("Error: GPU index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                self.config
                    .save_original_driver(&gpu.pci_address, &gpu.driver);
                match crate::passthrough::bind_to_vfio(gpu) {
                    Ok(result) => {
                        let msg = format_passthrough_result(result);
                        self.show_message(&msg, false);
                        if let Err(e) = self.config.save() {
                            self.show_message(&format!("Config save failed: {}", e), true);
                        }
                    }
                    Err(e) => self.show_message(&lang::t("msg.error").replace("{}", &e), true),
                }
                self.refresh();
            }
            ConfirmAction::UnbindVfio(gpu_idx) => {
                let gpu = match self.gpus.get(gpu_idx) {
                    Some(g) => g,
                    None => {
                        self.show_message("Error: GPU index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                let original = self
                    .config
                    .get_original_driver(&gpu.pci_address)
                    .unwrap_or("nouveau")
                    .to_string();
                match crate::passthrough::unbind_from_vfio(gpu, &original) {
                    Ok(result) => {
                        let msg = format_passthrough_result(result);
                        self.show_message(&msg, false);
                    }
                    Err(e) => self.show_message(&lang::t("msg.error").replace("{}", &e), true),
                }
                self.refresh();
            }
            ConfirmAction::AttachGpuToVm { gpu_idx, vm_idx, warnings: _ } => {
                let gpu = match self.gpus.get(gpu_idx) {
                    Some(g) => g,
                    None => {
                        self.show_message("Error: GPU index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                let vm_name = match self.vms.get(vm_idx) {
                    Some(v) => v.name.clone(),
                    None => {
                        self.show_message("Error: VM index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                match crate::vm::attach_gpu_to_vm(&vm_name, gpu, &self.vms) {
                    Ok(msg) => {
                        self.config
                            .assign_gpu_to_vm(&vm_name, &gpu.pci_address);
                        if let Err(e) = self.config.save() {
                            self.show_message(&format!("Config save failed: {}", e), true);
                        }
                        self.show_message(&lang::t("msg.success").replace("{}", &msg), false);
                    }
                    Err(e) => self.show_message(&lang::t("msg.error").replace("{}", &e), true),
                }
                self.refresh();
            }
            ConfirmAction::DetachGpuFromVm { gpu_idx, vm_idx } => {
                let gpu = match self.gpus.get(gpu_idx) {
                    Some(g) => g,
                    None => {
                        self.show_message("Error: GPU index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                let vm_name = match self.vms.get(vm_idx) {
                    Some(v) => v.name.clone(),
                    None => {
                        self.show_message("Error: VM index out of bounds", true);
                        self.refresh();
                        return;
                    }
                };
                match crate::vm::detach_gpu_from_vm(&vm_name, gpu, &self.vms) {
                    Ok(msg) => {
                        self.config
                            .remove_gpu_from_vm(&vm_name, &gpu.pci_address);
                        if let Err(e) = self.config.save() {
                            self.show_message(&format!("Config save failed: {}", e), true);
                        }
                        self.show_message(&lang::t("msg.success").replace("{}", &msg), false);
                    }
                    Err(e) => self.show_message(&lang::t("msg.error").replace("{}", &e), true),
                }
                self.refresh();
            }
            ConfirmAction::ApplyConfig => {
                let passable_gpus: Vec<&GpuDevice> = self
                    .gpus
                    .iter()
                    .filter(|g| g.can_passthrough || g.driver == "vfio-pci")
                    .collect();
                match crate::passthrough::apply_vfio_conf(&passable_gpus) {
                    Ok(result) => {
                        let msg = format_passthrough_result(result);
                        match crate::passthrough::update_initramfs() {
                            Ok(init_msg) => {
                                self.show_message(
                                    &lang::t("msg.initramfs_ok").replacen("{}", &msg, 1).replacen("{}", &init_msg, 1),
                                    false,
                                );
                            }
                            Err(e) => {
                                self.show_message(
                                    &lang::t("msg.initramfs_warn").replacen("{}", &msg, 1).replacen("{}", &e, 1),
                                    true,
                                );
                            }
                        }
                    }
                    Err(e) => self.show_message(&lang::t("msg.error").replace("{}", &e), true),
                }
            }
        }
    }

    fn show_message(&mut self, msg: &str, is_error: bool) {
        self.message = msg.to_string();
        self.message_is_error = is_error;
        self.mode = AppMode::MessageDialog;
    }

    fn refresh(&mut self) {
        self.gpus = crate::gpu::detect_gpus();
        self.vms = crate::vm::detect_vms();
        self.iommu_enabled = crate::gpu::check_iommu_enabled();
        if !self.gpus.is_empty() {
            self.gpu_list_state
                .select(self.gpu_list_state.selected, self.gpus.len());
        }
        if !self.vms.is_empty() {
            self.vm_list_state
                .select(self.vm_list_state.selected, self.vms.len());
        }
    }

    fn cycle_language(&mut self) {
        let new_lang = match lang::lang() {
            Lang::En => Lang::Zh,
            Lang::Zh => Lang::En,
        };
        lang::set_lang(new_lang);
        self.config.set_lang(new_lang);
        if let Err(e) = self.config.save() {
            self.show_message(&format!("Config save failed: {}", e), true);
        }
    }

    /// Collect warnings for attaching a GPU to a VM (for display in confirm dialog)
    fn collect_attach_warnings(&self, gpu: &crate::gpu::GpuDevice) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check GPU conflicts with other VMs
        let conflict_info = crate::vm::check_gpu_conflicts(&gpu.pci_address, &self.vms);
        if !conflict_info.conflicting_vms.is_empty() {
            warnings.push(lang::t("warn.vm_conflict")
                .replacen("{}", &gpu.pci_address, 1)
                .replacen("{}", &conflict_info.conflicting_vms.join(", "), 1));
        }
        if conflict_info.is_host_display_gpu {
            warnings.push(lang::t("warn.host_display_gpu").replace("{}", &gpu.pci_address));
        }

        // Check IOMMU group devices
        let group_devices = crate::gpu::get_iommu_group_devices(&gpu.pci_address);
        for pci_addr in &group_devices {
            let device_conflict = crate::vm::check_gpu_conflicts(pci_addr, &self.vms);
            if !device_conflict.conflicting_vms.is_empty() {
                warnings.push(lang::t("warn.iommu_vm_conflict")
                    .replacen("{}", pci_addr, 1)
                    .replacen("{}", &device_conflict.conflicting_vms.join(", "), 1));
            }
            if device_conflict.is_host_display_gpu {
                warnings.push(lang::t("warn.iommu_host_display").replace("{}", pci_addr));
            }
        }

        warnings
    }
}

fn format_passthrough_result(result: PassthroughResult) -> String {
    match result {
        PassthroughResult::Success(msg) => lang::t("msg.ok").replace("{}", &msg),
        PassthroughResult::Warning(msg) => lang::t("msg.warn").replace("{}", &msg),
        PassthroughResult::Error(msg) => lang::t("msg.err").replace("{}", &msg),
        PassthroughResult::NeedsReboot(msg) => lang::t("msg.reboot").replace("{}", &msg),
    }
}
