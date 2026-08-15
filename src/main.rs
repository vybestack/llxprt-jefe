//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

mod action_capture_emit;
mod action_context;
mod app_init;
mod app_input;
mod app_shell;
mod app_shell_attach;
mod app_shell_key_routing;
mod app_shell_liveness;
mod app_shell_panic;
mod app_shell_provider_worker;
mod app_shell_workers;
mod detail_wrap_map;
mod mouse_routing;
mod panic_capture;
mod pty_encoding;
mod state {
    pub use jefe::state::*;
}
mod terminal_init;

use std::io::Write;
use std::sync::Arc;

use iocraft::prelude::*;
use tracing::error;

use jefe::layout::{compute_pty_layout, is_fullscreen_enabled};
use jefe::runtime::TmuxRuntimeManager;
use jefe::theme::FileThemeManager;

/// Shared application context passed to the root component.
struct AppContext {
    keymap_snapshot: Option<jefe::domain::action_registry::ActionRegistrySnapshot>,
    settings_expected_hash: jefe::persistence::writer::ExpectedHash,
    settings_revision: u64,
    /// Whether `--config` isolated this session from the default locations.
    ///
    /// Settings reports it, because "which file am I actually editing" is the
    /// first thing to check when an edit appears not to have taken effect.
    config_isolated: bool,
    persistence: jefe::persistence::FilePersistenceManager,
    published_settings: jefe::persistence::settings_document::PublishedSettings,
    plugin_configs: std::collections::BTreeMap<
        jefe::domain::Id,
        jefe::messages::settings::SelectedPluginConfig,
    >,
    installed_plugin_configs: std::collections::BTreeMap<
        jefe::domain::Id,
        Vec<jefe::messages::settings::SelectedPluginConfig>,
    >,
    plugin_packages: Vec<jefe::persistence::plugin_inventory::InstalledPackage>,
    provider_containment: jefe::runtime::provider::Containment,
    theme_manager: FileThemeManager,
    runtime: TmuxRuntimeManager,
    /// `None` when the local JSP host could not start. Observation is
    /// optional telemetry, so Jefe still runs; agents simply launch
    /// uninstrumented and report telemetry as unsupported.
    jsp_host: Option<jefe::jsp_host::JspHostRuntime>,
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
    /// Prior runs that ended without recording a reason, found at startup.
    ///
    /// Consumed once when the app state is initialized, because the next start
    /// is the only moment at which a vanished run can still be attributed
    /// (issue #662).
    unclean_prior_runs: Vec<jefe::domain::UncleanRun>,
    /// Provider runtime coordinator (issue #390 CW-10, Slice D). Owns the
    /// persistent supervisor and the immutable provider action catalog.
    /// Held at the context boundary, never inside `AppState`.
    provider_coordinator: Option<jefe::runtime::provider::ProviderCoordinator>,
    /// Provider effect worker handle (issue #390 CW-10, Slice D). The input
    /// path pushes staged provider effects here; the background `use_future`
    /// drains and executes them via `smol::unblock`.
    provider_effect_handle: jefe::services::provider_effect_worker::ProviderEffectHandle,
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

fn write_optional_diagnostic(diagnostic: Option<String>) {
    if let Some(diagnostic) = diagnostic {
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(handle, "{diagnostic}");
    }
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
fn write_jsp_startup_error(error: &jefe::jsp_host::JspHostError) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(
        handle,
        "warning: local JSP host unavailable, agent telemetry disabled: {error}"
    );
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
    // `std::process::exit` runs no destructors, so flush the locked handle
    // explicitly to guarantee the report reaches piped/non-TTY consumers.
    let _ = handle.flush();
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

fn dispatch_binding_explain(cli_args: &jefe::cli::CliArgs) -> bool {
    let Some(explain) = cli_args.explain_binding.as_ref() else {
        return false;
    };
    let output = jefe::binding_explain::run(
        &explain.chord,
        explain.context.as_deref(),
        cli_args.config_dir.as_deref(),
    );
    write_recovery_output(&output);
    if output.exit_code != 0 {
        std::process::exit(i32::from(output.exit_code));
    }
    true
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
        _ => cli_args
            .plugin
            .as_ref()
            .map(|command| jefe::plugin_command::run(command, cli_args.config_dir.as_deref())),
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

fn runtime_manager(rows: u16, cols: u16, state_path: &std::path::Path) -> TmuxRuntimeManager {
    state_path.parent().map_or_else(
        || TmuxRuntimeManager::new(rows, cols),
        |parent| {
            TmuxRuntimeManager::with_session_host_root(
                rows,
                cols,
                parent.join(jefe::runtime::SESSION_HOST_ROOT_SEGMENT),
            )
        },
    )
}
fn init_diagnostics() {
    jefe::logging::init();
    panic_capture::install_panic_hook();
}

/// Run the interface and report why it stopped.
///
/// A render failure used to be logged and then discarded, leaving the exit
/// indistinguishable from the operator quitting. The reason is returned so the
/// run-end record can name it (issue #662).
fn run_app(context: Arc<std::sync::Mutex<AppContext>>) -> jefe::domain::RunEndReason {
    smol::block_on(async {
        let mut app = element!(app_shell::App(context: Some(context)));
        if is_fullscreen_enabled() {
            if let Err(error) = app.fullscreen().await {
                error!(%error, "fullscreen mode failed");
                return jefe::domain::RunEndReason::RenderFailed;
            }
        } else if let Err(error) = app.render_loop().await {
            error!(%error, "render loop failed");
            return jefe::domain::RunEndReason::RenderFailed;
        }
        jefe::domain::RunEndReason::UserQuit
    })
}

fn main() {
    run_internal_agent_launch_if_requested();
    let Some(cli_args) = parse_cli_or_exit() else {
        return;
    };
    if dispatch_binding_explain(&cli_args) || dispatch_recovery_command(&cli_args) {
        return;
    }

    // Dispatch doctor before startup persistence, logging, or TUI initialization.
    if cli_args.is_doctor() {
        run_doctor_and_exit(cli_args.config_dir.as_deref());
    }

    let mut startup = build_startup_or_exit(cli_args.config_dir.as_deref());
    let commit = commit_startup_or_exit(&mut startup, cli_args.config_dir.as_deref());
    // This temporary bridge is post-commit: no renderer can observe a partial
    // candidate while declaration consumers move to the aggregate in S4.
    publish_screen_registry_or_exit(&commit.workbench);
    run_tui(cli_args, startup, commit);
}

/// Bring logging up once persistence has validated, and record what it opened.
///
/// Deliberately after validation: a session that is about to refuse to start
/// over a malformed settings file should not first create log files for it.
fn init_startup_diagnostics(config_dir: Option<&std::path::Path>) {
    init_diagnostics();
    tracing::info!(version = jefe::VERSION, "jefe starting");
    tracing::debug!(
        log_file = ?jefe::logging::log_file_path(),
        config_dir = ?config_dir,
        "logging initialized"
    );
}

/// Build the agent runtime and the optional local JSP host it reports to.
///
/// The PTY viewport is derived from the dashboard geometry here rather than by
/// the caller, because the two are one decision: the runtime is sized for the
/// pane it will draw into.
fn build_runtime(
    state_path: &std::path::Path,
) -> (Option<jefe::jsp_host::JspHostRuntime>, TmuxRuntimeManager) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let layout = compute_pty_layout(cols, rows);
    let jsp_host = start_jsp_host(state_path);
    let mut runtime = runtime_manager(layout.pty_rows, layout.pty_cols, state_path);
    if let Some(host) = &jsp_host {
        runtime.install_jsp_launches(host.coordinator());
    }
    (jsp_host, runtime)
}

/// Start the local JSP host beside the state file.
///
/// Returns `None` when the host cannot start. Observation is optional
/// telemetry, so Jefe still runs and agents launch uninstrumented.
fn start_jsp_host(state_path: &std::path::Path) -> Option<jefe::jsp_host::JspHostRuntime> {
    let runtime_dir = state_path.parent().map_or_else(
        || std::path::PathBuf::from("jsp"),
        |parent| parent.join("jsp"),
    );
    match jefe::jsp_host::JspHostRuntime::start(runtime_dir) {
        Ok(host) => Some(host),
        Err(error) => {
            write_jsp_startup_error(&error);
            None
        }
    }
}

type PluginConfigCatalog =
    std::collections::BTreeMap<jefe::domain::Id, jefe::messages::settings::SelectedPluginConfig>;
type InstalledPluginConfigCatalog = std::collections::BTreeMap<
    jefe::domain::Id,
    Vec<jefe::messages::settings::SelectedPluginConfig>,
>;

fn plugin_config_catalogs(
    startup: &jefe::startup::StartupPersistence,
) -> (PluginConfigCatalog, InstalledPluginConfigCatalog) {
    let packages = &startup.plugin_packages;
    let selected =
        jefe::persistence::plugin_inventory::configured_packages(packages, &startup.settings)
            .into_iter()
            .filter_map(|package| {
                package.manifest().config().map(|schema| {
                    (
                        package.coordinate().id().owner_id().clone(),
                        jefe::messages::settings::SelectedPluginConfig {
                            version: package.coordinate().version().clone(),
                            schema: schema.clone(),
                            can_migrate: package.manifest().provider().mode()
                                != jefe::domain::plugin::ProviderMode::None,
                        },
                    )
                })
            })
            .collect();
    let mut installed = std::collections::BTreeMap::new();
    for package in packages {
        let Some(schema) = package.manifest().config() else {
            continue;
        };
        installed
            .entry(package.coordinate().id().owner_id().clone())
            .or_insert_with(Vec::new)
            .push(jefe::messages::settings::SelectedPluginConfig {
                version: package.coordinate().version().clone(),
                schema: schema.clone(),
                can_migrate: package.manifest().provider().mode()
                    != jefe::domain::plugin::ProviderMode::None,
            });
    }
    (selected, installed)
}

fn run_tui(
    cli_args: jefe::cli::CliArgs,
    startup: jefe::startup::StartupPersistence,
    commit: jefe::startup_commit::StartupCommit,
) {
    let (plugin_configs, installed_plugin_configs) = plugin_config_catalogs(&startup);
    let persist_paths = jefe::persistence::PersistencePaths {
        settings_path: startup.paths.settings.path.clone(),
        state_path: startup.paths.state.path.clone(),
    };
    let themes_dir = startup.paths.themes.clone();
    let keymap_diagnostic = startup.keymap_diagnostic_message();
    let settings_expected_hash = startup.settings_expected_hash;
    let startup_paths = startup.paths;
    let persistence = startup.manager;
    let jefe::startup_commit::StartupCommit {
        workbench,
        providers,
    } = commit;
    let keymap_snapshot = workbench.actions().clone();
    let published_settings = workbench.settings().clone();
    let plugin_packages = workbench.inventory().packages().to_vec();
    write_optional_diagnostic(keymap_diagnostic);
    init_startup_diagnostics(cli_args.config_dir.as_deref());

    // Claim the run boundary before anything else can fail: from here on the
    // run either records why it ended or leaves a marker saying it did not
    // (issue #662).
    let (run_guard, unclean_prior_runs) = jefe::run_diagnostics::begin_run(
        &jefe::persistence::run_marker::run_marker_dir(&startup_paths.state.path),
    );
    // Registered only once the run exists, so a teardown arriving during
    // startup keeps the host's default handling rather than reporting the end
    // of a run that never began.
    jefe::run_diagnostics::install_host_termination_handler();

    let mut theme_manager = FileThemeManager::new();
    theme_manager.load_from_dir(&themes_dir);
    let (jsp_host, runtime) = build_runtime(&startup_paths.state.path);

    let persist_handle =
        jefe::services::persist_worker::PersistHandle::new(build_persist_fn(persist_paths));
    let capture_handle = jefe::services::capture_worker::CaptureHandle::new();
    let provider_effect_handle =
        jefe::services::provider_effect_worker::ProviderEffectHandle::new();
    let provider_coordinator = Some(providers);
    let migration_containment = jefe::startup_commit::provider_containment(&startup_paths);

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        keymap_snapshot: Some(keymap_snapshot),
        settings_expected_hash,
        settings_revision: 0,
        config_isolated: cli_args.config_dir.is_some(),
        persistence,
        published_settings,
        plugin_configs,
        installed_plugin_configs,
        plugin_packages,
        provider_containment: migration_containment,
        theme_manager,
        runtime,
        jsp_host,
        gh_client: jefe::github::GhClient::new(),
        gh_deliveries: app_input::GhDeliveryHandle::default(),
        persist_handle,
        capture_handle,
        unclean_prior_runs,
        provider_coordinator,
        provider_effect_handle,
    }));

    let _console_guard = prepare_console_and_detect_font();
    let reason = run_app(context);
    run_guard.finish(reason);
}

