//! Subcommand handlers: prepare, capture, render, report, cleanup, validate-runtime.
//!
//! The plan-github subcommand is in [`super::plan_github`].

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::cli::{
    CaptureGithubOpts, CaptureLocalOpts, GitHubCaptureMode, PrepareOpts, RenderOpts,
    ValidateRuntimeOpts, parse_runtime_profile, write_stderr, write_stdout,
};

use jefe_tutorial_capture::{
    OwnedPathKind, RunId, RunManifest, RunSetup, is_valid_repo_format, load_manifest, prepare_run,
    save_manifest, save_report, verify_sentinel_ownership,
};

use super::capture_flow::{
    run_capture_github_scenario, run_capture_scenario, run_validate_runtime_scenario,
};
use super::tmux_helpers::render_single_artifact;

pub use super::plan_github::run_plan_github;

/// Register an artifact entry in the manifest with path validation.
/// Returns an error string if the relative path is unsafe.
/// **Finding #8**: Artifact registration validates the path before adding.
fn save_artifact_entry(
    manifest: &mut RunManifest,
    label: impl Into<String>,
    relative_path: impl Into<PathBuf>,
    kind: jefe_tutorial_capture::ArtifactKind,
) -> Result<(), String> {
    let relative_path = relative_path.into();
    let label = label.into();
    jefe_tutorial_capture::validate_artifact_path(&relative_path)
        .map_err(|e| format!("unsafe artifact path '{}': {e}", relative_path.display()))?;
    manifest
        .add_artifact(jefe_tutorial_capture::ArtifactEntry::new(
            label,
            relative_path,
            kind,
        ))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Run the `prepare` subcommand: create isolated fixtures and manifest.
pub fn run_prepare(opts: PrepareOpts) -> ExitCode {
    let Some(run_id) = RunId::new(&opts.run_id) else {
        write_stderr(&format!(
            "invalid run ID '{}': must be 1-64 alphanumeric or hyphen characters\n",
            opts.run_id
        ));
        return ExitCode::from(1);
    };
    let runtime_profile = match parse_runtime_profile(&opts.runtime_profile) {
        Ok(profile) => profile,
        Err(err) => {
            write_stderr(&format!("{err}\n"));
            return ExitCode::from(2);
        }
    };
    // Validate that real runtime executables exist before proceeding.
    if let Err(err) = jefe_tutorial_capture::validate_real_runtime(runtime_profile) {
        write_stderr(&format!("runtime profile error: {err}\n"));
        return ExitCode::from(1);
    }
    let setup = RunSetup {
        run_id: run_id.clone(),
        base_dir: opts.base_dir.clone(),
        jefe_version: jefe::VERSION.to_string(),
        scenario_name: opts.scenario_name.clone(),
        cols: 100,
        rows: 32,
        runtime_profile,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: Some("green-screen".to_string()),
        scenario_hash: None,
        shim_availability: opts.shim_availability,
    };
    let (dirs, manifest) = match prepare_run(&setup) {
        Ok(result) => result,
        Err(err) => {
            write_stderr(&format!("prepare failed: {err}\n"));
            return ExitCode::from(1);
        }
    };
    write_stdout(&format!(
        "prepared run '{}' at {}\n",
        run_id.as_str(),
        dirs.root.display()
    ));
    write_stdout(&format!("  config dir: {}\n", dirs.config_dir.display()));
    write_stdout(&format!(
        "  artifact dir: {}\n",
        dirs.artifact_dir.display()
    ));
    write_stdout(&format!(
        "  fixture repo: {}\n",
        dirs.fixture_repo.display()
    ));
    let manifest_path = dirs.manifest_path();
    if let Err(err) = save_manifest(&manifest, &manifest_path) {
        write_stderr(&format!("failed to save manifest: {err}\n"));
        return ExitCode::from(1);
    }
    write_stdout(&format!("  manifest: {}\n", manifest_path.display()));
    ExitCode::SUCCESS
}

/// Run the `capture-local` subcommand: drive the real Jefe TUI via tmux.
///
/// **Finding #6**: The local capture scenario is shim-specific — it expects
/// deterministic shim output (`runtime-shim: ready`). When a real runtime
/// profile is active, `capture-local` MUST REFUSE rather than running a
/// scenario that won't match the expected checkpoints. Real-runtime
/// validation requires the `validate-runtime` subcommand.
pub fn run_capture_local(opts: CaptureLocalOpts) -> ExitCode {
    let manifest = match load_manifest(&opts.manifest_path) {
        Ok(manifest) => manifest,
        Err(err) => {
            write_stderr(&format!(
                "failed to load manifest: {err}
"
            ));
            return ExitCode::from(1);
        }
    };
    // Refuse real-runtime profiles — the local capture scenario is shim-specific.
    if !manifest.runtime_profile.is_shim() {
        write_stderr(&format!(
            "error: capture-local is shim-specific but manifest runtime profile \
             is '{profile}'. The scenario expects deterministic shim output. \
             For real-runtime validation, use the 'validate-runtime' subcommand.\n",
            profile = runtime_profile_name(manifest.runtime_profile)
        ));
        return ExitCode::from(1);
    }
    run_capture_scenario(
        &opts.manifest_path,
        &opts.scenario_path,
        &opts.jefe_bin,
        opts.keep_session,
    )
}

/// Human-readable name for a runtime profile (for CLI output).
fn runtime_profile_name(profile: jefe_tutorial_capture::RuntimeProfile) -> &'static str {
    match profile {
        jefe_tutorial_capture::RuntimeProfile::Shim => "shim",
        jefe_tutorial_capture::RuntimeProfile::RealLlxprt => "real-llxprt",
        jefe_tutorial_capture::RuntimeProfile::RealCodePuppy => "real-code-puppy",
    }
}

