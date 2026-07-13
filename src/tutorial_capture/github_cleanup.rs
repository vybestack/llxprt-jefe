//! GitHub cleanup: manifest-scoped resource closing/deletion with safety
//! validation and idempotent handling.
//!
//! Extracted from `github_executor.rs` to keep file sizes under the project
//! limit.
//!
//! ## Boundary
//!
//! This module plans and executes GitHub cleanup commands via an injectable
//! command runner. It does not call tmux or the Jefe binary.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use super::allowlist::FixtureAllowlist;
use super::github_executor::{CommandRunner, PlannedCommand, TierBError};
use super::manifest::{GitHubResource, GitHubResourceKind, RunManifest};

/// Plan GitHub cleanup commands for manifest-owned resources.
///
/// Only resources recorded in the manifest by the current run are planned for
/// deletion. This prevents cleanup from touching unrelated resources.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn plan_github_cleanup(manifest: &RunManifest) -> Vec<PlannedCommand> {
    manifest
        .github_resources
        .iter()
        .map(plan_cleanup_for_resource)
        .collect()
}

/// Plan a cleanup command for a single GitHub resource.
fn plan_cleanup_for_resource(resource: &GitHubResource) -> PlannedCommand {
    match resource.kind {
        GitHubResourceKind::PullRequest => plan_close_pr(resource),
        GitHubResourceKind::Issue => plan_close_issue(resource),
        GitHubResourceKind::Branch => plan_delete_branch(resource),
    }
}

/// Plan a `gh pr close --delete-branch` command.
fn plan_close_pr(resource: &GitHubResource) -> PlannedCommand {
    PlannedCommand {
        description: format!(
            "Close fixture PR #{} in {}",
            resource.identifier, resource.repository
        ),
        program: "gh".to_string(),
        argv: vec![
            "pr".to_string(),
            "close".to_string(),
            resource.identifier.clone(),
            "--repo".to_string(),
            resource.repository.clone(),
            "--delete-branch".to_string(),
        ],
        cwd: None,
    }
}

/// Plan a `gh issue close --reason "not planned"` command.
fn plan_close_issue(resource: &GitHubResource) -> PlannedCommand {
    PlannedCommand {
        description: format!(
            "Close fixture issue #{} in {}",
            resource.identifier, resource.repository
        ),
        program: "gh".to_string(),
        argv: vec![
            "issue".to_string(),
            "close".to_string(),
            resource.identifier.clone(),
            "--repo".to_string(),
            resource.repository.clone(),
            "--reason".to_string(),
            "not planned".to_string(),
        ],
        cwd: None,
    }
}

/// Plan a `gh api ... DELETE` command for a branch ref.
fn plan_delete_branch(resource: &GitHubResource) -> PlannedCommand {
    PlannedCommand {
        description: format!(
            "Delete fixture branch {} in {}",
            resource.identifier, resource.repository
        ),
        program: "gh".to_string(),
        argv: vec![
            "api".to_string(),
            format!(
                "repos/{}/git/refs/heads/{}",
                resource.repository, resource.identifier
            ),
            "-X".to_string(),
            "DELETE".to_string(),
        ],
        cwd: None,
    }
}

/// Outcome of attempting to clean a single GitHub resource.
///
/// **Finding #4**: Per-resource outcomes are recorded so the manifest can
/// persist which resources were cleaned, skipped, or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCleanupOutcome {
    /// Description of what was attempted.
    pub description: String,
    /// The repository the resource belongs to.
    pub repository: String,
    /// The resource identifier.
    pub identifier: String,
    /// Whether the resource was cleaned, skipped (validation failure), or
    /// the command failed.
    pub status: GithubCleanupStatus,
}

/// Status of a single GitHub cleanup attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubCleanupStatus {
    /// The resource was successfully cleaned (closed/deleted).
    Cleaned,
    /// The resource was skipped because it failed validation (not fixture
    /// repo, production repo, empty identifier, etc.).
    Skipped { reason: String },
    /// The cleanup command failed.
    Failed { stderr: String },
}

/// Validate a single GitHub resource for cleanup safety.
///
/// **Finding #4**: Each resource must:
/// - Have a nonempty identifier.
/// - Belong to the manifest's fixture GitHub repo (if one is set).
/// - Not be a production repository.
/// - Be in the allowlist (if one is provided).
///
/// Returns `Ok(())` if the resource passes all checks, or `Err(reason)`
/// with a human-readable reason if it should be skipped.
pub(super) fn validate_resource_for_cleanup(
    resource: &GitHubResource,
    manifest: &RunManifest,
    allowlist: Option<&FixtureAllowlist>,
) -> Result<(), String> {
    if resource.identifier.is_empty() {
        return Err("empty identifier".to_string());
    }
    if let Some(fixture_repo) = &manifest.fixture_github_repo
        && !fixture_repo.eq_ignore_ascii_case(&resource.repository)
    {
        return Err(format!(
            "resource repository '{}' does not match fixture repo '{}'",
            resource.repository, fixture_repo
        ));
    }
    // Always refuse production repos.
    let prod_repos = ["vybestack/jefe", "vybestack/llxprt-jefe"];
    if prod_repos
        .iter()
        .any(|prod| prod.eq_ignore_ascii_case(&resource.repository))
    {
        return Err(format!(
            "resource repository '{}' is a production repository",
            resource.repository
        ));
    }
    // Finding #3: revalidate against creation-time allowlist provenance.
    if !manifest.creation_allowlist.is_empty()
        && !manifest.was_creation_allowed(&resource.repository)
    {
        return Err(format!(
            "resource repository '{}' was not in the creation-time allowlist",
            resource.repository
        ));
    }
    // If an allowlist is provided, the resource's repo must be allowed.
    if let Some(list) = allowlist
        && !list.is_allowed(&resource.repository)
    {
        return Err(format!(
            "resource repository '{}' is not in the cleanup allowlist",
            resource.repository
        ));
    }
    Ok(())
}

