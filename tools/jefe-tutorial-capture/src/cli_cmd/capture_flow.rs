//! Scenario execution flow: launch tmux scenarios, handle success/failure,
//! and finalize manifests.
//!
//! Extracted from `tmux_helpers.rs` to keep file sizes under the project
//! limit. The hook types, request builders, and manifest finalization helpers
//! remain in `tmux_helpers`.

use std::path::Path;
use std::process::ExitCode;

use super::cli::write_stderr;
use super::tmux_helpers::{
    self, build_github_tmux_request, build_tmux_request, enrich_manifest_with_capture_metadata,
    finalize_manifest_with_scenario, load_scenario, update_manifest_outcome,
};

use jefe::harness::{CaptureHook, RunOptions, run_tmux_scenario_with_hook};
use jefe_tutorial_capture::{
    OwnedPathKind, RunManifest, RunOutcome, detection_path_for, load_manifest, save_manifest,
};

/// Resolve the [`StatusSuppressHook`] for a capture run from the manifest's
/// owned config directory. The hook discovers the run-owned agent IDs at
/// capture time through the persistence-owned query API, targeting only the
/// nested agent session(s) on Jefe's private tmux socket. Pre-agent captures
/// (no agent in state.json) are a no-op.
fn build_status_suppress_hook(manifest: &RunManifest) -> Option<tmux_helpers::StatusSuppressHook> {
    let config_dir = manifest.find_path_by_kind(OwnedPathKind::ConfigDir)?;
    let run_root = config_dir.parent()?;
    Some(tmux_helpers::StatusSuppressHook::new(
        tmux_helpers::nested_jefe_socket_path(run_root),
        config_dir.to_path_buf(),
    ))
}

/// A no-op hook used when the manifest has no owned config directory to
/// discover agent IDs from.
#[derive(Debug, Default)]
struct NoSuppressHook;

impl CaptureHook for NoSuppressHook {
    fn before_capture(&mut self, _label: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Execute a tmux scenario and handle success/failure.
fn execute_capture_scenario(
    scenario: &jefe::harness::Scenario,
    request: &jefe::harness::TmuxStartRequest,
    artifact_dir: &Path,
    manifest: &RunManifest,
    manifest_path: &Path,
) -> ExitCode {
    // build_status_suppress_hook returns None when the manifest has no
    // owned config directory — in that case there is no isolated config to
    // discover agent IDs from, so we run with no hook.
    if let Some(mut hook) = build_status_suppress_hook(manifest) {
        run_with_status_suppress_hook(
            scenario,
            request,
            artifact_dir,
            manifest,
            manifest_path,
            &mut hook,
        )
    } else {
        let mut hook = NoSuppressHook;
        run_with_status_suppress_hook(
            scenario,
            request,
            artifact_dir,
            manifest,
            manifest_path,
            &mut hook,
        )
    }
}

/// Run a scenario with a status-suppress hook and finalize the result.
fn run_with_status_suppress_hook(
    scenario: &jefe::harness::Scenario,
    request: &jefe::harness::TmuxStartRequest,
    artifact_dir: &Path,
    manifest: &RunManifest,
    manifest_path: &Path,
    hook: &mut dyn CaptureHook,
) -> ExitCode {
    match run_tmux_scenario_with_hook(
        scenario,
        request,
        Some(artifact_dir),
        &RunOptions::with_ansi(),
        hook,
    ) {
        Ok(summary) => {
            super::cli::write_stdout(&format!(
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
            tmux_helpers::discover_and_register_artifacts(&mut updated, artifact_dir);
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

/// Redact artifacts, including fixture repo names if present.
/// Returns `Ok(())` on success or `Err` on failure.
fn redact_artifact_dir(artifact_dir: &Path, manifest: &RunManifest) -> Result<usize, String> {
    let repos: Vec<&str> = manifest
        .fixture_github_repo
        .as_deref()
        .into_iter()
        .collect();
    let result = if repos.is_empty() {
        jefe_tutorial_capture::redact_artifacts(artifact_dir)
    } else {
        jefe_tutorial_capture::redact_artifacts_with_repos(artifact_dir, &repos)
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
        if let Err(save_err) = update_manifest_outcome(manifest, manifest_path, RunOutcome::Failed)
        {
            write_stderr(&format!(
                "error: {save_err}
"
            ));
        }
        return ExitCode::from(1);
    }
    if let Err(err) =
        finalize_manifest_with_scenario(manifest, manifest_path, captures, soft_failures, scenario)
    {
        write_stderr(&format!(
            "error: {err}
"
        ));
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
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
    let Some(name) = jefe_tutorial_capture::runtime_label(updated.runtime_profile) else {
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
