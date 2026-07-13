//! Tier-B GitHub fixture executor: command planning and safe execution.
//!
//! This module plans and executes GitHub mutations (issue/branch/commit/PR
//! creation) against a fixture repository, with:
//!
//! - **Allowlist gating**: the fixture repository must be explicitly
//!   allowlisted and must not be a production repo.
//! - **Authenticated target validation**: verifies `gh` CLI is authenticated.
//! - **Production refusal**: the Jefe production repository is always refused.
//! - **Explicit disposable confirmation**: requires `--confirm-disposable`.
//! - **Correct executor sequence**: clone → change → commit → push →
//!   gh issue create → gh pr create.
//! - **Immediate manifest recording**: every resource is recorded after
//!   creation, before any later mutation.
//! - **Injectable command runner**: tests use a fake runner; no live mutation.
//! - **Scoped cleanup**: cleanup only touches manifest-recorded resources.
//!
//! ## Boundary
//!
//! This module plans commands (pure) and executes them via an injectable
//! command runner trait. It does not call tmux or the Jefe binary.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::path::{Path, PathBuf};

use super::allowlist::{
    AllowlistDecision, FixtureAllowlist, FixtureMutationPlan, build_mutation_plan,
};
use super::manifest::{GitHubResource, GitHubResourceKind, RunManifest};

/// A parsed GitHub resource URL: `https://github.com/<owner>/<repo>/<issues|pull>/<number>`.
///
/// **Finding #2**: gh outputs are parsed as strict canonical URLs so the
/// manifest records validated repo, kind, and numeric id — never arbitrary
/// text that could be misinterpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGitHubUrl {
    /// The `owner/repo` extracted from the URL path.
    pub repo: String,
    /// The resource kind inferred from the URL path segment.
    pub kind: GitHubResourceKind,
    /// The numeric identifier extracted from the last path segment.
    pub number: String,
}

/// The canonical GitHub URL prefix for resource URLs.
const GITHUB_URL_PREFIX: &str = "https://github.com/";

/// Parse a `gh issue/pr create` output as a strict canonical GitHub URL.
///
/// Accepts only `https://github.com/<owner>/<repo>/<issues|pull>/<number>`
/// where `<number>` is a positive integer. Leading/trailing whitespace is
/// trimmed. Returns `None` if the URL is not canonical, not github.com, or
/// the number is not a positive integer.
///
/// **Finding #2**: Replaces the naive `rsplit('/')` extraction with proper
/// URL validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn parse_github_resource_url(stdout: &str) -> Option<ParsedGitHubUrl> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    let rest = trimmed.strip_prefix(GITHUB_URL_PREFIX)?;
    // Strip query string and fragment if present.
    let path = rest.split(['?', '#']).next()?;
    let path = path.trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // Expected: owner/repo/issues|pull/number
    if segments.len() != 4 {
        return None;
    }
    let owner = segments[0];
    let repo_name = segments[1];
    let kind_segment = segments[2];
    let number_str = segments[3];
    let kind = match kind_segment {
        "issues" => GitHubResourceKind::Issue,
        "pull" => GitHubResourceKind::PullRequest,
        _ => return None,
    };
    // Validate the number is a positive integer.
    if number_str.is_empty() || !number_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Validate owner/repo format (basic: nonempty, valid chars).
    if owner.is_empty() || repo_name.is_empty() {
        return None;
    }
    Some(ParsedGitHubUrl {
        repo: format!("{owner}/{repo_name}"),
        kind,
        number: number_str.to_string(),
    })
}

/// A planned command: the program and argv that would be executed.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    /// Human-readable description of what the command does.
    pub description: String,
    /// The program name (e.g. "gh", "git").
    pub program: String,
    /// The argv (not including the program name).
    pub argv: Vec<String>,
    /// Optional working directory for the command.
    pub cwd: Option<PathBuf>,
}

/// Trait for executing commands. In production, `RealCommandRunner` calls
/// `std::process::Command`. In tests, a fake runner records commands and
/// returns canned output.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
pub trait CommandRunner {
    /// Execute a command, returning stdout on success or stderr on failure.
    fn run(&mut self, program: &str, argv: &[String], cwd: Option<&Path>)
    -> Result<String, String>;
}