/// Whether a command failure stderr indicates the resource was already
/// closed/deleted (idempotent success).
///
/// **Finding #3**: Treat already-closed/deleted as idempotent success
/// where gh output/status supports it.
///
/// Also recognizes merged PRs and auto-deleted branches (when a PR merge
/// auto-deletes the branch, the branch ref no longer exists).
fn is_already_closed_or_deleted(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("already closed")
        || lower.contains("already deleted")
        || lower.contains("no such")
        || lower.contains("not found")
        || lower.contains("does not exist")
        || lower.contains("already been closed")
        || lower.contains("already merged")
        || lower.contains("reference does not exist")
        || lower.contains("branch not found")
}

/// Execute GitHub cleanup using the provided command runner.
///
/// **Finding #4**: Each resource is validated before cleanup. Resources that
/// fail validation are skipped (fail-closed) and recorded as `Skipped`.
/// Resources whose cleanup command fails are recorded as `Failed`.
/// The function returns an error only if at least one cleanup command fails;
/// per-resource outcomes are always returned.
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
    execute_github_cleanup_with_allowlist(manifest, runner, None)
}

/// Execute GitHub cleanup with an explicit allowlist for per-resource
/// validation.
///
/// **Finding #4**: Validates all resources against the fixture GitHub repo,
/// the allowlist, production refusal, and nonempty identifiers.
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
    check_creation_provenance(manifest)?;
    let commands = plan_github_cleanup(manifest);
    let resources: &[GitHubResource] = &manifest.github_resources;
    let mut outcomes = Vec::new();
    let mut had_failure = false;

    for (cmd, resource) in commands.iter().zip(resources.iter()) {
        let outcome = execute_single_cleanup(cmd, resource, manifest, allowlist, runner);
        if matches!(outcome.status, GithubCleanupStatus::Failed { .. }) {
            had_failure = true;
        }
        outcomes.push(outcome);
    }

    finalize_cleanup_outcomes(outcomes, had_failure)
}

/// Fail-closed check: if GitHub resources exist but creation provenance is
/// empty, refuse to clean rather than risk touching uncertain-origin resources.
fn check_creation_provenance(manifest: &RunManifest) -> Result<(), TierBError> {
    if manifest.github_resources.is_empty() || !manifest.creation_allowlist.is_empty() {
        return Ok(());
    }
    Err(TierBError::CleanupPartialFailure {
        outcomes: manifest
            .github_resources
            .iter()
            .map(|r| GithubCleanupOutcome {
                description: format!(
                    "Cleanup of {} #{}",
                    resource_kind_label(r.kind),
                    r.identifier
                ),
                repository: r.repository.clone(),
                identifier: r.identifier.clone(),
                status: GithubCleanupStatus::Skipped {
                    reason: "creation allowlist provenance is empty — cannot verify cleanup authorization".to_string(),
                },
            })
            .collect(),
    })
}

/// Execute cleanup for a single resource: validate, run, and record outcome.
fn execute_single_cleanup(
    cmd: &PlannedCommand,
    resource: &GitHubResource,
    manifest: &RunManifest,
    allowlist: Option<&FixtureAllowlist>,
    runner: &mut dyn CommandRunner,
) -> GithubCleanupOutcome {
    match validate_resource_for_cleanup(resource, manifest, allowlist) {
        Err(reason) => GithubCleanupOutcome {
            description: cmd.description.clone(),
            repository: resource.repository.clone(),
            identifier: resource.identifier.clone(),
            status: GithubCleanupStatus::Skipped { reason },
        },
        Ok(()) => match runner.run(&cmd.program, &cmd.argv, cmd.cwd.as_deref()) {
            Ok(_) => cleaned_outcome(cmd, resource),
            Err(stderr) => {
                if is_already_closed_or_deleted(&stderr) {
                    cleaned_outcome(cmd, resource)
                } else {
                    GithubCleanupOutcome {
                        description: cmd.description.clone(),
                        repository: resource.repository.clone(),
                        identifier: resource.identifier.clone(),
                        status: GithubCleanupStatus::Failed { stderr },
                    }
                }
            }
        },
    }
}

/// Build a `Cleaned` outcome for a resource.
fn cleaned_outcome(cmd: &PlannedCommand, resource: &GitHubResource) -> GithubCleanupOutcome {
    GithubCleanupOutcome {
        description: cmd.description.clone(),
        repository: resource.repository.clone(),
        identifier: resource.identifier.clone(),
        status: GithubCleanupStatus::Cleaned,
    }
}

/// Finalize cleanup: return error if any failures or skips occurred.
fn finalize_cleanup_outcomes(
    outcomes: Vec<GithubCleanupOutcome>,
    had_failure: bool,
) -> Result<Vec<GithubCleanupOutcome>, TierBError> {
    let had_skips = outcomes
        .iter()
        .any(|o| matches!(o.status, GithubCleanupStatus::Skipped { .. }));
    if had_failure || had_skips {
        Err(TierBError::CleanupPartialFailure { outcomes })
    } else {
        Ok(outcomes)
    }
}

/// Human-readable label for a GitHub resource kind.
fn resource_kind_label(kind: GitHubResourceKind) -> &'static str {
    match kind {
        GitHubResourceKind::Issue => "issue",
        GitHubResourceKind::Branch => "branch",
        GitHubResourceKind::PullRequest => "pull request",
    }
}

#[cfg(test)]
#[path = "github_cleanup_tests.rs"]
mod tests;
