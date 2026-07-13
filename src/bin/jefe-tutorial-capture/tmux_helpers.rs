//! Tmux integration helpers: scenario loading, request building, and manifest
//! finalization.
//!
//! Extracted from `main.rs` to keep file sizes under the project limit.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cli::write_stderr;

use jefe::harness::{TmuxPaneSize, TmuxStartRequest, parse_scenario, run_tmux_scenario};
use jefe::tutorial_capture::{
    ArtifactEntry, ArtifactKind, OwnedPathKind, RunManifest, RunOutcome, compute_binary_hash,
    compute_scenario_hash, detection_path_for, load_manifest, redact_artifacts,
    redact_artifacts_with_repos, save_manifest, validate_artifact_path,
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

/// Resolve the jefe binary to an absolute path so it can be found regardless
/// of the tmux session's working directory.
pub fn resolve_jefe_bin(jefe_bin: &Path) -> Result<PathBuf, String> {
    if jefe_bin.is_absolute() {
        return Ok(jefe_bin.to_path_buf());
    }
    let cwd = env::current_dir().map_err(|e| format!("cannot get current dir: {e}"))?;
    Ok(cwd.join(jefe_bin))
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
        .with_extra_env("JEFE_TUTORIAL_CAPTURE", "1"))
}

/// Update the manifest outcome and save it.
pub fn update_manifest_outcome(manifest: &RunManifest, manifest_path: &Path, outcome: RunOutcome) {
    let mut updated = manifest.clone();
    updated.set_outcome(outcome);
    if let Err(err) = save_manifest(&updated, manifest_path) {
        write_stderr(&format!("warning: failed to update manifest: {err}\n"));
    }
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
) {
    finalize_manifest_with_scenario(manifest, manifest_path, captures, &[], None);
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
) {
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
    if let Err(err) = save_manifest(&updated, manifest_path) {
        write_stderr(&format!("warning: failed to finalize manifest: {err}\n"));
    }
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
        manifest.add_artifact(ArtifactEntry {
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
        manifest.add_artifact(ArtifactEntry {
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
    let working_dir = manifest_dir.join("fixture-clone");
    let request = TmuxStartRequest::jefe(
        session_name,
        jefe_bin,
        config_dir.to_path_buf(),
        working_dir,
        dims,
    )
    .map_err(|err| err.to_string())?;
    Ok(request
        .with_keep_session(keep_session)
        .with_env_path(controlled_path.to_string())
        .with_extra_env("JEFE_TUTORIAL_CAPTURE", "1"))
}

/// Discover all produced text/ANSI artifacts in the artifact directory and
/// register them in the manifest. This is used on hard capture failure to
/// ensure all partial evidence is captured before the atomic save.
///
/// **Finding**: On hard failure, the scenario may have produced screen
/// captures (.screen.txt), ANSI captures (.screen.ansi), and other artifacts
/// before failing. These must be discovered and registered so the manifest
/// truthfully reflects what was produced, even on failure.
#[cfg(test)]
pub fn discover_and_register_artifacts(manifest: &mut RunManifest, artifact_dir: &Path) {
    discover_and_register_artifacts_impl(manifest, artifact_dir);
}

fn discover_and_register_artifacts_impl(manifest: &mut RunManifest, artifact_dir: &Path) {
    let manifest_dir = artifact_dir.parent().unwrap_or_else(|| Path::new("."));
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
        manifest.add_artifact(ArtifactEntry {
            label,
            relative_path: rel.to_path_buf(),
            kind,
        });
    }
    let _ = manifest_dir;
}

/// Handle capture failure: discover all produced artifacts, redact, register,
/// mark failed, and save atomically.
///
/// **Finding**: On hard capture failure, all produced plain/ANSI/failure
/// artifacts must be discovered and registered in the manifest BEFORE the
/// atomic save, so partial evidence is captured. This happens AFTER redaction
/// so artifacts are scrubbed before being referenced.
fn handle_capture_failure(
    artifact_dir: &Path,
    manifest: &RunManifest,
    manifest_path: &Path,
    err: &str,
) -> ExitCode {
    write_stderr(&format!(
        "scenario failed: {err}
"
    ));
    let mut updated = manifest.clone();
    match redact_artifact_dir(artifact_dir, manifest) {
        Ok(_) => {
            discover_and_register_artifacts_impl(&mut updated, artifact_dir);
        }
        Err(redact_err) => {
            write_stderr(&format!(
                "error: hard-failure artifact redaction failed (fail-closed): {redact_err}
"
            ));
            updated.add_discrepancy(format!(
                "artifact redaction failed (fail-closed): artifacts NOT registered to prevent publishing unredacted data: {redact_err}"
            ));
        }
    }
    updated.set_outcome(RunOutcome::Failed);
    if let Err(save_err) = save_manifest(&updated, manifest_path) {
        write_stderr(&format!(
            "warning: failed to save manifest after failure: {save_err}
"
        ));
    }
    ExitCode::from(1)
}

/// Run the GitHub capture scenario flow, launching Jefe from the
/// fixture-clone directory (after state seeding).
///
/// This is the same flow as [`run_capture_scenario`] but builds the tmux
/// request with the fixture-clone as the working directory so the isolated
/// Jefe sees the seeded repo/agent immediately.
pub fn run_capture_github_scenario(
    manifest_path: &Path,
    scenario_path: &Path,
    jefe_bin: &Path,
    keep_session: bool,
) -> ExitCode {
    let manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!(
                "failed to load manifest: {err}
"
            ));
            return ExitCode::from(1);
        }
    };
    let scenario = match load_scenario(scenario_path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let Some(manifest_dir) = manifest_path.parent() else {
        write_stderr(
            "manifest path has no parent directory
",
        );
        return ExitCode::from(1);
    };
    let artifact_dir = manifest_dir.join("artifacts");
    let shim_dir = manifest_dir.join("shims");
    let curated_path = detection_path_for(&shim_dir, manifest.runtime_profile);
    let request = match build_github_tmux_request(
        &manifest,
        jefe_bin,
        &curated_path,
        keep_session,
        manifest_dir,
    ) {
        Ok(req) => req,
        Err(err) => {
            write_stderr(&format!(
                "invalid tmux request: {err}
"
            ));
            return ExitCode::from(1);
        }
    };
    let mut manifest = manifest;
    enrich_manifest_with_capture_metadata(&mut manifest, jefe_bin, scenario_path);
    // Finding #3: manifest save before capture is fatal — propagate the error.
    if let Err(err) = save_manifest(&manifest, manifest_path) {
        write_stderr(&format!(
            "fatal: failed to save manifest before capture: {err}\n"
        ));
        return ExitCode::from(1);
    }
    execute_capture_scenario(&scenario, &request, &artifact_dir, &manifest, manifest_path)
}

/// Execute a tmux scenario and handle success/failure.
fn execute_capture_scenario(
    scenario: &jefe::harness::Scenario,
    request: &jefe::harness::TmuxStartRequest,
    artifact_dir: &Path,
    manifest: &RunManifest,
    manifest_path: &Path,
) -> ExitCode {
    match run_tmux_scenario(scenario, request, Some(artifact_dir)) {
        Ok(summary) => {
            crate::cli::write_stdout(&format!(
                "ok: {} steps
",
                summary.steps_run
            ));
            finalize_capture_success(
                manifest,
                manifest_path,
                artifact_dir,
                &summary.captures,
                &summary.soft_failures,
                Some(scenario),
            )
        }
        Err(err) => handle_capture_failure(artifact_dir, manifest, manifest_path, &err.to_string()),
    }
}

/// Enrich the manifest with capture-time metadata: actual binary SHA-256,
/// scenario SHA-256, scenario name, and resolved theme. This is called at
/// capture time (before the scenario runs) so the manifest records the
/// exact binary and scenario under test.
///
/// Always overwrites metadata for the current invocation so re-runs with
/// a different binary or scenario are accurately reflected.
///
/// **Finding**: If scenario hash computation fails (e.g., the scenario file
/// cannot be read), the stale hash from a previous run is cleared and a
/// discrepancy is recorded. This prevents a stale hash from being trusted
/// as the current scenario's identity.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007 (task #4)
pub fn enrich_manifest_with_capture_metadata(
    manifest: &mut RunManifest,
    jefe_bin: &Path,
    scenario_path: &Path,
) {
    // Always overwrite binary hash for the current invocation.
    manifest.binary_hash = compute_binary_hash(jefe_bin);
    // Always overwrite scenario hash for the current invocation. If the hash
    // computation fails, clear any stale hash and record a discrepancy so
    // the manifest never trusts a stale hash from a prior run.
    match compute_scenario_hash(scenario_path) {
        Ok(hash) => {
            manifest.scenario_hash = Some(hash);
        }
        Err(err) => {
            // Fail closed: clear stale hash and record the discrepancy.
            if manifest.scenario_hash.is_some() {
                manifest.add_discrepancy(format!(
                    "cleared stale scenario hash (recompute failed: {err})"
                ));
            }
            manifest.scenario_hash = None;
        }
    }
    // Always overwrite scenario name from the file stem.
    if let Some(stem) = scenario_path.file_stem().and_then(|s| s.to_str()) {
        manifest.scenario_name = stem.to_string();
    }
    // Finding #5: Always set theme: use manifest theme or default to green-screen.
    if manifest.theme.is_none() {
        manifest.theme = Some("green-screen".to_string());
    }
}

/// Run the shared capture scenario flow: load manifest, parse scenario, build
/// tmux request, run the scenario, redact artifacts, and finalize the manifest.
pub fn run_capture_scenario(
    manifest_path: &Path,
    scenario_path: &Path,
    jefe_bin: &Path,
    keep_session: bool,
) -> ExitCode {
    let manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let scenario = match load_scenario(scenario_path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let Some(manifest_dir) = manifest_path.parent() else {
        write_stderr("manifest path has no parent directory\n");
        return ExitCode::from(1);
    };
    let artifact_dir = manifest_dir.join("artifacts");
    let shim_dir = manifest_dir.join("shims");
    let curated_path = detection_path_for(&shim_dir, manifest.runtime_profile);
    let request = match build_tmux_request(&manifest, jefe_bin, &curated_path, keep_session) {
        Ok(req) => req,
        Err(err) => {
            write_stderr(&format!("invalid tmux request: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let mut manifest = manifest;
    enrich_manifest_with_capture_metadata(&mut manifest, jefe_bin, scenario_path);
    // Finding #3: manifest save before capture is fatal — propagate the error.
    if let Err(err) = save_manifest(&manifest, manifest_path) {
        write_stderr(&format!(
            "fatal: failed to save manifest before capture: {err}\n"
        ));
        return ExitCode::from(1);
    }
    execute_capture_scenario(&scenario, &request, &artifact_dir, &manifest, manifest_path)
}

/// Run the validate-runtime scenario flow: load manifest, generate and run
/// the validate-runtime scenario via tmux with the curated detection PATH,
/// redact artifacts, and record semantic evidence in the manifest.
///
/// **Finding #2**: This launches the real Jefe TUI with a curated PATH
/// containing only the selected real runtime. The scenario asserts on the
/// actual runtime binary name in the chooser, proving Jefe detection.
/// Uses `jefe_bin` and `keep_session`.
pub fn run_validate_runtime_scenario(
    manifest_path: &Path,
    scenario_path: &Path,
    jefe_bin: &Path,
    keep_session: bool,
) -> ExitCode {
    let manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!(
                "failed to load manifest: {err}
"
            ));
            return ExitCode::from(1);
        }
    };
    let scenario = match load_scenario(scenario_path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let Some(manifest_dir) = manifest_path.parent() else {
        write_stderr(
            "manifest path has no parent directory
",
        );
        return ExitCode::from(1);
    };
    let artifact_dir = manifest_dir.join("artifacts");
    let shim_dir = manifest_dir.join("shims");
    let curated_path = detection_path_for(&shim_dir, manifest.runtime_profile);
    let request = match build_tmux_request(&manifest, jefe_bin, &curated_path, keep_session) {
        Ok(req) => req,
        Err(err) => {
            write_stderr(&format!(
                "invalid tmux request: {err}
"
            ));
            return ExitCode::from(1);
        }
    };
    let mut manifest = manifest;
    enrich_manifest_with_capture_metadata(&mut manifest, jefe_bin, scenario_path);
    // Finding #3: manifest save before validate-runtime is fatal — propagate.
    if let Err(err) = save_manifest(&manifest, manifest_path) {
        write_stderr(&format!(
            "fatal: failed to save manifest before validate-runtime: {err}\n"
        ));
        return ExitCode::from(1);
    }
    let exit =
        execute_capture_scenario(&scenario, &request, &artifact_dir, &manifest, manifest_path);
    // Finding #3: record validation evidence only on success + chooser artifact.
    // Finding #3: manifest save for evidence is fatal — exit nonzero on failure.
    if exit == ExitCode::SUCCESS && !record_validate_runtime_evidence(manifest_path) {
        return ExitCode::from(1);
    }
    exit
}

/// Finding #3: Add validated action only after ExitCode success AND the
/// expected chooser artifact is registered. Failure leaves no success evidence.
/// Returns `true` if evidence was recorded and persisted successfully.
fn record_validate_runtime_evidence(manifest_path: &Path) -> bool {
    let Ok(mut updated) = load_manifest(manifest_path) else {
        write_stderr("warning: failed to reload manifest for validation evidence\n");
        return false;
    };
    let has_chooser_artifact = updated
        .artifacts
        .iter()
        .any(|a| a.label.contains("validate-runtime-chooser"));
    if !has_chooser_artifact {
        return false;
    }
    let Some(name) = jefe::tutorial_capture::runtime_label(updated.runtime_profile) else {
        return false;
    };
    updated.add_observed_action(
        "validate-runtime",
        format!("validated runtime detection: Jefe chooser showed '{name}'"),
        Some("validate-runtime-chooser".to_string()),
    );
    // Finding #3: manifest save for validation evidence is fatal — return
    // the failure so the caller can exit nonzero.
    if let Err(err) = save_manifest(&updated, manifest_path) {
        write_stderr(&format!(
            "fatal: failed to save validation evidence to manifest: {err}\n"
        ));
        return false;
    }
    true
}

/// Redact artifacts, including fixture repo names if present.
/// Returns `Ok(())` on success or `Err` on failure.
fn redact_artifact_dir(artifact_dir: &Path, manifest: &RunManifest) -> Result<usize, String> {
    let repos: Vec<&str> = manifest
        .fixture_github_repo
        .as_deref()
        .into_iter()
        .collect();
    let result = if repos.is_empty() {
        redact_artifacts(artifact_dir)
    } else {
        redact_artifacts_with_repos(artifact_dir, &repos)
    };
    result.map_err(|e| e.to_string())
}

/// Finalize a successful capture: fail-closed redaction and manifest update.
///
/// **Finding #6**: Redaction fails closed on both success and failure paths.
/// If redaction fails, the manifest outcome is set to Failed and the function
/// returns an error exit code, preventing publication of unredacted artifacts.
fn finalize_capture_success(
    manifest: &RunManifest,
    manifest_path: &Path,
    artifact_dir: &Path,
    captures: &[String],
    soft_failures: &[jefe::harness::RunnerFailure],
    scenario: Option<&jefe::harness::Scenario>,
) -> ExitCode {
    if let Err(err) = redact_artifact_dir(artifact_dir, manifest) {
        write_stderr(&format!(
            "error: artifact redaction failed (fail-closed): {err}\n"
        ));
        update_manifest_outcome(manifest, manifest_path, RunOutcome::Failed);
        return ExitCode::from(1);
    }
    finalize_manifest_with_scenario(manifest, manifest_path, captures, soft_failures, scenario);
    ExitCode::SUCCESS
}
