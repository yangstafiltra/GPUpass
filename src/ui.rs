use crate::app::App;
use crate::event::{Action, AppMode, PanelFocus};
use crate::lang;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.area();

    match app.mode {
        AppMode::Main => draw_main(f, app, size),
        AppMode::ActionMenu => draw_main_with_action(f, app, size),
        AppMode::GpuAssign => draw_assign(f, app, size),
        AppMode::ConfirmDialog => draw_main_with_confirm(f, app, size),
        AppMode::MessageDialog => draw_main_with_message(f, app, size),
    }
}

// ─── Language Selection ────────────────────────────────────────────

// ─── Main Layout ────────────────────────────────────────────────────

fn draw_main(f: &mut Frame, app: &App, size: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    draw_header(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);
}

fn draw_main_with_action(f: &mut Frame, app: &App, size: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    draw_header(f, app, chunks[0]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    draw_gpu_panel(f, app, content[0]);
    draw_action_panel(f, app, content[1]);

    draw_footer(f, app, chunks[2]);
}

fn draw_main_with_confirm(f: &mut Frame, app: &App, size: Rect) {
    draw_main(f, app, size);
    draw_confirm_dialog(f, app, size);
}

fn draw_main_with_message(f: &mut Frame, app: &App, size: Rect) {
    draw_main(f, app, size);
    draw_message_dialog(f, app, size);
}

// ─── Header ──────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let iommu_status = if app.iommu_enabled {
        Span::styled(
            lang::t("iommu.enabled"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            lang::t("iommu.disabled"),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    };

    let title = Span::styled(
        lang::t("app.title"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let gpu_count = Span::raw(format!(
        "  GPUs: {}  VMs: {} ",
        app.gpus.len(),
        app.vms.len()
    ));

    let lang_indicator = Span::styled(
        format!(" [{}]", lang::lang()),
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![title, iommu_status, gpu_count, lang_indicator]);
    let paragraph = Paragraph::new(line);
    f.render_widget(paragraph, area);
}

// ─── Content (two panels) ────────────────────────────────────────────

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    draw_gpu_panel(f, app, chunks[0]);
    draw_vm_panel(f, app, chunks[1]);
}

// ─── GPU Panel ────────────────────────────────────────────────────────

fn draw_gpu_panel(f: &mut Frame, app: &App, area: Rect) {
    let is_active = matches!(app.mode, AppMode::Main)
        && app.panel_focus == PanelFocus::GpuList;

    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .gpus
        .iter()
        .enumerate()
        .map(|(i, gpu)| {
            let status_icon = if gpu.driver == "vfio-pci" {
                "●"
            } else if gpu.can_passthrough {
                "○"
            } else {
                "✗"
            };

            let status_color = if gpu.driver == "vfio-pci" {
                Color::Green
            } else if gpu.can_passthrough {
                Color::Yellow
            } else {
                Color::Red
            };

            let is_selected = app.gpu_list_state.selected == i;

            let driver_info = if gpu.driver.is_empty() {
                lang::t("gpu.no_driver")
            } else {
                gpu.driver.clone()
            };

            let boot_marker = if gpu.is_boot_gpu {
                &lang::t("gpu.boot")
            } else {
                ""
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", status_icon),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    truncate(&format!("{}{}", gpu.device_name, boot_marker), 22),
                    if is_selected && is_active {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    format!(" {}", gpu.pci_address),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let detail_line = Line::from(vec![
                Span::raw("     "),
                Span::styled(
                    format!(
                        "{}: {} | {}: {}",
                        lang::t("status.driver"),
                        driver_info,
                        lang::t("status.iommu_group"),
                        gpu.iommu_group
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(vec![line, detail_line])
        })
        .collect();

    let panel_title = format!(
        " {} (Enter: {}) ",
        lang::t("gpu.panel_title"),
        lang::t("footer.select")
    );

    let list = List::new(items)
        .block(
            Block::default()
                .title(panel_title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.gpu_list_state.selected));
    f.render_stateful_widget(list, area, &mut state);
}

// ─── VM Panel ──────────────────────────────────────────────────────────

fn draw_vm_panel(f: &mut Frame, app: &App, area: Rect) {
    let is_active = matches!(app.mode, AppMode::Main)
        && app.panel_focus == PanelFocus::VmList;

    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = app
        .vms
        .iter()
        .enumerate()
        .map(|(_i, vm)| {
            let state_icon = if vm.is_running() { "▶" } else { "■" };
            let state_color = if vm.is_running() {
                Color::Green
            } else {
                Color::Red
            };

            let assigned_gpus: Vec<String> = if let Some(gpu_addrs) =
                app.config.vm_gpu_assignments.get(&vm.name)
            {
                gpu_addrs
                    .iter()
                    .filter_map(|addr| {
                        app.gpus
                            .iter()
                            .find(|g| g.pci_address == *addr)
                            .map(|g| g.device_name.clone())
                    })
                    .collect()
            } else {
                vm.assigned_gpus
                    .iter()
                    .filter_map(|addr| {
                        app.gpus
                            .iter()
                            .find(|g| g.pci_address == *addr)
                            .map(|g| g.device_name.clone())
                    })
                    .collect()
            };

            let gpu_info = if assigned_gpus.is_empty() {
                lang::t("vm.no_gpu")
            } else {
                format!("{}{}", lang::t("vm.gpu_label"), assigned_gpus.join(", "))
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", state_icon),
                    Style::default().fg(state_color),
                ),
                Span::styled(
                    truncate(&vm.name, 22),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" [{}]", vm.state),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let detail_line = Line::from(vec![
                Span::raw("     "),
                Span::styled(gpu_info, Style::default().fg(Color::Magenta)),
            ]);

            ListItem::new(vec![line, detail_line])
        })
        .collect();

    let panel_title = format!(
        " {} (Enter: {}) ",
        lang::t("vm.panel_title"),
        lang::t("footer.select")
    );

    let list = List::new(items)
        .block(
            Block::default()
                .title(panel_title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.vm_list_state.selected));
    f.render_stateful_widget(list, area, &mut state);
}

// ─── Action Panel ──────────────────────────────────────────────────────

fn draw_action_panel(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let icon = match action {
                Action::BindVfio => ">>",
                Action::UnbindVfio => "<<",
                Action::AttachToVm => "->",
                Action::DetachFromVm => "<-",
                Action::AutoPassthrough => "!!",
                Action::ApplyConfig => "##",
                Action::Back => "..",
            };

            let is_selected = app.action_list_state.selected == i;
            let label = match action {
                Action::BindVfio => lang::t("action.bind_vfio"),
                Action::UnbindVfio => lang::t("action.unbind_vfio"),
                Action::AttachToVm => lang::t("action.attach_vm"),
                Action::DetachFromVm => lang::t("action.detach_vm"),
                Action::AutoPassthrough => lang::t("action.auto_passthrough"),
                Action::ApplyConfig => lang::t("action.apply_config"),
                Action::Back => lang::t("action.back"),
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", icon),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    label,
                    if is_selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(
                    " {} (Enter: {} | Esc: {}) ",
                    lang::t("action.title"),
                    lang::t("footer.select"),
                    lang::t("action.back"),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.action_list_state.selected));
    f.render_stateful_widget(list, area, &mut state);
}

// ─── GPU Assign ──────────────────────────────────────────────────────

fn draw_assign(f: &mut Frame, app: &App, size: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    let vm_name = app
        .selected_vm_for_assign
        .or(if !app.vms.is_empty() { Some(app.vm_list_state.selected) } else { None })
        .map(|i| app.vms[i].name.clone())
        .unwrap_or_default();
    let header = Paragraph::new(format!("{}{}", lang::t("assign.title"), vm_name))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(header, chunks[0]);

    let mut items: Vec<ListItem> = app
        .gpus
        .iter()
        .enumerate()
        .map(|(_i, gpu)| {
            let status = if gpu.can_passthrough || gpu.driver == "vfio-pci" {
                "●"
            } else {
                "✗"
            };
            let color = if gpu.can_passthrough || gpu.driver == "vfio-pci" {
                Color::Green
            } else {
                Color::Red
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", status), Style::default().fg(color)),
                Span::styled(
                    format!(
                        "{} [{}] - {}",
                        gpu.device_name, gpu.pci_address, gpu.passthrough_reason
                    ),
                    Style::default(),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    items.push(ListItem::new(Line::from(vec![
        Span::styled(" ○ ", Style::default().fg(Color::Gray)),
        Span::styled(lang::t("assign.none"), Style::default().fg(Color::DarkGray)),
    ])));

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(
                    " {} (Enter: {} | Esc: {}) ",
                    lang::t("assign.select_title"),
                    lang::t("footer.select"),
                    lang::t("action.back"),
                ))
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.assign_gpu_state.selected));
    f.render_stateful_widget(list, chunks[1], &mut state);

    draw_footer(f, app, chunks[2]);
}

// ─── Confirm Dialog ──────────────────────────────────────────────────

fn draw_confirm_dialog(f: &mut Frame, _app: &App, size: Rect) {
    let dialog_width = 60.min(size.width - 4);
    let msg_lines: Vec<&str> = _app.confirm_message.lines().collect();
    let dialog_height = (msg_lines.len() as u16 + 5).min(size.height - 4);
    let x = (size.width - dialog_width) / 2;
    let y = (size.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);
    f.render_widget(Clear, dialog_area);

    let lines: Vec<Line> = msg_lines
        .iter()
        .map(|l| Line::from(Span::styled(l.to_string(), Style::default().fg(Color::Yellow))))
        .chain(std::iter::once(Line::raw("")))
        .chain(std::iter::once(Line::from(vec![
            Span::styled(
                lang::t("confirm.yes"),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                lang::t("confirm.no"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ])))
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" {} ", lang::t("confirm.title")))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, dialog_area);
}

// ─── Message Dialog ──────────────────────────────────────────────────

fn draw_message_dialog(f: &mut Frame, app: &App, size: Rect) {
    let dialog_width = 70.min(size.width - 4);
    let msg_lines: Vec<&str> = app.message.lines().collect();
    let dialog_height = (msg_lines.len() as u16 + 4).min(size.height - 4);
    let x = (size.width - dialog_width) / 2;
    let y = (size.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);
    f.render_widget(Clear, dialog_area);

    let border_color = if app.message_is_error {
        Color::Red
    } else {
        Color::Green
    };

    let lines: Vec<Line> = msg_lines
        .iter()
        .map(|l| {
            let style = if l.starts_with("[错误]") || l.starts_with("[ERR]") {
                Style::default().fg(Color::Red)
            } else if l.starts_with("[警告]") || l.starts_with("[WARN]") {
                Style::default().fg(Color::Yellow)
            } else if l.starts_with("[需重启]") || l.starts_with("[REBOOT]") {
                Style::default().fg(Color::Magenta)
            } else if l.starts_with("[成功]") || l.starts_with("[OK]") {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(l.to_string(), style))
        })
        .chain(std::iter::once(Line::raw("")))
        .chain(std::iter::once(Line::from(Span::styled(
            lang::t("msg.close"),
            Style::default().fg(Color::DarkGray),
        ))))
        .collect();

    let title = if app.message_is_error {
        lang::t("msg.title_err")
    } else {
        lang::t("msg.title_ok")
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, dialog_area);
}

// ─── Footer ──────────────────────────────────────────────────────────

fn draw_footer(f: &mut Frame, _app: &App, area: Rect) {
    let keys = Line::from(vec![
        Span::styled(" j/k ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.navigate"))),
        Span::styled(" Enter ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.select"))),
        Span::styled(" Tab/←→ ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.tab"))),
        Span::styled(" a ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.assign"))),
        Span::styled(" r ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.refresh"))),
        Span::styled(" L ", Style::default().fg(Color::White)),
        Span::raw(format!(" {}  ", lang::t("footer.lang"))),
        Span::styled(" q ", Style::default().fg(Color::White)),
        Span::raw(format!(" {} ", lang::t("footer.quit"))),
    ]);

    let paragraph = Paragraph::new(keys);
    f.render_widget(paragraph, area);
}

fn truncate(s: &str, max_len: usize) -> String {
    // For CJK characters, count them as 2 width
    if max_len <= 3 {
        return "...".to_string();
    }
    let mut width = 0;
    let mut fit_chars = 0;
    for c in s.chars() {
        let cw = if c.is_ascii() { 1 } else { 2 };
        if width + cw > max_len - 3 {
            break;
        }
        width += cw;
        fit_chars += 1;
    }
    if fit_chars < s.chars().count() {
        format!("{}...", s.chars().take(fit_chars).collect::<String>())
    } else {
        s.to_string()
    }
}
