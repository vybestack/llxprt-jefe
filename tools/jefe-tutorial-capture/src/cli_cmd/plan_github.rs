//! Plan-github subcommand handler: plan or execute opt-in GitHub fixture
//! mutations with allowlist safety checks.
//!
//! Extracted from `commands.rs` to keep file sizes under the project limit.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::cli::{PlanGithubOpts, write_stderr, write_stdout};

use jefe_tutorial_capture::{
    OrchestrationError, OwnedPathKind, RealCommandRunner, RunId, RunManifest, RunOutcome,
    TierBContext, TierBPlan, build_allowlist_from_sources_checked, check_fixture_repo,
    execute_tier_b, load_manifest, plan_tier_b, save_manifest,
};

/// The canonical environment variable name for the fixture allowlist.
const FIXTURE_ALLOWLIST_ENV: &str = "JEFE_TUTORIAL_FIXTURE_ALLOWLIST";

/// Build the allowlist from independent sources, returning an exit code on error.
fn build_plan_allowlist(
    opts: &PlanGithubOpts,
) -> Result<jefe_tutorial_capture::FixtureAllowlist, ExitCode> {
    let allow_repos: Vec<&str> = opts.allow_repos.iter().map(String::as_str).collect();
    match build_allowlist_from_sources_checked(
        Some(FIXTURE_ALLOWLIST_ENV),
        opts.allowlist_file.as_deref(),
        &allow_repos,
    ) {
        Ok(al) => Ok(al),
        Err(err) => {
            write_stderr(&format!(
                "error: {err}
"
            ));
            Err(ExitCode::from(1))
        }
    }
}

/// Resolve the clone destination from explicit flag or manifest path.
fn resolve_clone_dest(opts: &PlanGithubOpts) -> PathBuf {
    opts.clone_dest.clone().unwrap_or_else(|| {
        if let Some(manifest_path) = &opts.manifest_path {
            let run_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
            run_root.join("fixture-clone")
        } else {
            PathBuf::from(format!("/tmp/jefe-tutorial-{}/fixture-clone", opts.run_id))
        }
    })
}

/// Run the `plan-github` subcommand: plan or execute opt-in GitHub fixture
/// mutations with allowlist safety checks.
///
/// **Finding #1 fix**: The target repository is NEVER self-allowed. The
/// allowlist is built only from independent sources: env var
/// (`JEFE_TUTORIAL_FIXTURE_ALLOWLIST`), `--allowlist-file`, and
/// `--allow-repo` flags. The `--fixture-repo` target is evaluated against
/// this allowlist and refused if not present.
///
/// **Finding #12 fix**: The target repository is printed before any mutation.
///
/// **Finding #2/#4 fix**: When `--manifest` is provided, plan-github operates
/// on an existing prepared manifest/run root and records clone/file path
/// ownership in it. A prepared sentinel manifest is required for Tier B.
///
/// In dry-run mode, only plans are produced. In execution mode, the typed
/// executor runs the full Tier-B sequence.
pub fn run_plan_github(opts: PlanGithubOpts) -> ExitCode {
    // Validate the fixture repo format at the CLI boundary.
    if !jefe_tutorial_capture::is_valid_repo_format(&opts.fixture_repo) {
        write_stderr(&format!(
            "error: invalid fixture repo format '{}': must be 'owner/repo' with valid GitHub characters\n",
            opts.fixture_repo
        ));
        return ExitCode::from(2);
    }
    // Finding #5: Parse plan-github run_id as RunId before any work.
    let Some(run_id) = RunId::new(&opts.run_id) else {
        write_stderr(&format!(
            "error: invalid run ID '{}': must be 1-64 alphanumeric or hyphen characters\n",
            opts.run_id
        ));
        return ExitCode::from(2);
    };
    let allowlist = match build_plan_allowlist(&opts) {
        Ok(al) => al,
        Err(code) => return code,
    };
    match check_fixture_repo(&allowlist, &opts.fixture_repo) {
        Ok(()) => {}
        Err(OrchestrationError::FixtureRefused { repo, reason }) => {
            write_stderr(&format!("REFUSED: {repo} -- {reason}\n"));
            return ExitCode::from(1);
        }
        Err(err) => {
            write_stderr(&format!("error: {err}\n"));
            return ExitCode::from(1);
        }
    }
    // Finding #12: print target repo before any mutation.
    write_stdout(&format!("target fixture repo: {}\n", opts.fixture_repo));

    // Determine the clone destination and manifest path.
    let clone_dest = resolve_clone_dest(&opts);

    let plan = match plan_tier_b(
        &allowlist,
        &opts.fixture_repo,
        run_id.as_str(),
        opts.allow_merge,
        &clone_dest,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            write_stderr(&format!("plan error: {err}\n"));
            return ExitCode::from(1);
        }
    };
    print_plan_summary(&plan, &clone_dest);
    if opts.dry_run {
        return print_dry_run(&plan);
    }
    if !opts.confirm_disposable {
        write_stderr("error: --confirm-disposable is required for non-dry-run execution\n");
        return ExitCode::from(1);
    }
    // Finding #5: For non-dry-run, pass the validated RunId for manifest
    // run ID exact match verification before mutation.
    execute_plan_github(&opts, &plan, &clone_dest, &allowlist, &run_id)
}