/// Run the `capture-github` subcommand: drive Jefe's Issues/PR modes against
/// a fixture repository. Requires a prepared manifest and the real jefe binary.
///
/// **Finding #2**: capture-github fails closed unless the manifest has an exact
/// valid current-run issue, branch, and PR — all from the same fixture repo.
/// The generic static fallback scenario is removed. If the manifest does not
/// pass `validate_tier_b_resources`, capture is refused with a nonzero exit.
///
/// **Finding #3**: Production capture-github generates the scenario from the
/// manifest's exact issue/PR resources (exact titles, numbers, branch names).
/// The generated scenario asserts on exact identity/filter before any action.
///
/// **Finding #4**: The generated scenario is written under
/// `artifacts/scenarios/`, registered as an artifact in the manifest, and
/// saved atomically.
///
/// Before launching Jefe, this verifies the fixture clone exists (created by
/// `plan-github --confirm-disposable`), seeds the isolated Jefe config with
/// the repository/agent state pointing at the clone, and launches Jefe from
/// the fixture-clone directory.
/// Generate the Tier-B scenario from the manifest's exact resources, write it
/// atomically under `artifacts/scenarios/`, register it as a `Scenario`
/// artifact, and save the manifest.
///
/// **Finding #4**: Atomic write + artifact registration + fatal manifest save.
/// Returns the generated scenario path on success, or an `ExitCode` on failure.
fn write_generated_scenario(
    manifest: &mut RunManifest,
    manifest_dir: &Path,
    manifest_path: &Path,
    is_merge: bool,
) -> Result<PathBuf, ExitCode> {
    let params = jefe_tutorial_capture::extract_scenario_params(manifest, "TutorialAgent")
        .ok_or_else(|| {
            write_stderr("error: could not extract scenario parameters from manifest resources.\n");
            ExitCode::from(1)
        })?;
    let generated_json = if is_merge {
        jefe_tutorial_capture::generate_tier_b_merge_scenario(&params)
    } else {
        jefe_tutorial_capture::generate_tier_b_scenario(&params)
    };
    let artifact_dir = manifest_dir.join("artifacts");
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        write_stderr(&format!(
            "fatal: failed to create artifacts directory: {err}\n"
        ));
        ExitCode::from(1)
    })?;
    let scenario_filename = if is_merge {
        "generated-github-merge-scenario.json"
    } else {
        "generated-github-scenario.json"
    };
    let scenario_rel = std::path::Path::new("scenarios").join(scenario_filename);
    let scenario_label = if is_merge {
        "generated-github-merge-scenario"
    } else {
        "generated-github-scenario"
    };
    jefe_tutorial_capture::write_artifact_atomic(
        &artifact_dir,
        &scenario_rel,
        &generated_json,
        manifest,
        scenario_label,
        jefe_tutorial_capture::ArtifactKind::Scenario,
    )
    .map_err(|err| {
        write_stderr(&format!(
            "fatal: failed to write generated scenario atomically: {err}\n"
        ));
        ExitCode::from(1)
    })?;
    save_manifest(manifest, manifest_path).map_err(|err| {
        write_stderr(&format!(
            "fatal: failed to update manifest with scenario artifact: {err}\n"
        ));
        ExitCode::from(1)
    })?;
    let generated_path = artifact_dir.join(&scenario_rel);
    write_stdout(&format!(
        "generated scenario from manifest exact resources: {}\n",
        generated_path.display()
    ));
    Ok(generated_path)
}