/// Real command runner that shells out to `std::process::Command`.
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(
        &mut self,
        program: &str,
        argv: &[String],
        cwd: Option<&Path>,
    ) -> Result<String, String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(argv);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let output = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

/// Result of planning all Tier-B mutations.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone)]
pub struct TierBPlan {
    /// The validated fixture repository (`owner/repo`).
    pub repository: String,
    /// The mutation plan (issue/branch/PR titles).
    pub mutation_plan: FixtureMutationPlan,
    /// All commands that would be executed, in the correct sequence.
    pub commands: Vec<PlannedCommand>,
    /// Whether merge is planned.
    pub merge: bool,
}

/// Error returned by Tier-B operations.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierBError {
    /// The repository is not allowlisted or is a production repo.
    FixtureRefused { repo: String, reason: String },
    /// `gh` is not authenticated or not available.
    NotAuthenticated { reason: String },
    /// The target repository does not exist.
    RepoNotFound { repo: String },
    /// Disposable confirmation was not given.
    NotConfirmed,
    /// A command failed.
    CommandFailed { description: String, stderr: String },
    /// The manifest does not record the fixture repository.
    NoFixtureRepo,
    /// The clone destination already exists.
    CloneDestinationExists { path: PathBuf },
    /// One or more cleanup commands failed. Per-resource outcomes are included
    /// so the caller can persist them.
    ///
    /// **Finding #4**: Cleanup failures are returned with detailed outcomes
    /// rather than just the first error.
    CleanupPartialFailure { outcomes: Vec<GithubCleanupOutcome> },
    /// The clone destination is not the expected `run-root/fixture-clone`
    /// path. Arbitrary external clone destinations are rejected before
    /// any mutation to prevent cleanup from touching unmanaged paths.
    ///
    /// **Finding #4**: Clone destination must be exactly contained within
    /// the run root as `fixture-clone`.
    CloneDestinationNotContained { path: PathBuf, reason: String },
}