/// Print the mutation plan summary.
fn print_plan_summary(plan: &TierBPlan, clone_dest: &Path) {
    write_stdout("Mutation plan:\n");
    write_stdout(&format!(
        "  issue title: {}\n",
        plan.mutation_plan.issue_title
    ));
    write_stdout(&format!(
        "  branch name: {}\n",
        plan.mutation_plan.branch_name
    ));
    write_stdout(&format!("  pr title: {}\n", plan.mutation_plan.pr_title));
    write_stdout(&format!("  clone dest: {}\n", clone_dest.display()));
    write_stdout(&format!("  {} commands planned\n", plan.commands.len()));
}

/// Print the dry-run plan details.
fn print_dry_run(plan: &TierBPlan) -> ExitCode {
    write_stdout("(dry-run: no mutations performed)\n");
    for cmd in &plan.commands {
        write_stdout(&format!("  -> {} ({})\n", cmd.description, cmd.program));
    }
    ExitCode::SUCCESS
}

/// Execute the Tier-B plan via the typed executor with real command runner.
///
/// **Finding #4 fix**: Records clone/file paths ownership in the manifest
/// before mutation and saves immediately. Requires a prepared sentinel
/// manifest for Tier B execution.
///
/// **Task #3 fix**: Persists the normalized effective creation allowlist
/// before the first mutation so cleanup can revalidate against immutable
/// manifest provenance (not env).
fn execute_plan_github(
    opts: &PlanGithubOpts,
    plan: &TierBPlan,
    clone_dest: &Path,
    allowlist: &jefe_tutorial_capture::FixtureAllowlist,
    run_id: &RunId,
) -> ExitCode {
    let manifest_path = resolve_manifest_path(opts, clone_dest);

    let Some(mut manifest) = load_tier_b_manifest(&manifest_path) else {
        return ExitCode::from(1);
    };

    // Finding #5: Non-dry-run manifest run ID must exactly match the CLI
    // run_id before any mutation.
    if manifest.run_id.as_str() != run_id.as_str() {
        write_stderr(&format!(
            "error: manifest run ID '{}' does not match CLI run ID '{}'. Refusing mutation.\n",
            manifest.run_id.as_str(),
            run_id.as_str()
        ));
        return ExitCode::from(1);
    }

    if !manifest.github_resources.is_empty() || clone_dest.exists() {
        write_stderr(
            "error: manifest already contains Tier-B resources; refusing to overwrite cleanup provenance\n",
        );
        return ExitCode::from(1);
    }
    if let Some(recorded_clone) = manifest.find_path_by_kind(OwnedPathKind::FixtureClone)
        && recorded_clone != clone_dest
    {
        write_stderr("error: manifest records a different Tier-B clone destination\n");
        return ExitCode::from(1);
    }

    let run_root = manifest_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    if let Err(code) = pre_execution_checks(clone_dest, run_root) {
        return code;
    }
    record_tier_b_paths(&mut manifest, clone_dest, &opts.fixture_repo);
    manifest.set_creation_allowlist(allowlist.normalized_repos());
    manifest.set_merge_authorized(opts.allow_merge);
    if let Err(err) = save_manifest(&manifest, &manifest_path) {
        write_stderr(&format!(
            "failed to save manifest before execution: {err}\n"
        ));
        return ExitCode::from(1);
    }

    let mut runner = RealCommandRunner;
    let manifest_path_clone = manifest_path.clone();
    let mut save_fn = |m: &RunManifest| match save_manifest(m, &manifest_path_clone) {
        Ok(()) => Ok(()),
        Err(e) => Err(e.to_string()),
    };
    let mut ctx = TierBContext {
        confirm_disposable: opts.confirm_disposable,
        skip_validation: false,
        runner: &mut runner,
        clone_dest,
        run_root,
        save_fn: &mut save_fn,
    };
    let result = execute_tier_b(plan, &mut manifest, &mut ctx);
    finalize_tier_b_execution(result, &mut manifest, &manifest_path)
}

