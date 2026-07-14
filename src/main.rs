//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

mod app_init;
mod app_input;
mod app_shell;
mod app_shell_attach;
mod app_shell_workers;
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
    /// Coalescing persistence worker handle (issue #301). When present,
    /// `persist_state` schedules snapshots here instead of calling
    /// `save_state` synchronously on the input path.
    persist_handle: jefe::services::persist_worker::PersistHandle,
    /// Async capture worker handle (issue #301 Phase 2). When present, the
    /// render path requests a background capture instead of calling
    /// `capture_history` synchronously.
    capture_handle: jefe::services::capture_worker::CaptureHandle,
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

fn run_internal_agent_launch_if_requested() {
    let mut args = std::env::args();
    let _program = args.next();
    if args.next().as_deref() != Some(jefe::runtime::INTERNAL_LAUNCH_ARGUMENT) {
        return;
    }
    let Some(plan_path) = args.next() else {
        std::process::exit(2);
    };
    if args.next().is_some() {
        std::process::exit(2);
    }
    match jefe::runtime::run_launch_plan(std::path::Path::new(&plan_path)) {
        Ok(status) => {
            let code = status.code().map_or(1, |value| value);
            std::process::exit(code);
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "internal agent launch failed: {error}");
            std::process::exit(1);
        }
    }
}

fn main() {
    run_internal_agent_launch_if_requested();
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
    let runtime = TmuxRuntimeManager::new(pty_rows, pty_cols);

    let persist_handle = jefe::services::persist_worker::PersistHandle::new(build_persist_fn(
        cli_args.config_dir.as_deref(),
    ));
    let capture_handle = jefe::services::capture_worker::CaptureHandle::new();

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        persistence,
        theme_manager,
        runtime,
        gh_client: jefe::github::GhClient::new(),
        persist_handle,
        capture_handle,
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

/// Build the coalescing persistence worker's durable-write boundary (issue #301).
///
/// The worker calls this function on a background OS thread; the input path
/// never touches the filesystem directly. This reuses the persistence
/// manager already constructed at startup (via `build_persistence`) by
/// re-invoking the same factory function — the startup instance and the
/// worker instance operate on the same config dir and file, so writes are
/// consistent. A second `build_persistence` call is used rather than moving
/// the startup instance because the startup instance is owned by
/// `AppContext` and used for synchronous reads (e.g. initial state load);
/// the worker needs its own `Arc<Mutex<>>` to avoid lock contention with
/// the input path.
fn build_persist_fn(
    config_dir: Option<&std::path::Path>,
) -> jefe::services::persist_worker::PersistFn {
    use jefe::persistence::PersistenceManager;
    let manager =
        jefe::startup::build_persistence(config_dir).map(|m| Arc::new(std::sync::Mutex::new(m)));
    match manager {
        Ok(m) => {
            let manager = Arc::clone(&m);
            Arc::new(move |state: &jefe::persistence::State| match manager.lock() {
                Ok(mgr) => mgr
                    .save_state(state)
                    .map_err(|e: jefe::persistence::PersistenceError| e.to_string()),
                Err(poisoned) => {
                    tracing::warn!("persist worker: mutex poisoned; recovering");
                    poisoned
                        .into_inner()
                        .save_state(state)
                        .map_err(|e| e.to_string())
                }
            })
        }
        Err(e) => {
            tracing::warn!(error = %e, "persist worker: build_persistence failed; durable writes disabled");
            Arc::new(|_: &jefe::persistence::State| Ok(()))
        }
    }
}
