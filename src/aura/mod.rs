//! Aura Launcher — a terminal Steam client.
//!
//! The TUI is intentionally a thin frontend: every Steam operation is delegated
//! back to this same executable through its documented `--json` interface. The
//! on-disk library is Steam's own, so the TUI can run next to the official Steam
//! client without reinstalling games.

mod app;
mod backend;
mod markdown;
mod model;
mod ui;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::cursor::Show;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui_image::picker::{Picker, ProtocolType};

use self::app::App;
use self::backend::Backend;

/// Run the TUI. `backend_path` defaults to this executable; commands are issued
/// as `<backend> --json <cmd>`, so a separately installed Aurelia also works.
pub fn run(
    backend_path: Option<PathBuf>,
    country: Option<String>,
    lang: Option<String>,
    installed_only: bool,
) -> Result<()> {
    let path = match backend_path {
        Some(path) => {
            if !path.is_file() {
                bail!("Aurelia backend not found at {}", path.display());
            }
            path
        }
        None => std::env::current_exe()?,
    };
    let backend = Backend { path };

    install_panic_hook();
    let mut terminal = init_terminal()?;

    // Detect graphics support from the environment instead of querying stdio:
    // the query can leave a partial escape sequence in the input buffer and
    // swallow the first key press. Capability hints are conservative; when in
    // doubt we fall back to half-blocks and only draw them in a colour terminal.
    let protocol = detected_protocol();
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(protocol);
    let images_enabled = match protocol {
        ProtocolType::Halfblocks => supports_color(),
        _ => true,
    };

    let result = event_loop(
        &mut terminal,
        backend,
        country,
        lang,
        installed_only,
        picker,
        images_enabled,
    );
    restore_terminal(&mut terminal)?;
    result
}

fn detected_protocol() -> ProtocolType {
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let lc_terminal = std::env::var("LC_TERMINAL")
        .unwrap_or_default()
        .to_ascii_lowercase();
    // tmux may not have graphics passthrough enabled, and a Kitty/Sixel
    // protocol selected inside it can leak placeholder text into the pane.
    // Half-blocks are safe and look the same everywhere.
    if term.starts_with("tmux") || program == "tmux" {
        return ProtocolType::Halfblocks;
    }
    if std::env::var_os("KITTY_WINDOW_ID").is_some()
        || term.contains("kitty")
        || program.contains("kitty")
        || program.contains("ghostty")
    {
        ProtocolType::Kitty
    } else if program.contains("iterm") || lc_terminal.contains("iterm") {
        ProtocolType::Iterm2
    } else if term.contains("sixel")
        || program.contains("wezterm")
        || program.contains("foot")
        || program.contains("mlterm")
    {
        ProtocolType::Sixel
    } else {
        ProtocolType::Halfblocks
    }
}

fn supports_color() -> bool {
    let color_term = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if color_term.contains("truecolor") || color_term.contains("24bit") {
        return true;
    }
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["256color", "truecolor", "kitty", "sixel", "iterm"]
        .iter()
        .any(|hint| term.contains(hint))
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    backend: Backend,
    country: Option<String>,
    lang: Option<String>,
    installed_only: bool,
    picker: Picker,
    images_enabled: bool,
) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(
        backend,
        tx,
        rx,
        app::ViewOptions {
            country,
            lang,
            only_installed: installed_only,
            picker,
            images_enabled,
        },
    );

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            app.handle_key(key);
        }
        app.poll();
        app.tick();

        if app.login_requested {
            app.login_requested = false;
            suspend_terminal(terminal)?;
            eprintln!("Running `aurelia login` — complete the Steam sign-in, then return here.");
            let status = run_login(&app.backend);
            resume_terminal(terminal)?;
            app.login_finished(status);
        }

        if app.quit {
            app.close_chat_now();
            break;
        }
    }
    Ok(())
}

fn run_login(backend: &Backend) -> bool {
    #[cfg(unix)]
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(&backend.path);
    command.arg("login");
    // Ctrl-C during the interactive login must stop the child, not this TUI.
    // Ignore it here, then give the child the default disposition again.
    #[cfg(unix)]
    {
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
        }
        unsafe {
            command.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                Ok(())
            });
        }
    }
    let success = command
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }
    success
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn suspend_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn resume_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        original(info);
    }));
}
