//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

mod app_init;
mod app_input;
mod app_shell;
mod pty_encoding;

use std::io::Write;
use std::sync::Arc;

use iocraft::prelude::*;
use tracing::error;

use jefe::layout::{compute_pty_layout, is_fullscreen_enabled};
use jefe::runtime::TmuxRuntimeManager;
use jefe::theme::FileThemeManager;

/// Shared application context passed to the root component.
struct AppContext {
    persistence: jefe::persistence::FilePersistenceManager,
    theme_manager: FileThemeManager,
    runtime: TmuxRuntimeManager,
    /// @plan PLAN-20260329-ISSUES-MODE.P09
    gh_client: jefe::github::GhClient,
}

/// Parse CLI arguments, handling early-exit flags (`--version`, `--help`).
///
/// Returns the parsed [`CliArgs`] when execution should continue, or `None`
/// when the process has already handled an early-exit flag and `main` should
/// return.
fn parse_cli_or_exit() -> Option<jefe::cli::CliArgs> {
    match jefe::cli::parse_args(std::env::args().skip(1)) {
        Ok(args) => handle_parsed_cli_args(args),
        Err(e) => {
            write_cli_error(&e);
            std::process::exit(2);
        }
    }
}

fn handle_parsed_cli_args(args: jefe::cli::CliArgs) -> Option<jefe::cli::CliArgs> {
    if args.help {
        write_stdout_line(jefe::cli::USAGE);
        return None;
    }
    if args.version {
        let version = jefe::VERSION;
        write_stdout_line(&format!("jefe {version}"));
        return None;
    }
    Some(args)
}

fn write_stdout_line(message: &str) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{message}");
}

fn write_cli_error(error: &jefe::cli::CliError) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "error: {error}");
    let _ = writeln!(handle);
    let _ = writeln!(handle, "{}", jefe::cli::USAGE);
}

fn main() {
    let Some(cli_args) = parse_cli_or_exit() else {
        return;
    };

    // Initialize structured logging (no-op if JEFE_LOG_FILE is unset).
    jefe::logging::init();
    tracing::info!(version = jefe::VERSION, "jefe starting");
    tracing::debug!(
        log_file = ?jefe::logging::log_file_path(),
        config_dir = ?cli_args.config_dir,
        "logging initialized"
    );

    // Get terminal size and derive PTY viewport size from dashboard geometry.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols, _, _) = compute_pty_layout(cols, rows);

    // Initialize managers. An explicit `--config <dir>` isolates settings,
    // state, and themes under that directory; otherwise fall back to the
    // default platform paths and environment variable overrides.
    let mut theme_manager = FileThemeManager::new();
    let persistence = if let Some(dir) = cli_args.config_dir.as_deref() {
        let paths = jefe::persistence::resolve_paths_from_dir(dir);
        theme_manager.load_from_dir(&dir.join("themes"));
        jefe::persistence::FilePersistenceManager::with_paths(paths)
    } else {
        jefe::persistence::FilePersistenceManager::new()
    };
    let runtime = TmuxRuntimeManager::new(pty_rows, pty_cols);

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        persistence,
        theme_manager,
        runtime,
        gh_client: jefe::github::GhClient::new(),
    }));

    smol::block_on(async {
        let mut app = element!(app_shell::App(context: Some(context)));

        if is_fullscreen_enabled() {
            if let Err(e) = app.fullscreen().await {
                error!(error = %e, "fullscreen mode failed");
            }
        } else if let Err(e) = app.render_loop().await {
            error!(error = %e, "render loop failed");
        }
    });
}
