//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

mod app_init;
mod app_input;
mod app_shell;
mod detail_wrap_map;
mod mouse_routing;
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

/// Print a startup persistence error (e.g. an unusable explicit `--config`
/// directory) to stderr with actionable guidance, then let the caller exit
/// nonzero.
fn write_startup_error(error: &jefe::persistence::PersistenceError) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "error: {error}");
    let _ = writeln!(
        handle,
        "hint: check that the --config directory exists, is a directory, and is writable"
    );
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
    let layout = compute_pty_layout(cols, rows);
    let pty_rows = layout.pty_rows;
    let pty_cols = layout.pty_cols;

    // Initialize managers. An explicit `--config <dir>` isolates settings,
    // state, and themes under that directory; otherwise fall back to the
    // default platform paths and environment variable overrides.
    //
    // An explicit config directory is validated fail-fast: if it cannot be
    // created or written to (e.g. an unwritable path from a `--config` typo),
    // surface a clear error and exit instead of starting a session whose state
    // will silently fail to persist.
    let mut theme_manager = FileThemeManager::new();
    let persistence = match jefe::startup::build_persistence(cli_args.config_dir.as_deref()) {
        Ok(manager) => manager,
        Err(error) => {
            write_startup_error(&error);
            std::process::exit(1);
        }
    };
    // Load themes: explicit --config dir takes precedence; otherwise load
    // from the default config dir's themes/ (JEFE_SETTINGS_PATH parent /
    // JEFE_CONFIG_DIR / platform default).
    let themes_dir = match cli_args.config_dir.as_deref() {
        Some(dir) => dir.join("themes"),
        None => jefe::persistence::default_themes_dir(),
    };
    theme_manager.load_from_dir(&themes_dir);
    let runtime = TmuxRuntimeManager::with_npm_executable(
        pty_rows,
        pty_cols,
        jefe::agent_detection::npm_path().map(std::path::Path::to_path_buf),
    );

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
