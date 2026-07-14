//! Tmux integration helpers: scenario loading, request building, and manifest
//! finalization.
//!
//! Extracted from `main.rs` to keep file sizes under the project limit.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::cli::write_stderr;

use jefe::harness::{CaptureHook, TmuxPaneSize, TmuxStartRequest, parse_scenario};
use jefe_tutorial_capture::{
    ArtifactEntry, ArtifactKind, OwnedPathKind, RunManifest, RunOutcome, compute_binary_hash,
    compute_scenario_hash, save_manifest, validate_artifact_path,
};

pub use super::svg_helpers::render_single_artifact;

/// Load and parse a scenario JSON file.
pub fn load_scenario(scenario_path: &Path) -> Result<jefe::harness::Scenario, ExitCode> {
    let json = match fs::read_to_string(scenario_path) {
        Ok(value) => value,
        Err(err) => {
            write_stderr(&format!(
                "failed to read scenario '{}': {err}\n",
                scenario_path.display()
            ));
            return Err(ExitCode::from(1));
        }
    };
    match parse_scenario(&json) {
        Ok(value) => Ok(value),
        Err(err) => {
            write_stderr(&format!("failed to parse scenario: {err}\n"));
            Err(ExitCode::from(1))
        }
    }
}

/// Resolve the jefe binary to an absolute path and verify it exists.
///
/// Fail-fast: if the binary does not exist on disk, returns an error
/// immediately rather than allowing tmux to fail later with a confusing
/// message.
pub fn resolve_jefe_bin(jefe_bin: &Path) -> Result<PathBuf, String> {
    let resolved = if jefe_bin.is_absolute() {
        jefe_bin.to_path_buf()
    } else {
        let cwd = env::current_dir().map_err(|e| format!("cannot get current dir: {e}"))?;
        cwd.join(jefe_bin)
    };
    // Finding: fail fast if the binary does not exist, rather than letting
    // tmux produce a confusing startup error.
    if !resolved.exists() {
        return Err(format!(
            "jefe binary not found at '{}'. Build it first with 'cargo build' or check the --jefe-bin path.",
            resolved.display()
        ));
    }
    Ok(resolved)
}

/// Build the tmux start request for the Jefe binary.
///
/// Uses the manifest's config directory (recorded during `prepare`) as the
/// `--config` argument, and the fixture repo as the working directory. Falls
/// back to the current directory if the fixture repo path is absent.
pub fn build_tmux_request(
    manifest: &RunManifest,
    jefe_bin: &Path,
    controlled_path: &str,
    keep_session: bool,
) -> Result<TmuxStartRequest, String> {
    let jefe_bin = resolve_jefe_bin(jefe_bin)?;
    let dims = TmuxPaneSize::new(manifest.cols, manifest.rows, 2000);
    let session_name = format!("jefe-tutorial-{}", manifest.run_id.as_str());
    let config_dir = manifest
        .find_path_by_kind(OwnedPathKind::ConfigDir)
        .ok_or_else(|| "manifest missing config directory".to_string())?;
    let working_dir = manifest
        .fixture_repo_path
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    // Finding: fail fast if the working directory does not exist.
    if !working_dir.exists() {
        return Err(format!(
            "working directory '{}' does not exist. Run 'prepare' first to create the fixture repository.",
            working_dir.display()
        ));
    }
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_bin,
        config_dir.to_path_buf(),
        working_dir.to_path_buf(),
        dims,
    )
    .map_err(|err| err.to_string())?;
    Ok(request
        .with_keep_session(keep_session)
        .with_env_path(controlled_path.to_string())
        .with_suppress_status_bar(true))
}

