use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppMode {
    Main,              // Two-panel: GPU left + VM right
    ActionMenu,        // Action menu overlay
    GpuAssign,         // Assign a GPU to a VM
    ConfirmDialog,
    MessageDialog,
}

/// Which panel is focused in Main mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelFocus {
    GpuList,
    VmList,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    BindVfio,
    UnbindVfio,
    AttachToVm,
    DetachFromVm,
    AutoPassthrough,
    ApplyConfig,
    Back,
}

pub fn poll_event(timeout: Duration) -> Option<KeyEvent_> {
    if event::poll(timeout).ok()? {
        if let Event::Key(key) = event::read().ok()? {
            if key.kind == KeyEventKind::Press {
                return Some(KeyEvent_::from(key));
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct KeyEvent_ {
    pub code: KeyCode,
    pub modifiers: event::KeyModifiers,
}

impl From<crossterm::event::KeyEvent> for KeyEvent_ {
    fn from(key: crossterm::event::KeyEvent) -> Self {
        KeyEvent_ {
            code: key.code,
            modifiers: key.modifiers,
        }
    }
}