pub fn run_capture_github(opts: CaptureGithubOpts) -> ExitCode {
    write_stdout("capturing GitHub scenario\n");
    let mut manifest = match load_manifest(&opts.manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let Some(manifest_dir) = opts.manifest_path.parent() else {
        write_stderr("manifest path has no parent directory\n");
        return ExitCode::from(1);
    };
    if let Err(err) = validate_capture_manifest_provenance(&manifest, manifest_dir) {
        write_stderr(&format!(
            "error: capture-github safety validation failed: {err}\n"
        ));
        return ExitCode::from(1);
    }
    // Validate exact current-run resources before any config seeding or artifact
    // mutation so an invalid manifest remains safely retryable.
    if let Err(err) = jefe_tutorial_capture::validate_tier_b_resources(&manifest) {
        write_stderr(&format!(
            "error: capture-github requires exact valid current-run issue, branch, and PR \
             in the fixture repo. Validation failed: {err}\n"
        ));
        return ExitCode::from(1);
    }
    if let Err(code) = validate_capture_github_prerequisites(&manifest, &opts, manifest_dir) {
        return code;
    }
    // Finding #4: Generate the scenario, write it atomically under
    // artifacts/scenarios/, and register it as a Scenario artifact.
    // The capture mode is an explicit typed flag — no filename inference.
    let is_merge = opts.mode == GitHubCaptureMode::Merge;
    let generated_path = match write_generated_scenario(
        &mut manifest,
        manifest_dir,
        &opts.manifest_path,
        is_merge,
    ) {
        Ok(path) => path,
        Err(code) => return code,
    };
    run_capture_github_scenario(
        &opts.manifest_path,
        &generated_path,
        &opts.jefe_bin,
        opts.keep_session,
    )
}

fn validate_capture_manifest_provenance(
    manifest: &RunManifest,
    run_root: &Path,
) -> Result<(), String> {
    verify_sentinel_ownership(run_root, manifest.run_id.as_str()).map_err(|err| err.to_string())?;
    let repo = manifest
        .fixture_github_repo
        .as_deref()
        .ok_or_else(|| "missing fixture GitHub repository".to_string())?;
    if !is_valid_repo_format(repo) {
        return Err(format!("invalid GitHub repository identity '{repo}'"));
    }
    if ["vybestack/jefe", "vybestack/llxprt-jefe"]
        .iter()
        .any(|production| production.eq_ignore_ascii_case(repo))
    {
        return Err(format!("production repository '{repo}' is always refused"));
    }
    if manifest.creation_allowlist.is_empty() || !manifest.was_creation_allowed(repo) {
        return Err(format!(
            "repository '{repo}' lacks creation-time allowlist provenance"
        ));
    }
    Ok(())
}

/// Validate prerequisites for `capture-github`: fixture clone exists,
/// fixture GitHub repo is set, merge authorization is present for merge mode.
/// Seeds the Tier B state on success.
fn validate_owned_fixture_clone<'a>(
    manifest: &'a RunManifest,
    manifest_dir: &Path,
) -> Result<&'a Path, ExitCode> {
    let expected = manifest_dir.join("fixture-clone");
    let Some(path) = manifest.find_path_by_kind(OwnedPathKind::FixtureClone) else {
        write_stderr("error: manifest does not own a fixture clone\n");
        return Err(ExitCode::from(1));
    };
    if path != expected || !path.is_dir() {
        write_stderr(&format!(
            "error: fixture clone is not the expected owned directory '{}'. Run 'plan-github --confirm-disposable' first.\n",
            expected.display()
        ));
        return Err(ExitCode::from(1));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_symlink() => Ok(path),
        Ok(_) => {
            write_stderr("error: fixture clone must not be a symlink\n");
            Err(ExitCode::from(1))
        }
        Err(err) => {
            write_stderr(&format!("error: cannot inspect fixture clone: {err}\n"));
            Err(ExitCode::from(1))
        }
    }
}