/// Tool-owned capture hook that suppresses the tmux status bar on the
/// tutorial-capture owned nested agent session(s) immediately before each
/// screen capture, so the status bar never leaks into captures.
///
/// ## Lifecycle (BLOCKER 1 fix)
///
/// This hook is **lifecycle-aware**: it discovers the run-owned agent
/// identity at capture time through the narrow persistence-owned
/// [`jefe::persistence::seed::read_agent_ids`] API, rather than hard-coding
/// an agent name. The behavior depends on whether an agent exists yet:
///
/// - **Pre-agent captures** (no agent in `state.json`): the hook is a
///   **no-op**. This supports scenarios that capture the dashboard before
///   any agent is created.
/// - **Post-agent captures** (one or more agents exist): the hook targets
///   **exactly** those sessions (named `jefe-<agent_id>` via
///   [`jefe::runtime::RuntimeSession::session_name_for`]) on Jefe's private
///   tmux socket. A suppression failure for a **known** session is
///   **fatal** — the run does not silently produce artifacts with a status
///   bar.
///
/// Already-suppressed agent IDs are tracked so the (idempotent) tmux call
/// is issued only once per agent per run.
///
/// This adapter targets **only** the owned nested agent sessions. It does not
/// touch the harness session, the default tmux server, or any other session.
/// The core harness knows nothing about this behavior — it just calls the
/// generic [`CaptureHook`] seam before each capture.
#[derive(Debug, Clone)]
pub struct StatusSuppressHook<R: CommandRunner = ProcessCommandRunner> {
    /// Absolute path to Jefe's private tmux socket (resolved via
    /// `jefe::runtime::jefe_tmux_socket_path`).
    jefe_socket: PathBuf,
    /// The isolated Jefe config directory used to discover run-owned agent
    /// IDs at capture time.
    config_dir: PathBuf,
    /// Agent IDs whose status bar has already been suppressed during this
    /// run. Tracked so the (idempotent) tmux call is issued only once per
    /// agent.
    suppressed: std::collections::BTreeSet<String>,
    /// Injectable command runner so unit tests can verify argv planning and
    /// hook behavior without invoking real tmux.
    runner: R,
}

/// Narrow seam for executing an external command with a given argv vector.
///
/// Production uses [`ProcessCommandRunner`] which spawns a real
/// `std::process::Command`. Unit tests inject a recording fake that captures
/// the argv without spawning any process, proving command planning targets
/// the exact private socket and owned session.
pub trait CommandRunner: std::fmt::Debug {
    /// Execute `program` with the given `args`. Returns the combined
    /// success/failure status and stderr on error.
    ///
    /// # Errors
    ///
    /// Returns an error string describing why the command failed to execute
    /// or returned a non-zero exit status.
    fn run(&self, program: &str, args: &[String]) -> Result<(), String>;
}

/// Production command runner that spawns a real `std::process::Command`.
///
/// A bounded timeout is enforced so a hung or unresponsive `tmux` invocation
/// cannot stall the capture run indefinitely. The timeout prevents the
/// harness from blocking forever when the private socket is unresponsive
/// (e.g. a crashed multiplexer server or a permissions deadlock).
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessCommandRunner;

/// Bounded timeout for the tool status-suppression command. Generous enough
/// for a healthy local tmux server, but finite so a hung command surfaces as
/// a fatal hook error rather than blocking forever.
const STATUS_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<(), String> {
        use std::io::Read;
        let mut command = std::process::Command::new(program);
        command.args(args);
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| format!("spawn {program}: {e}"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("capture {program} stderr"))?;
        let stderr_reader = std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut buffer = String::new();
            let _ = stderr.read_to_string(&mut buffer);
            buffer
        });
        let deadline = std::time::Instant::now() + STATUS_COMMAND_TIMEOUT;
        let status = loop {
            if let Ok(Some(status)) = child.try_wait() {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_reader.join();
                return Err(format!(
                    "{program} {} timed out after {:?}",
                    args.join(" "),
                    STATUS_COMMAND_TIMEOUT
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let stderr = stderr_reader.join().unwrap_or_default();
        if status.success() {
            return Ok(());
        }
        if stderr.trim().is_empty() {
            return Err(format!(
                "{program} {} failed (exit {:?})",
                args.join(" "),
                status.code()
            ));
        }
        Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            stderr.trim()
        ))
    }
}