/// Finalize the Tier-B execution: set outcome, save manifest fatally, print
/// output. Returns the appropriate exit code.
///
/// **Finding #3**: All manifest saves are fatal — a save failure produces a
/// nonzero exit regardless of whether the execution itself succeeded.
fn finalize_tier_b_execution(
    result: Result<Vec<String>, jefe_tutorial_capture::TierBError>,
    manifest: &mut RunManifest,
    manifest_path: &Path,
) -> ExitCode {
    match result {
        Ok(outputs) => {
            for output in &outputs {
                write_stdout(&format!("  done: {output}\n"));
            }
            manifest.set_outcome(RunOutcome::Success);
            if let Err(err) = save_manifest(manifest, manifest_path) {
                write_stderr(&format!(
                    "fatal: failed to save manifest after success: {err}\n"
                ));
                return ExitCode::from(1);
            }
            write_stdout("Tier-B execution complete.\n");
            ExitCode::SUCCESS
        }
        Err(err) => {
            write_stderr(&format!("tier-b execution failed: {err}\n"));
            manifest.set_outcome(RunOutcome::Failed);
            if let Err(save_err) = save_manifest(manifest, manifest_path) {
                write_stderr(&format!(
                    "fatal: failed to save manifest after failure: {save_err}\n"
                ));
                return ExitCode::from(1);
            }
            ExitCode::from(1)
        }
    }
}

/// Pre-execution safety checks: Tier B required tools and clone destination
/// validation.
fn pre_execution_checks(clone_dest: &Path, run_root: &Path) -> Result<(), ExitCode> {
    // Finding #1: gh is required only for Tier B (GitHub fixture execution).
    let tier_b_missing = jefe_tutorial_capture::check_tier_b_required_tools(
        &std::env::var("PATH").unwrap_or_default(),
    );
    if !tier_b_missing.is_empty() {
        write_stderr(&format!(
            "error: required Tier B tools not found on PATH: {}\n",
            tier_b_missing.join(", ")
        ));
        return Err(ExitCode::from(1));
    }
    if let Err(err) = jefe_tutorial_capture::validate_clone_destination(clone_dest, run_root) {
        write_stderr(&format!(
            "error: clone destination validation failed: {err}\n"
        ));
        return Err(ExitCode::from(1));
    }
    Ok(())
}

/// Resolve the manifest path: explicit `--manifest` or default to the clone
/// dest's parent directory.
fn resolve_manifest_path(opts: &PlanGithubOpts, clone_dest: &Path) -> PathBuf {
    if let Some(mp) = &opts.manifest_path {
        mp.clone()
    } else {
        clone_dest
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("run-manifest.json")
    }
}

/// Load the Tier B manifest, requiring a prepared sentinel. Returns None
/// (and prints an error) if the manifest cannot be loaded.
fn load_tier_b_manifest(manifest_path: &Path) -> Option<RunManifest> {
    if !manifest_path.exists() {
        write_stderr(&format!(
            "error: Tier B execution requires a prepared manifest with sentinel. \
             Run 'prepare' first. Expected at: {}\n",
            manifest_path.display()
        ));
        return None;
    }
    match load_manifest(manifest_path) {
        Ok(m) => Some(m),
        Err(err) => {
            write_stderr(&format!("failed to load manifest: {err}\n"));
            None
        }
    }
}

/// Record the clone destination and fixture file as owned paths in the
/// manifest, and associate the fixture GitHub repo, before any mutation.
///
/// This ensures cleanup can find and manage these resources even if the
/// run is interrupted.
///
/// **Finding #1**: The clone destination is recorded as `FixtureClone`
/// (not `FixtureRepo`) so the containment validation matches the actual
/// expected sub-directory (`fixture-clone`).
fn record_tier_b_paths(manifest: &mut RunManifest, clone_dest: &Path, validated_repo: &str) {
    // Record the clone destination as an owned fixture-clone path.
    manifest.add_owned_path(OwnedPathKind::FixtureClone, clone_dest.to_path_buf());
    // Do NOT overwrite fixture_repo_path (which is the provisioned local
    // fixture-repo from `prepare`). The clone destination is tracked
    // separately via the FixtureClone owned path above.
    manifest.set_fixture_github_repo(validated_repo);
}