fn validate_capture_github_prerequisites(
    manifest: &RunManifest,
    opts: &CaptureGithubOpts,
    manifest_dir: &Path,
) -> Result<(), ExitCode> {
    let fixture_clone = validate_owned_fixture_clone(manifest, manifest_dir)?;
    let Some(github_repo) = manifest.fixture_github_repo.clone() else {
        write_stderr(
            "error: manifest is missing fixture GitHub repository identity. \
             Run 'plan-github --fixture-repo <owner/repo>' to set it.\n",
        );
        return Err(ExitCode::from(1));
    };
    let theme = manifest
        .theme
        .clone()
        .unwrap_or_else(|| "green-screen".to_string());
    let config_dir = if let Some(d) = manifest.find_path_by_kind(OwnedPathKind::ConfigDir) {
        d.to_path_buf()
    } else {
        write_stderr("manifest missing config directory\n");
        return Err(ExitCode::from(1));
    };
    // Typed explicit merge mode — no filename inference.
    if opts.mode == GitHubCaptureMode::Merge && !manifest.merge_authorized {
        write_stderr(
            "error: merge mode requires merge authorization. \
             Run 'plan-github --allow-merge' to authorize merge.\n",
        );
        return Err(ExitCode::from(1));
    }
    let agent_kind = jefe_tutorial_capture::derive_agent_kind(
        manifest.runtime_profile,
        manifest.shim_availability,
    );
    let seed = jefe_tutorial_capture::TierBStateSeed {
        config_dir,
        fixture_clone_path: fixture_clone.to_path_buf(),
        fixture_github_repo: github_repo,
        theme,
        agent_name: "TutorialAgent".to_string(),
        agent_kind,
    };
    if let Err(err) = jefe_tutorial_capture::seed_tier_b_state(&seed) {
        write_stderr(&format!("failed to seed Tier B state: {err}\n"));
        return Err(ExitCode::from(1));
    }
    Ok(())
}

/// Run the `validate-runtime` subcommand: validate a real-runtime profile
/// against the real Jefe TUI by launching Jefe via tmux with a curated PATH
/// containing only the selected real runtime, then driving a minimal scenario
/// that opens New Repository to trigger the runtime chooser and asserts on the
/// actual runtime binary name text.
///
/// **Finding #2**: Real-runtime profiles must be validated by actually
/// launching Jefe and asserting on the runtime chooser identity, not just
/// checking executable availability. This subcommand generates the
/// validate-runtime scenario from the manifest's runtime profile, launches
/// Jefe with `jefe_bin` and `keep_session`, and records semantic evidence.
///
/// **Finding #7**: The scenario asserts on the actual runtime binary name
/// (`llxprt` or `code-puppy`), not just a generic title.
///
/// It requires the manifest to have a real-runtime profile (RealLlxprt or
/// RealCodePuppy), not Shim. It may avoid starting an agent but must prove
/// Jefe detection.
pub fn run_validate_runtime(opts: ValidateRuntimeOpts) -> ExitCode {
    let mut manifest = match load_manifest(&opts.manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };
    if manifest.runtime_profile.is_shim() {
        write_stderr(
            "error: validate-runtime requires a real-runtime profile \
             (real-llxprt or real-code-puppy), not shim. \
             Use 'capture-local' for shim-based capture.\n",
        );
        return ExitCode::from(1);
    }
    // Validate that the real runtime exists before proceeding.
    if let Err(err) = jefe_tutorial_capture::validate_real_runtime(manifest.runtime_profile) {
        write_stderr(&format!("runtime profile error: {err}\n"));
        return ExitCode::from(1);
    }
    let runtime_name =
        jefe_tutorial_capture::runtime_binary_name(manifest.runtime_profile).unwrap_or("unknown");
    write_stdout(&format!(
        "validating runtime profile '{}' (expecting '{}' in chooser)\n",
        runtime_profile_name(manifest.runtime_profile),
        runtime_name
    ));
    // Finding #1: Prepare the validate-runtime scenario atomically under
    // artifacts/scenarios/ and register it in the manifest as
    // ArtifactKind::Scenario. The manifest is persisted BEFORE tmux launch
    // so the scenario artifact is tracked even if the run fails.
    let manifest_dir = opts
        .manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let run_root = manifest_dir;
    let artifact_dir = manifest_dir.join("artifacts");
    let scenario_path = match jefe_tutorial_capture::prepare_validate_runtime_scenario(
        &mut manifest,
        &artifact_dir,
        run_root,
    ) {
        Ok(path) => path,
        Err(err) => {
            write_stderr(&format!(
                "fatal: failed to persist validate-runtime scenario artifact: {err}\n"
            ));
            return ExitCode::from(1);
        }
    };
    write_stdout(&format!(
        "validate-runtime scenario written to {}\n",
        scenario_path.display()
    ));
    // Launch Jefe via tmux with the curated detection PATH.
    run_validate_runtime_scenario(
        &opts.manifest_path,
        &scenario_path,
        &opts.jefe_bin,
        opts.keep_session,
    )
}