/// Pure function: build the ordered tmux argv for suppressing the status bar
/// on a specific session via a specific private socket.
///
/// This is separated from command execution so unit tests can verify the exact
/// argv (socket path, session name, `status off`) without invoking real tmux.
/// The argv order matches the tmux CLI contract:
///   `tmux -S <socket> set-option -t <session> status off`
#[must_use]
pub fn status_suppress_argv(jefe_socket: &Path, agent_session_name: &str) -> Vec<String> {
    vec![
        "-S".to_string(),
        jefe_socket.to_string_lossy().into_owned(),
        "set-option".to_string(),
        "-t".to_string(),
        agent_session_name.to_string(),
        "status".to_string(),
        "off".to_string(),
    ]
}

impl StatusSuppressHook<ProcessCommandRunner> {
    /// Construct a hook that discovers run-owned agent IDs from the given
    /// isolated config directory at capture time.
    ///
    /// The hook targets the nested agent session(s) (named `jefe-<agent_id>`)
    /// on Jefe's private socket. When no agent exists yet, the hook is a
    /// no-op; once an agent exists, it targets exactly that session.
    #[must_use]
    pub fn new(jefe_socket: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            jefe_socket,
            config_dir,
            suppressed: std::collections::BTreeSet::new(),
            runner: ProcessCommandRunner,
        }
    }
}

impl<R: CommandRunner> StatusSuppressHook<R> {
    /// Construct a hook with an injectable command runner, for unit tests
    /// that need to verify argv planning without invoking real tmux.
    #[cfg(test)]
    #[must_use]
    pub fn with_runner(jefe_socket: PathBuf, config_dir: PathBuf, runner: R) -> Self {
        Self {
            jefe_socket,
            config_dir,
            suppressed: std::collections::BTreeSet::new(),
            runner,
        }
    }

    /// Read agent IDs whose persisted lifecycle implies an active nested
    /// runtime session. Queued agents are intentionally ignored.
    fn discover_agent_ids(&self) -> Result<Vec<jefe::domain::AgentId>, String> {
        jefe::persistence::seed::read_active_agent_ids(&self.config_dir)
            .map_err(|err| err.to_string())
    }
}

impl<R: CommandRunner> CaptureHook for StatusSuppressHook<R> {
    fn before_capture(&mut self, _label: &str) -> Result<(), String> {
        // Discover the run-owned agent IDs at capture time through the
        // narrow persistence-owned API. This avoids hard-coding an agent
        // name and supports the lifecycle where an agent may not exist yet.
        let agent_ids = self.discover_agent_ids()?;

        // Pre-agent captures: no agents in state.json → no-op. The scenario
        // may capture the dashboard before any agent is created.
        if agent_ids.is_empty() {
            return Ok(());
        }

        // Once agents exist, target exactly their nested sessions. Failure
        // for a known session is fatal.
        for agent_id in &agent_ids {
            let id_str = agent_id.0.as_str();
            if self.suppressed.contains(id_str) {
                continue;
            }
            let session_name = nested_agent_session_name(id_str);
            let argv = status_suppress_argv(&self.jefe_socket, &session_name);
            self.runner.run("tmux", &argv).map_err(|reason| {
                format!(
                    "tmux set-option status off failed on socket '{}' session '{}': {reason}",
                    self.jefe_socket.display(),
                    session_name,
                )
            })?;
            self.suppressed.insert(id_str.to_string());
        }
        Ok(())
    }
}

/// Resolve the nested agent session name that Jefe will create for the seeded
/// agent, delegating to the canonical root runtime session-name policy
/// ([`jefe::runtime::RuntimeSession::session_name_for`]) so the tool never
/// duplicates the naming convention.
#[must_use]
pub fn nested_agent_session_name(agent_id: &str) -> String {
    jefe::runtime::RuntimeSession::session_name_for(&jefe::domain::AgentId(agent_id.to_string()))
}