impl std::fmt::Display for TierBError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FixtureRefused { repo, reason } => {
                write!(f, "fixture refused for '{repo}': {reason}")
            }
            Self::NotAuthenticated { reason } => {
                write!(f, "gh not authenticated: {reason}")
            }
            Self::RepoNotFound { repo } => {
                write!(f, "repository '{repo}' not found via gh")
            }
            Self::NotConfirmed => {
                write!(
                    f,
                    "disposable confirmation not given: pass --confirm-disposable to proceed"
                )
            }
            Self::CommandFailed {
                description,
                stderr,
            } => {
                write!(f, "command '{description}' failed: {stderr}")
            }
            Self::NoFixtureRepo => {
                write!(f, "manifest does not record a fixture GitHub repository")
            }
            Self::CloneDestinationExists { path } => {
                write!(f, "clone destination already exists: '{}'", path.display())
            }
            Self::CleanupPartialFailure { outcomes } => {
                let failed = outcomes
                    .iter()
                    .filter(|o| matches!(o.status, GithubCleanupStatus::Failed { .. }))
                    .count();
                write!(f, "{failed} GitHub cleanup command(s) failed")
            }
            Self::CloneDestinationNotContained { path, reason } => {
                write!(
                    f,
                    "clone destination '{}' is not contained: {reason}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for TierBError {}

/// Plan all Tier-B mutations for a given repository and run ID.
///
/// This is pure: it produces the command list without executing anything.
/// The `allowlist` is consulted to refuse production repos.
///
/// The command sequence is:
/// 1. `gh issue create` — create the fixture issue.
/// 2. `gh repo clone` — clone the fixture repo to a deterministic path.
/// 3. `git checkout -b <branch>` — create and switch to the feature branch.
/// 4. Write a changed file (done by orchestration, not a command).
/// 5. `git add` + `git commit` — stage and commit the change.
/// 6. `git push` — push the branch to origin.
/// 7. `gh pr create` — create the fixture PR from the branch.
/// 8. (optional) `gh pr merge` — merge the fixture PR.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
pub fn plan_tier_b(
    allowlist: &FixtureAllowlist,
    repo: &str,
    run_id: &str,
    merge: bool,
    clone_dest: &Path,
) -> Result<TierBPlan, TierBError> {
    match allowlist.evaluate(repo) {
        AllowlistDecision::Allowed => {}
        decision => {
            return Err(TierBError::FixtureRefused {
                repo: repo.to_string(),
                reason: decision.reason(),
            });
        }
    }
    let mutation_plan = build_mutation_plan(repo, run_id, merge);
    let commands = build_command_list(&mutation_plan, clone_dest);
    Ok(TierBPlan {
        repository: mutation_plan.repository.clone(),
        mutation_plan,
        commands,
        merge,
    })
}

/// Build the full command list for a mutation plan, in the correct sequence.
///
/// The merge command is NOT included here because it must reference the
/// PR number that is only known after `gh pr create` runs. The executor
/// builds the merge command dynamically using the captured PR number.
fn build_command_list(plan: &FixtureMutationPlan, clone_dest: &Path) -> Vec<PlannedCommand> {
    vec![
        build_issue_command(plan),
        build_clone_command(plan, clone_dest),
        build_checkout_command(plan, clone_dest),
        build_add_command(plan, clone_dest),
        build_commit_command(plan, clone_dest),
        build_push_command(plan, clone_dest),
        build_pr_command(plan),
    ]
}

/// Build the `gh issue create` command.
///
/// Labels are NOT assumed — the `documentation` label may not exist on the
/// fixture repo. The issue is created without labels. Labels can be probed
/// and applied separately if the fixture repo has them configured.
fn build_issue_command(plan: &FixtureMutationPlan) -> PlannedCommand {
    PlannedCommand {
        description: "Create fixture issue".to_string(),
        program: "gh".to_string(),
        argv: vec![
            "issue".to_string(),
            "create".to_string(),
            "--repo".to_string(),
            plan.repository.clone(),
            "--title".to_string(),
            plan.issue_title.clone(),
            "--body".to_string(),
            "Fixture issue created by jefe-tutorial-capture for documentation capture.".to_string(),
        ],
        cwd: None,
    }
}

/// Build the `gh repo clone` command.
fn build_clone_command(plan: &FixtureMutationPlan, clone_dest: &Path) -> PlannedCommand {
    PlannedCommand {
        description: "Clone fixture repo".to_string(),
        program: "gh".to_string(),
        argv: vec![
            "repo".to_string(),
            "clone".to_string(),
            plan.repository.clone(),
            clone_dest.to_string_lossy().into_owned(),
        ],
        cwd: None,
    }
}

/// Build the `git checkout -b` command.
fn build_checkout_command(plan: &FixtureMutationPlan, clone_dest: &Path) -> PlannedCommand {
    PlannedCommand {
        description: "Create fixture branch".to_string(),
        program: "git".to_string(),
        argv: vec![
            "checkout".to_string(),
            "-b".to_string(),
            plan.branch_name.clone(),
        ],
        cwd: Some(clone_dest.to_path_buf()),
    }
}

/// Build the `git add` command.
fn build_add_command(_plan: &FixtureMutationPlan, clone_dest: &Path) -> PlannedCommand {
    PlannedCommand {
        description: "Stage changed file".to_string(),
        program: "git".to_string(),
        argv: vec!["add".to_string(), "TUTORIAL_FIXTURE.md".to_string()],
        cwd: Some(clone_dest.to_path_buf()),
    }
}

/// Build the `git commit` command.
fn build_commit_command(plan: &FixtureMutationPlan, clone_dest: &Path) -> PlannedCommand {
    PlannedCommand {
        description: "Commit fixture change".to_string(),
        program: "git".to_string(),
        argv: vec![
            "commit".to_string(),
            "-m".to_string(),
            format!("[tutorial-capture] fixture change for {}", plan.branch_name),
        ],
        cwd: Some(clone_dest.to_path_buf()),
    }
}

/// Build the `git push` command.
fn build_push_command(plan: &FixtureMutationPlan, clone_dest: &Path) -> PlannedCommand {
    PlannedCommand {
        description: "Push fixture branch".to_string(),
        program: "git".to_string(),
        argv: vec![
            "push".to_string(),
            "-u".to_string(),
            "origin".to_string(),
            plan.branch_name.clone(),
        ],
        cwd: Some(clone_dest.to_path_buf()),
    }
}

/// Build the `gh pr create` command.
///
/// Labels are NOT assumed — the `documentation` label may not exist on the
/// fixture repo. The PR is created without labels.
fn build_pr_command(plan: &FixtureMutationPlan) -> PlannedCommand {
    PlannedCommand {
        description: "Create fixture PR".to_string(),
        program: "gh".to_string(),
        argv: vec![
            "pr".to_string(),
            "create".to_string(),
            "--repo".to_string(),
            plan.repository.clone(),
            "--title".to_string(),
            plan.pr_title.clone(),
            "--body".to_string(),
            "Fixture PR created by jefe-tutorial-capture for documentation capture.".to_string(),
            "--head".to_string(),
            plan.branch_name.clone(),
        ],
        cwd: None,
    }
}

/// Build the optional `gh pr merge` command.
///
/// **Finding #5**: The merge command explicitly targets the created PR
/// number, not just the repo. The PR number is captured from the
/// `gh pr create` output URL during execution.
///
/// Note: merge is not executed by the setup executor — it is driven
/// through the Jefe UI during capture. This builder is retained for
/// test verification of the merge command structure.
#[cfg(test)]
fn build_merge_command(plan: &FixtureMutationPlan, pr_number: &str) -> PlannedCommand {
    PlannedCommand {
        description: "Merge fixture PR".to_string(),
        program: "gh".to_string(),
        argv: vec![
            "pr".to_string(),
            "merge".to_string(),
            pr_number.to_string(),
            "--repo".to_string(),
            plan.repository.clone(),
            "--squash".to_string(),
            "--delete-branch".to_string(),
        ],
        cwd: None,
    }
}

/// Check that `gh` is authenticated and the target repository exists.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBError`] if `gh` is not available, not authenticated, or the
/// repository does not exist.
pub fn validate_gh_target(repo: &str) -> Result<(), TierBError> {
    let auth_status = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map_err(|e| TierBError::NotAuthenticated {
            reason: format!("failed to run gh: {e}"),
        })?;
    if !auth_status.status.success() {
        let stderr = String::from_utf8_lossy(&auth_status.stderr).to_string();
        return Err(TierBError::NotAuthenticated {
            reason: format!("gh auth status failed: {stderr}"),
        });
    }
    let repo_check = std::process::Command::new("gh")
        .args(["repo", "view", repo, "--json", "name"])
        .output()
        .map_err(|e| TierBError::NotAuthenticated {
            reason: format!("failed to run gh: {e}"),
        })?;
    if !repo_check.status.success() {
        return Err(TierBError::RepoNotFound {
            repo: repo.to_string(),
        });
    }
    Ok(())
}

/// Validate that the clone destination is exactly `run_root/fixture-clone`.
///
/// Arbitrary external clone paths are rejected before any mutation so
/// cleanup can never target unmanaged paths.
///
/// **Finding #4**: The clone destination must be the exact expected
/// sub-directory `fixture-clone` within the run root.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBError::CloneDestinationNotContained`] if the clone
/// destination is not `run_root/fixture-clone`, contains path traversal,
/// or is outside the run root.
pub fn validate_clone_destination(clone_dest: &Path, run_root: &Path) -> Result<(), TierBError> {
    // Reject NUL bytes (path injection defense).
    let dest_str = clone_dest.to_string_lossy();
    if dest_str.contains('\0') {
        return Err(TierBError::CloneDestinationNotContained {
            path: clone_dest.to_path_buf(),
            reason: "NUL byte in path".to_string(),
        });
    }
    // Lexically canonicalize both paths (resolves . and .. without touching fs).
    let canonical_dest = lexical_canonical(clone_dest);
    let canonical_root = lexical_canonical(run_root);
    let expected = canonical_root.join("fixture-clone");
    if canonical_dest != expected {
        return Err(TierBError::CloneDestinationNotContained {
            path: clone_dest.to_path_buf(),
            reason: format!(
                "clone destination must be exactly '{}/fixture-clone' but got '{}'",
                canonical_root.display(),
                canonical_dest.display()
            ),
        });
    }
    Ok(())
}

/// Lexically canonicalize a path: resolve `.` and `..` without touching the
/// filesystem. This is sufficient because the run root is always absolute and
/// sub-paths are constructed by joining known components.
fn lexical_canonical(path: &Path) -> std::path::PathBuf {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
            }
        }
    }
    result
}

