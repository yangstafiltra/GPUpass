mod app;
mod config;
mod event;
mod gpu;
mod lang;
mod passthrough;
mod ui;
mod vm;

use app::App;
use crossterm::{
    event::{KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, time::Duration};

fn main() -> Result<(), io::Error> {
    // Check if running as root (needed for VFIO operations)
    let euid = unsafe { libc::geteuid() };
    if euid != 0 {
        eprintln!("gpupass: GPU passthrough management requires root privileges.");
        eprintln!("Please run with: sudo gpupass");
        std::process::exit(1);
    }

    // Check dependencies
    let missing = gpu::check_dependencies();
    if !missing.is_empty() {
        eprintln!("gpupass: Missing required tools: {}", missing.join(", "));
        eprintln!("Please install them before running gpupass.");
        std::process::exit(1);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let result = run_app(&mut terminal);

    // Always restore terminal even if there was an error
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), io::Error> {
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if let Some(key) = event::poll_event(Duration::from_millis(100)) {
            let key_event = event::KeyEvent_::from(key);
            if key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
            app.handle_key(key_event);
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