/// Update the manifest outcome and save it.
pub fn update_manifest_outcome(
    manifest: &RunManifest,
    manifest_path: &Path,
    outcome: RunOutcome,
) -> Result<(), String> {
    let mut updated = manifest.clone();
    updated.set_outcome(outcome);
    save_manifest(&updated, manifest_path)
        .map_err(|err| format!("failed to update manifest outcome: {err}"))
}

/// Register captured artifacts and set outcome to Success in a single atomic
/// manifest update, avoiding the overwrite race between separate saves.
///
/// **Finding #9**: Also records observed actions (from capture labels) and
/// discrepancies (from soft failures) in the manifest so the report is
/// truthful about what was observed during the run.
///
/// Artifact relative paths are validated for safety (no traversal).
#[cfg(test)]
pub fn finalize_manifest_success(
    manifest: &RunManifest,
    manifest_path: &Path,
    captures: &[String],
) -> Result<(), String> {
    finalize_manifest_with_scenario(manifest, manifest_path, captures, &[], None)
}

/// Extended finalization that also derives observed actions from the scenario
/// steps that were executed.
///
/// **Finding #8**: Observed actions are derived from the actual scenario steps
/// rather than just from capture labels. Each key, type, and capture step is
/// recorded as an observed action with its actual keybinding or text.
pub fn finalize_manifest_with_scenario(
    manifest: &RunManifest,
    manifest_path: &Path,
    captures: &[String],
    soft_failures: &[jefe::harness::RunnerFailure],
    scenario: Option<&jefe::harness::Scenario>,
) -> Result<(), String> {
    let mut updated = manifest.clone();
    register_capture_artifacts(&mut updated, manifest_path, captures);
    register_observed_actions(&mut updated, scenario, captures);
    for failure in soft_failures {
        updated.add_discrepancy(format!(
            "step {} ({}): {}",
            failure.step_index, failure.step_kind, failure.reason
        ));
    }
    updated.set_outcome(if soft_failures.is_empty() {
        RunOutcome::Success
    } else {
        RunOutcome::Partial
    });
    save_manifest(&updated, manifest_path)
        .map_err(|err| format!("failed to finalize manifest: {err}"))
}

/// Register screen-capture and ANSI artifacts for each capture checkpoint.
fn register_capture_artifacts(
    manifest: &mut RunManifest,
    manifest_path: &Path,
    captures: &[String],
) {
    for name in captures {
        let sanitized: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let relative_path = PathBuf::from(format!("{sanitized}.screen.txt"));
        if let Err(err) = validate_artifact_path(&relative_path) {
            write_stderr(&format!(
                "warning: skipping unsafe artifact path '{}': {err}\n",
                relative_path.display()
            ));
            continue;
        }
        let _ = manifest.add_artifact(ArtifactEntry {
            label: name.clone(),
            relative_path,
            kind: ArtifactKind::ScreenCapture,
        });
        register_ansi_artifact(manifest, manifest_path, name, &sanitized);
    }
}

/// Register the matching ANSI capture artifact if the file exists.
fn register_ansi_artifact(
    manifest: &mut RunManifest,
    manifest_path: &Path,
    name: &str,
    sanitized: &str,
) {
    let ansi_relative = PathBuf::from(format!("{sanitized}.screen.ansi"));
    if let Err(err) = validate_artifact_path(&ansi_relative) {
        write_stderr(&format!(
            "warning: skipping unsafe ANSI artifact path '{}': {err}\n",
            ansi_relative.display()
        ));
        return;
    }
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let ansi_full = manifest_dir.join("artifacts").join(&ansi_relative);
    if ansi_full.exists() {
        let _ = manifest.add_artifact(ArtifactEntry {
            label: format!("{name}-ansi"),
            relative_path: ansi_relative,
            kind: ArtifactKind::AnsiCapture,
        });
    }
}