/// Compose and publish the screen registry before anything renders.
///
/// Compiled descriptors and every enabled user definition become one candidate
/// set, which is published atomically. Stopping here with a diagnostic is better
/// than letting a renderer meet a half-formed or partially composed screen set,
/// where the failure would surface as unexplained geometry rather than as the
/// rule that was broken.
///
/// Warnings name definitions that were preserved on disk and left out of the
/// registry, which is not a reason to refuse to start.
fn commit_startup_or_exit(
    startup: &mut jefe::startup::StartupPersistence,
    config_dir: Option<&std::path::Path>,
) -> jefe::startup_commit::StartupCommit {
    match jefe::startup_commit::commit_startup(startup) {
        Ok(commit) => commit,
        Err(error) => {
            let stderr = std::io::stderr();
            let mut handle = stderr.lock();
            let _ = writeln!(handle, "jefe: {error}");
            let suffix =
                config_dir.map_or_else(String::new, |path| format!(" --config {}", path.display()));
            let _ = writeln!(handle, "jefe config validate{suffix}");
            let _ = writeln!(handle, "jefe config migrate-state{suffix}");
            std::process::exit(i32::from(error.exit_code()));
        }
    }
}

fn publish_screen_registry_or_exit(workbench: &jefe::published_workbench::PublishedWorkbench) {
    match jefe::startup_screens::publish_composed(workbench.screen_composition().clone()) {
        Ok(warnings) => {
            for warning in warnings {
                write_optional_diagnostic(Some(format!(
                    "{} {}: {}",
                    warning.code.as_str(),
                    warning.path.as_str(),
                    warning.redacted_detail
                )));
            }
        }
        Err(error) => {
            let stderr = std::io::stderr();
            let mut handle = stderr.lock();
            let _ = writeln!(handle, "jefe: {error}");
            std::process::exit(i32::from(error.exit_code()));
        }
    }
}

fn build_startup_or_exit(
    config_dir: Option<&std::path::Path>,
) -> jefe::startup::StartupPersistence {
    match jefe::startup::build_persistence(config_dir) {
        Ok(startup) => startup,
        Err(error) => {
            write_startup_error(&error, config_dir);
            std::process::exit(i32::from(error.exit_code));
        }
    }
}

/// Set the console output code page to UTF-8 (issue #434) and then probe the
/// console font for rounded-corner glyph coverage (issue #497).
///
/// The capability probe must run after the code page is set so it sees UTF-8
/// behavior. On Windows the returned guard restores the original code page on
/// drop and must stay alive for the duration of the render loop; on other
/// platforms no guard is returned.
fn prepare_console_and_detect_font() -> Option<terminal_init::ConsoleGuard> {
    let guard = terminal_init::prepare_console_for_unicode();
    jefe::border_capability::detect_and_initialize();
    guard
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