/// Plan GitHub cleanup commands for manifest-owned resources.
///
/// Only resources recorded in the manifest by the current run are planned for
/// deletion. This prevents cleanup from touching unrelated resources.
///
/// Delegated to the `github_cleanup` module.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn plan_github_cleanup(manifest: &RunManifest) -> Vec<PlannedCommand> {
    super::github_cleanup::plan_github_cleanup(manifest)
}

/// Execute GitHub cleanup using the provided command runner.
///
/// Delegated to the `github_cleanup` module.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBError`] if any cleanup command fails (after validation).
pub fn execute_github_cleanup(
    manifest: &RunManifest,
    runner: &mut dyn CommandRunner,
) -> Result<Vec<GithubCleanupOutcome>, TierBError> {
    super::github_cleanup::execute_github_cleanup(manifest, runner)
}

/// Execute GitHub cleanup with an explicit allowlist for per-resource
/// validation.
///
/// Delegated to the `github_cleanup` module.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBError`] if any cleanup command fails (after validation).
pub fn execute_github_cleanup_with_allowlist(
    manifest: &RunManifest,
    runner: &mut dyn CommandRunner,
    allowlist: Option<&FixtureAllowlist>,
) -> Result<Vec<GithubCleanupOutcome>, TierBError> {
    super::github_cleanup::execute_github_cleanup_with_allowlist(manifest, runner, allowlist)
}