/// Register observed actions from scenario steps or capture labels.
fn register_observed_actions(
    manifest: &mut RunManifest,
    scenario: Option<&jefe::harness::Scenario>,
    captures: &[String],
) {
    if let Some(scenario) = scenario {
        derive_observed_actions_from_scenario(manifest, &scenario.steps, captures);
    } else {
        for name in captures {
            manifest.add_observed_action(
                "capture",
                format!("captured checkpoint: {name}"),
                Some(name.clone()),
            );
        }
    }
}

/// Derive observed actions from the actual scenario steps executed.
/// Each key/type/capture step becomes an observed action with its actual
/// keybinding or text content.
fn derive_observed_actions_from_scenario(
    manifest: &mut RunManifest,
    steps: &[jefe::harness::Step],
    captures: &[String],
) {
    use jefe::harness::Step;
    let capture_set: std::collections::BTreeSet<&String> = captures.iter().collect();
    for step in steps {
        match step {
            Step::Key { key } => {
                manifest.add_observed_action(key.clone(), format!("key: {key}"), None);
            }
            Step::Keys { keys } => {
                let combined = keys.join("+");
                manifest.add_observed_action(combined.clone(), format!("keys: {combined}"), None);
            }
            Step::Type { text } => {
                // Truncate long text for the keybinding field.
                let display: String = text.chars().take(40).collect();
                manifest.add_observed_action(
                    format!("type:{display}"),
                    format!("typed: {display}"),
                    None,
                );
            }
            Step::Line { text } => {
                let display: String = text.chars().take(40).collect();
                manifest.add_observed_action(
                    format!("line:{display}"),
                    format!("entered line: {display}"),
                    None,
                );
            }
            Step::Capture { name } => {
                // Only record if the capture was actually made (in captures list).
                if capture_set.contains(name) {
                    manifest.add_observed_action(
                        "capture",
                        format!("captured checkpoint: {name}"),
                        Some(name.clone()),
                    );
                }
            }
            Step::HistorySample { name } => {
                manifest.add_observed_action(
                    "history-sample",
                    format!("sampled history: {name}"),
                    Some(name.clone()),
                );
            }
            Step::WaitFor { pattern } => {
                let display: String = pattern.chars().take(40).collect();
                manifest.add_observed_action("waitFor", format!("waited for: {display}"), None);
            }
            Step::Expect { pattern } => {
                let display: String = pattern.chars().take(40).collect();
                manifest.add_observed_action(
                    "expect",
                    format!("asserted present: {display}"),
                    None,
                );
            }
            // Other steps (Wait, CopyMode, WaitForExit, etc.) are not
            // user-facing actions and are not recorded.
            _ => {}
        }
    }
}

/// Resolve the run-scoped socket used by nested Jefe agent sessions.
#[must_use]
pub fn nested_jefe_socket_path(run_root: &Path) -> PathBuf {
    run_root.join("jefe-runtime.sock")
}

