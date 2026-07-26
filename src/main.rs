//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

mod app_init;
mod app_input;
mod app_shell;
mod app_shell_attach;
mod app_shell_key_routing;
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
    /// Root-owned delivery slot for background GitHub request results.
    gh_deliveries: app_input::GhDeliveryHandle,
    /// Coalescing persistence worker handle (issue #301). When present,
    /// `persist_state` schedules durable save requests here instead of
    /// writing synchronously on the input path.
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
            std::process::exit(i32::from(e.exit_code()));
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

fn write_recovery_output(output: &jefe::recovery::RecoveryOutput) {
    if !output.stdout.is_empty() {
        write_stdout_line(&output.stdout);
    }
    if !output.stderr.is_empty() {
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(handle, "{}", output.stderr);
    }
}

fn write_cli_error(error: &jefe::cli::CliError) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(handle, "error: {error}");
    let _ = writeln!(handle);
    let _ = writeln!(handle, "{}", jefe::cli::USAGE);
}

/// Print a typed startup diagnostic and exact offline recovery commands.
fn write_startup_error(
    error: &jefe::persistence::paths::PathError,
    config_dir: Option<&std::path::Path>,
) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let rendered = serde_json::to_string_pretty(error.diagnostic.as_ref())
        .unwrap_or_else(|_| error.diagnostic.redacted_detail.clone());
    let _ = writeln!(handle, "{rendered}");
    let suffix =
        config_dir.map_or_else(String::new, |path| format!(" --config {}", path.display()));
    let _ = writeln!(handle, "jefe config validate{suffix}");
    let _ = writeln!(handle, "jefe config migrate-state{suffix}");
}

/// Run the read-only `jefe doctor` diagnostics, write the redacted report to
/// locked stdout, and exit with the typed outcome code (issue #264).
///
/// Dispatched before logging/TUI initialization so it never starts a session
/// or mutates persistence state.
fn run_doctor_and_exit(config_dir: Option<&std::path::Path>) {
    let report = jefe::doctor::collect(config_dir);
    let outcome = jefe::doctor::classify_doctor(report.findings());
    let rendered = jefe::doctor::render_report(&report);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle, "{rendered}");
    std::process::exit(i32::from(outcome.exit_code().as_u8()));
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

fn dispatch_recovery_command(cli_args: &jefe::cli::CliArgs) -> bool {
    let output = match cli_args.command {
        Some(jefe::cli::ConfigCommand::Path) => {
            Some(jefe::recovery::run_path(cli_args.config_dir.as_deref()))
        }
        Some(jefe::cli::ConfigCommand::Validate) => {
            Some(jefe::recovery::run_validate(cli_args.config_dir.as_deref()))
        }
        Some(jefe::cli::ConfigCommand::ShowEffective { provenance }) => Some(
            jefe::recovery::run_show_effective(cli_args.config_dir.as_deref(), provenance),
        ),
        Some(jefe::cli::ConfigCommand::Edit) => {
            Some(jefe::recovery::run_edit(cli_args.config_dir.as_deref()))
        }
        Some(jefe::cli::ConfigCommand::Format { check, migrate }) => Some(
            jefe::recovery::run_format(cli_args.config_dir.as_deref(), check, migrate),
        ),
        Some(jefe::cli::ConfigCommand::MigrateState) => Some(jefe::recovery::run_migrate_state(
            cli_args.config_dir.as_deref(),
        )),
        _ => None,
    };
    let Some(output) = output else {
        return false;
    };
    write_recovery_output(&output);
    if output.exit_code != 0 {
        std::process::exit(i32::from(output.exit_code));
    }
    true
}

fn main() {
    run_internal_agent_launch_if_requested();
    let Some(cli_args) = parse_cli_or_exit() else {
        return;
    };
    if dispatch_recovery_command(&cli_args) {
        return;
    }

    // Dispatch doctor before startup persistence, logging, or TUI initialization.
    if cli_args.is_doctor() {
        run_doctor_and_exit(cli_args.config_dir.as_deref());
    }

    let startup = match jefe::startup::build_persistence(cli_args.config_dir.as_deref()) {
        Ok(startup) => startup,
        Err(error) => {
            write_startup_error(&error, cli_args.config_dir.as_deref());
            std::process::exit(i32::from(error.exit_code));
        }
    };
    let persist_paths = jefe::persistence::PersistencePaths {
        settings_path: startup.paths.settings.path.clone(),
        state_path: startup.paths.state.path.clone(),
    };
    let themes_dir = startup.paths.themes.clone();
    let persistence = startup.manager;

    // Initialize structured logging only after persistence has validated.
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

    let mut theme_manager = FileThemeManager::new();
    theme_manager.load_from_dir(&themes_dir);
    let runtime = TmuxRuntimeManager::new(pty_rows, pty_cols);

    let persist_handle =
        jefe::services::persist_worker::PersistHandle::new(build_persist_fn(persist_paths));
    let capture_handle = jefe::services::capture_worker::CaptureHandle::new();

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        persistence,
        theme_manager,
        runtime,
        gh_client: jefe::github::GhClient::new(),
        gh_deliveries: app_input::GhDeliveryHandle::default(),
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
/// never touches the filesystem directly. The worker receives its own manager
/// over the exact startup-resolved [`jefe::persistence::PersistencePaths`], so
/// worker writes and startup reads share one selected authority without
/// re-running startup validation or lock contention with the input path.
fn build_persist_fn(
    paths: jefe::persistence::PersistencePaths,
) -> jefe::services::persist_worker::PersistFn {
    let manager = Arc::new(std::sync::Mutex::new(
        jefe::persistence::FilePersistenceManager::with_paths(paths),
    ));
    Arc::new(
        move |request: &jefe::services::persist_worker::PersistRequest, generation, freshness| {
            // The writer fences on the worker's schedule generation, not the
            // document revision: the candidate already carries its own
            // revision, while `generation` decides which in-flight write is
            // still the newest.
            let candidate = request.candidate.as_ref();
            let result = match manager.lock() {
                Ok(mgr) => mgr.save_state_v2_revisioned(candidate, generation, freshness),
                Err(poisoned) => {
                    tracing::warn!("persist worker: mutex poisoned; recovering");
                    poisoned
                        .into_inner()
                        .save_state_v2_revisioned(candidate, generation, freshness)
                }
            };
            result.map_err(|error| error.to_string())
        },
    )
}