// Re-export cleanup types for backward compatibility.
pub use super::github_cleanup::{GithubCleanupOutcome, GithubCleanupStatus};

/// Context for executing a Tier-B plan, bundling runtime dependencies.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
pub struct TierBContext<'a> {
    /// Whether disposable confirmation was given.
    pub confirm_disposable: bool,
    /// Whether to skip `gh` validation (for testing).
    pub skip_validation: bool,
    /// Command runner for executing `gh`/`git` commands.
    pub runner: &'a mut dyn CommandRunner,
    /// Destination for `gh repo clone`. Must be `run_root/fixture-clone`.
    pub clone_dest: &'a Path,
    /// The run root directory (parent of `clone_dest`).
    /// Used for clone-destination containment validation.
    ///
    /// **Finding #4**: The clone destination is validated against the run
    /// root before any mutation.
    pub run_root: &'a Path,
    /// Callback to save the manifest atomically.
    pub save_fn: &'a mut dyn FnMut(&RunManifest) -> Result<(), String>,
}

/// Execute the full Tier-B plan using the provided context, recording
/// each resource in the manifest immediately after creation.
///
/// This is the live execution path. It requires:
/// - `confirm_disposable` to be true.
/// - The fixture repository to be validated via `gh` (unless `skip_validation`).
///
/// After each created resource, the manifest is saved atomically via the
/// provided `save` callback.
///
/// **Finding #5**: When merge is planned, the merge command is dynamically
/// built using the PR number captured from the `gh pr create` output. The
/// merge explicitly targets the created PR identifier.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBError`] on any safety check failure or command failure.
pub fn execute_tier_b(
    plan: &TierBPlan,
    manifest: &mut RunManifest,
    ctx: &mut TierBContext,
) -> Result<Vec<String>, TierBError> {
    if !ctx.confirm_disposable {
        return Err(TierBError::NotConfirmed);
    }
    // Finding #4: validate clone destination is exactly run_root/fixture-clone.
    // This is always checked — it is a structural safety requirement, not a
    // network validation that can be skipped.
    validate_clone_destination(ctx.clone_dest, ctx.run_root)?;
    if !ctx.skip_validation {
        validate_gh_target(&plan.repository)?;
    }
    if ctx.clone_dest.exists() {
        return Err(TierBError::CloneDestinationExists {
            path: ctx.clone_dest.to_path_buf(),
        });
    }
    let mut outputs = Vec::new();
    for cmd in &plan.commands {
        let stdout = execute_plan_command(cmd, plan, manifest, ctx)?;
        outputs.push(format!("{}: {}", cmd.description, stdout.trim()));
    }
    // Note: merge is intentionally NOT executed here. The setup executor
    // only creates fixtures (issue, branch, PR). The merge is driven through
    // the real Jefe UI during the capture scenario when --allow-merge is
    // specified. --allow-merge selects the merge capture permission/variant
    // only; it does not trigger a setup-time merge.
    Ok(outputs)
}