/// Build the tmux start request for the Jefe binary using the fixture-clone
/// directory as the working directory (for GitHub capture scenarios).
///
/// This variant is used by `capture-github` after `seed_tier_b_state` has
/// populated the isolated config with the cloned repo. Jefe is launched from
/// the fixture-clone so it immediately sees the seeded repo/agent.
pub fn build_github_tmux_request(
    manifest: &RunManifest,
    jefe_bin: &Path,
    controlled_path: &str,
    keep_session: bool,
    manifest_dir: &Path,
) -> Result<TmuxStartRequest, String> {
    let jefe_bin = resolve_jefe_bin(jefe_bin)?;
    let dims = TmuxPaneSize::new(manifest.cols, manifest.rows, 2000);
    let session_name = format!("jefe-tutorial-{}", manifest.run_id.as_str());
    let config_dir = manifest
        .find_path_by_kind(OwnedPathKind::ConfigDir)
        .ok_or_else(|| "manifest missing config directory".to_string())?;
    let expected_working_dir = manifest_dir.join("fixture-clone");
    let working_dir = manifest
        .find_path_by_kind(OwnedPathKind::FixtureClone)
        .ok_or_else(|| "manifest missing owned fixture clone".to_string())?;
    if working_dir != expected_working_dir || !working_dir.is_dir() {
        return Err(format!(
            "fixture clone is not the expected owned working directory: {}",
            expected_working_dir.display()
        ));
    }
    let metadata = std::fs::symlink_metadata(working_dir).map_err(|err| err.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("fixture clone working directory must not be a symlink".to_string());
    }
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_bin,
        config_dir.to_path_buf(),
        working_dir.to_path_buf(),
        dims,
    )
    .map_err(|err| err.to_string())?;
    request
        .with_keep_session(keep_session)
        .with_env_path(controlled_path.to_string())
        .with_suppress_status_bar(true)
        .with_extra_env(
            "JEFE_SOCKET_PATH",
            nested_jefe_socket_path(manifest_dir).to_string_lossy(),
        )
        .map_err(|err| err.to_string())
}

/// Discover all produced text/ANSI artifacts in the artifact directory and
/// register them in the manifest. This is used on hard capture failure to
/// ensure all partial evidence is captured before the atomic save.
///
/// Enrich the manifest with the exact binary, scenario, and theme used for capture.
pub fn enrich_manifest_with_capture_metadata(
    manifest: &mut RunManifest,
    jefe_bin: &Path,
    scenario_path: &Path,
) {
    manifest.binary_hash = compute_binary_hash(jefe_bin);
    match compute_scenario_hash(scenario_path) {
        Ok(hash) => manifest.scenario_hash = Some(hash),
        Err(err) => {
            if manifest.scenario_hash.is_some() {
                manifest.add_discrepancy(format!(
                    "cleared stale scenario hash (recompute failed: {err})"
                ));
            }
            manifest.scenario_hash = None;
        }
    }
    if let Some(stem) = scenario_path.file_stem().and_then(|value| value.to_str()) {
        manifest.scenario_name = stem.to_string();
    }
    if manifest.theme.is_none() {
        manifest.theme = Some("green-screen".to_string());
    }
}

/// **Finding**: On hard failure, the scenario may have produced screen
/// captures (.screen.txt), ANSI captures (.screen.ansi), and other artifacts
/// before failing. These must be discovered and registered so the manifest
/// truthfully reflects what was produced, even on failure.
pub fn discover_and_register_artifacts(manifest: &mut RunManifest, artifact_dir: &Path) {
    let Ok(entries) = fs::read_dir(artifact_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(rel) = path.strip_prefix(artifact_dir).ok() else {
            continue;
        };
        let rel_str = rel.to_string_lossy();
        let (label, kind) = if rel_str.ends_with(".screen.txt") {
            let stem = rel
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("capture")
                .trim_end_matches(".screen")
                .to_string();
            (stem, ArtifactKind::ScreenCapture)
        } else if rel_str.ends_with(".screen.ansi") {
            let stem = rel
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("capture")
                .trim_end_matches(".screen")
                .to_string();
            (format!("{stem}-ansi"), ArtifactKind::AnsiCapture)
        } else if rel_str.ends_with(".txt") || rel_str.ends_with(".ansi") {
            (
                rel.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("artifact")
                    .to_string(),
                ArtifactKind::ScreenCapture,
            )
        } else {
            continue;
        };
        // Validate the relative path for safety.
        if validate_artifact_path(rel).is_err() {
            continue;
        }
        if let Err(err) = manifest.add_artifact(ArtifactEntry {
            label,
            relative_path: rel.to_path_buf(),
            kind,
        }) {
            manifest.add_discrepancy(format!(
                "failed to register discovered artifact '{}': {err}",
                rel.display()
            ));
        }
    }
}
