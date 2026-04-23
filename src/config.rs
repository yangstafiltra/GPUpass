use crate::gpu::GpuDevice;
use crate::lang::Lang;
use crate::vm::VmInfo;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Language preference
    pub language: String,
    /// Mapping of VM name -> list of GPU PCI addresses
    pub vm_gpu_assignments: std::collections::HashMap<String, Vec<String>>,
    /// Known GPUs at last scan
    #[serde(default)]
    pub known_gpus: Vec<GpuDevice>,
    /// Known VMs at last scan
    #[serde(default)]
    pub known_vms: Vec<VmInfo>,
    /// Original drivers before VFIO binding
    #[serde(default)]
    pub original_drivers: std::collections::HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            language: "en".to_string(),
            vm_gpu_assignments: std::collections::HashMap::new(),
            known_gpus: Vec::new(),
            known_vms: Vec::new(),
            original_drivers: std::collections::HashMap::new(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str(&data) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&path, data)
            .map_err(|e| format!("Failed to write config: {}", e))
    }

    pub fn get_lang(&self) -> Lang {
        match self.language.as_str() {
            "zh" => Lang::Zh,
            _ => Lang::En,
        }
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.language = match lang {
            Lang::Zh => "zh".to_string(),
            Lang::En => "en".to_string(),
        };
    }

    pub fn assign_gpu_to_vm(&mut self, vm_name: &str, pci_address: &str) {
        let entry = self.vm_gpu_assignments.entry(vm_name.to_string()).or_default();
        if !entry.contains(&pci_address.to_string()) {
            entry.push(pci_address.to_string());
        }
    }

    pub fn remove_gpu_from_vm(&mut self, vm_name: &str, pci_address: &str) {
        if let Some(gpus) = self.vm_gpu_assignments.get_mut(vm_name) {
            gpus.retain(|g| g != pci_address);
            if gpus.is_empty() {
                self.vm_gpu_assignments.remove(vm_name);
            }
        }
    }

    pub fn save_original_driver(&mut self, pci_address: &str, driver: &str) {
        if !driver.is_empty() && driver != "vfio-pci" {
            self.original_drivers
                .insert(pci_address.to_string(), driver.to_string());
        }
    }

    pub fn get_original_driver(&self, pci_address: &str) -> Option<&str> {
        self.original_drivers.get(pci_address).map(|s| s.as_str())
    }

    #[allow(dead_code)]
    pub fn get_vm_assigned_gpus(&self, vm_name: &str) -> &[String] {
        self.vm_gpu_assignments
            .get(vm_name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

fn config_path() -> PathBuf {
    let xdg_config = std::env::var("XDG_CONFIG_HOME").ok().or_else(|| {
        std::env::var("HOME").ok().map(|home| format!("{}/.config", home))
    }).unwrap_or_else(|| "/root/.config".to_string());
    PathBuf::from(xdg_config).join("gpupass").join("config.json")
}