/// Execute a single plan command: write fixture file if needed, run, record, save.
///
/// **Finding #3**: Resources are recorded ONLY after the command succeeds.
/// On failure, no resource is recorded — failed outputs are never persisted.
/// The manifest is still saved on failure so partial state is visible, but
/// no `GitHubResource` entry is added for a command that did not complete.
fn execute_plan_command(
    cmd: &PlannedCommand,
    plan: &TierBPlan,
    manifest: &mut RunManifest,
    ctx: &mut TierBContext,
) -> Result<String, TierBError> {
    if cmd.description == "Stage changed file"
        && let Some(cwd) = &cmd.cwd
    {
        let fixture_file = cwd.join("TUTORIAL_FIXTURE.md");
        std::fs::write(
            &fixture_file,
            format!(
                "# Tutorial Fixture\n\nCreated by run {}.\n",
                plan.mutation_plan.branch_name
            ),
        )
        .map_err(|e| TierBError::CommandFailed {
            description: "Write fixture file".to_string(),
            stderr: e.to_string(),
        })?;
    }
    let stdout = match ctx.runner.run(&cmd.program, &cmd.argv, cmd.cwd.as_deref()) {
        Ok(out) => out,
        Err(stderr) => {
            // Finding #3: do NOT record a resource for a failed command.
            // Save the manifest so partial state is visible, but without
            // a spurious GitHubResource entry. The save failure is also
            // fatal — it must propagate rather than being silently swallowed.
            if let Err(save_err) = (ctx.save_fn)(manifest) {
                return Err(TierBError::CommandFailed {
                    description: "save manifest".to_string(),
                    stderr: save_err,
                });
            }
            return Err(TierBError::CommandFailed {
                description: cmd.description.clone(),
                stderr,
            });
        }
    };
    // Finding #3: record the resource only after the command succeeds,
    // using the explicit plan repository and nonempty validated identifiers.
    record_successful_resource(cmd, &stdout, &plan.repository, plan, manifest)?;
    if let Err(e) = (ctx.save_fn)(manifest) {
        return Err(TierBError::CommandFailed {
            description: "save manifest".to_string(),
            stderr: e,
        });
    }
    Ok(stdout)
}

/// Record a created GitHub resource in the manifest based on the command,
/// but ONLY for successful commands with nonempty validated identifiers.
///
/// **Finding #2**:
/// - Uses the explicit plan repository rather than argv-derived values.
/// - Issue and PR resources are recorded when their `gh ... create` commands
///   succeed, with the number extracted via strict URL parsing.
/// - The URL is validated as a canonical `https://github.com/...` URL.
/// - The URL's repo and kind must match the expected plan repo and kind.
/// - The exact title from the mutation plan is persisted with the resource.
/// - Branch resource is recorded when `git push` succeeds, with the branch
///   name from the plan matching `tutorial-capture/<run-id>`.
fn record_successful_resource(
    cmd: &PlannedCommand,
    stdout: &str,
    plan_repo: &str,
    plan: &TierBPlan,
    manifest: &mut RunManifest,
) -> Result<(), TierBError> {
    if cmd.description.starts_with("Create fixture issue") {
        record_issue_resource(cmd, stdout, plan_repo, plan, manifest)?;
    } else if cmd.description.starts_with("Push fixture branch") {
        record_branch_resource(cmd, plan_repo, plan, manifest)?;
    } else if cmd.description.starts_with("Create fixture PR") {
        record_pr_resource(cmd, stdout, plan_repo, plan, manifest)?;
    }
    Ok(())
}