/// Run the `render` subcommand: convert screen captures to SVG.
///
/// **Finding #7**: Registers SVG outputs in the manifest as artifacts and
/// saves the updated manifest atomically.
pub fn run_render(opts: &RenderOpts) -> ExitCode {
    let mut manifest = match load_manifest(&opts.manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let manifest_dir = opts
        .manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let svg_dir = manifest_dir.join("artifacts").join("svg");
    if let Err(err) = fs::create_dir_all(&svg_dir) {
        write_stderr(&format!("failed to create svg dir: {err}\n"));
        return ExitCode::from(1);
    }
    let mut rendered = 0usize;
    for artifact in &manifest.artifacts.clone() {
        match render_single_artifact(artifact, manifest_dir, &manifest, &svg_dir) {
            Ok(svgs) => {
                if let Some(mono_svg) = svgs.mono_svg {
                    rendered += 1;
                    if let Err(err) = save_artifact_entry(
                        &mut manifest,
                        format!("{}-svg", artifact.label),
                        mono_svg,
                        jefe_tutorial_capture::ArtifactKind::Visual,
                    ) {
                        write_stderr(&format!("fatal: failed to register SVG artifact: {err}\n"));
                        return ExitCode::from(1);
                    }
                }
                if let Some(color_svg) = svgs.color_svg
                    && let Err(err) = save_artifact_entry(
                        &mut manifest,
                        format!("{}-color-svg", artifact.label),
                        color_svg,
                        jefe_tutorial_capture::ArtifactKind::ColorSvg,
                    )
                {
                    write_stderr(&format!(
                        "fatal: failed to register color SVG artifact: {err}\n"
                    ));
                    return ExitCode::from(1);
                }
            }
            Err(_path) => return ExitCode::from(1),
        }
    }
    if let Err(err) = save_manifest(&manifest, &opts.manifest_path) {
        write_stderr(&format!(
            "fatal: failed to update manifest with SVG artifacts: {err}\n"
        ));
        return ExitCode::from(1);
    }
    write_stdout(&format!(
        "rendered {rendered} SVG file(s) to {}\n",
        svg_dir.display()
    ));
    ExitCode::SUCCESS
}

/// Run the `report` subcommand: generate a Markdown evidence report.
///
/// **Finding #7**: Registers the report as an artifact in the manifest.
pub fn run_report(manifest_path: &Path) -> ExitCode {
    let mut manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let manifest_dir = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let report_path = manifest_dir.join("artifacts").join("run-report.md");
    if let Err(err) = save_report(&manifest, &report_path) {
        write_stderr(&format!("failed to save report: {err}\n"));
        return ExitCode::from(1);
    }
    // Finding #8: register the report as an artifact in the manifest.
    // Registration failure is fatal.
    // Task #5: path is relative to ArtifactDir (no "artifacts/" prefix).
    let report_relative = PathBuf::from("run-report.md");
    if let Err(err) = save_artifact_entry(
        &mut manifest,
        "run-report",
        &report_relative,
        jefe_tutorial_capture::ArtifactKind::Report,
    ) {
        write_stderr(&format!(
            "fatal: failed to register report artifact: {err}\n"
        ));
        return ExitCode::from(1);
    }
    // Finding #8: save manifest with registered report artifact (fatal on failure).
    if let Err(err) = save_manifest(&manifest, manifest_path) {
        write_stderr(&format!(
            "fatal: failed to update manifest with report artifact: {err}\n"
        ));
        return ExitCode::from(1);
    }
    write_stdout(&format!("report saved to {}\n", report_path.display()));
    ExitCode::SUCCESS
}