/// Parse and record an issue resource from the `gh issue create` output URL.
fn record_issue_resource(
    cmd: &PlannedCommand,
    stdout: &str,
    plan_repo: &str,
    plan: &TierBPlan,
    manifest: &mut RunManifest,
) -> Result<(), TierBError> {
    let parsed = parse_github_resource_url(stdout).ok_or_else(|| TierBError::CommandFailed {
        description: cmd.description.clone(),
        stderr: format!(
            "could not parse issue URL from gh output: {}",
            stdout.trim()
        ),
    })?;
    validate_parsed_url_matches(&parsed, plan_repo, GitHubResourceKind::Issue, cmd)?;
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: plan_repo.to_string(),
        identifier: parsed.number,
        url: Some(stdout.trim().to_string()),
        title: plan.mutation_plan.issue_title.clone(),
    });
    Ok(())
}

/// Record a branch resource from the mutation plan (after push succeeds).
fn record_branch_resource(
    cmd: &PlannedCommand,
    plan_repo: &str,
    plan: &TierBPlan,
    manifest: &mut RunManifest,
) -> Result<(), TierBError> {
    let branch_name = &plan.mutation_plan.branch_name;
    if branch_name.is_empty() {
        return Err(TierBError::CommandFailed {
            description: cmd.description.clone(),
            stderr: "branch name from mutation plan is empty".to_string(),
        });
    }
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Branch,
        repository: plan_repo.to_string(),
        identifier: branch_name.clone(),
        url: None,
        title: String::new(),
    });
    Ok(())
}

/// Parse and record a PR resource from the `gh pr create` output URL.
fn record_pr_resource(
    cmd: &PlannedCommand,
    stdout: &str,
    plan_repo: &str,
    plan: &TierBPlan,
    manifest: &mut RunManifest,
) -> Result<(), TierBError> {
    let parsed = parse_github_resource_url(stdout).ok_or_else(|| TierBError::CommandFailed {
        description: cmd.description.clone(),
        stderr: format!("could not parse PR URL from gh output: {}", stdout.trim()),
    })?;
    validate_parsed_url_matches(&parsed, plan_repo, GitHubResourceKind::PullRequest, cmd)?;
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::PullRequest,
        repository: plan_repo.to_string(),
        identifier: parsed.number,
        url: Some(stdout.trim().to_string()),
        title: plan.mutation_plan.pr_title.clone(),
    });
    Ok(())
}

/// Validate that a parsed URL matches the expected repo and resource kind.
fn validate_parsed_url_matches(
    parsed: &ParsedGitHubUrl,
    expected_repo: &str,
    expected_kind: GitHubResourceKind,
    cmd: &PlannedCommand,
) -> Result<(), TierBError> {
    if parsed.repo != expected_repo {
        return Err(TierBError::CommandFailed {
            description: cmd.description.clone(),
            stderr: format!(
                "URL repo '{}' does not match plan repo '{}'",
                parsed.repo, expected_repo
            ),
        });
    }
    if parsed.kind != expected_kind {
        return Err(TierBError::CommandFailed {
            description: cmd.description.clone(),
            stderr: format!(
                "expected {:?} URL but got {:?} URL",
                expected_kind, parsed.kind
            ),
        });
    }
    Ok(())
}

/// Extract the issue/PR number from a `gh issue/pr create` output URL.
///
/// **Finding #2**: Now delegates to `parse_github_resource_url` for strict
/// validation, ensuring only canonical GitHub URLs produce numbers.
#[cfg(test)]
fn extract_number_from_url(stdout: &str) -> Option<String> {
    parse_github_resource_url(stdout).map(|p| p.number)
}

// Finding #5: scenario generation extracted to `scenario_gen.rs` to keep
// this module under the 750-line limit.
pub use super::scenario_gen::{
    TierBScenarioParams, TierBValidationError, extract_scenario_params,
    generate_tier_b_merge_scenario, generate_tier_b_scenario, validate_tier_b_resources,
};

#[cfg(test)]
#[path = "github_executor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "github_executor_exact_tests.rs"]
mod exact_tests;
