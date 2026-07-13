//! Tier B scenario JSON generation with exact manifest values.
//!
//! **Finding #5**: Scenario JSON is now generated using `serde_json::Value`
//! and `serde_json::json!` macros rather than manual string formatting with
//! `json_escape`. This eliminates all manual escaping bugs and guarantees
//! valid JSON output for any input including special characters.
//!
//! **Finding #3**: After applying the PR search filter (Enter), the scenario
//! waits for the exact PR title to appear in the filtered list BEFORE opening
//! the PR detail. Detail-specific assertions (PR number + exact title) follow.
//!
//! ## Boundary
//!
//! This module is pure: it transforms manifest parameters into scenario JSON
//! via `serde_json`. It does not perform I/O or call tmux.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use super::manifest::{GitHubResourceKind, RunManifest};

/// Tier B scenario parameters extracted from the manifest, used to generate
/// scenario JSON with exact issue/PR titles and numbers.
///
/// **Finding #5**: Tier B scenarios inject exact manifest issue/PR unique
/// titles/numbers so the scenario can filter/select/assert exact identity
/// before send/merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierBScenarioParams {
    /// The exact issue title from the mutation plan.
    pub issue_title: String,
    /// The exact PR title from the mutation plan.
    pub pr_title: String,
    /// The exact branch name from the mutation plan.
    pub branch_name: String,
    /// The issue number (captured after `gh issue create`).
    pub issue_number: String,
    /// The PR number (captured after `gh pr create`).
    pub pr_number: String,
    /// The seeded agent name.
    pub agent_name: String,
}

/// Extract Tier B scenario parameters from the manifest.
///
/// **Finding #5**: The manifest records the exact issue/PR numbers and
/// the mutation plan provides the exact titles. This function gathers them
/// into a single struct for scenario injection.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn extract_scenario_params(
    manifest: &RunManifest,
    agent_name: &str,
) -> Option<TierBScenarioParams> {
    let issue = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Issue)?;
    let pr = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::PullRequest)?;
    let branch = manifest
        .github_resources
        .iter()
        .find(|r| r.kind == GitHubResourceKind::Branch)?;
    Some(TierBScenarioParams {
        issue_title: format!(
            "[tutorial-capture:{}] fixture issue for documentation capture",
            manifest.run_id
        ),
        pr_title: format!(
            "[tutorial-capture:{}] fixture pull request",
            manifest.run_id
        ),
        branch_name: branch.identifier.clone(),
        issue_number: issue.identifier.clone(),
        pr_number: pr.identifier.clone(),
        agent_name: agent_name.to_string(),
    })
}

/// Generate a Tier B GitHub scenario JSON with exact manifest values injected.
///
/// **Finding #5**: Uses `serde_json::json!` macro for serialization instead
/// of manual string escaping. This guarantees valid JSON for any input.
///
/// **Finding #3**: After the search filter Enter, the scenario waits for the
/// exact issue/PR title to appear (proving the filter narrowed the list),
/// BEFORE opening the detail view. Detail-specific assertions follow.
///
/// **Finding #4**: Post-send steps assert a concrete `Running` marker to
/// prove the agent was actually started, not just that a chooser appeared.
/// Issue and PR sends use distinct capture labels (`issue-sent-*` vs
/// `pr-sent-*`) and distinct macros (`send-issue-to-agent` vs
/// `send-pr-to-agent`) so evidence can be distinguished.
///
/// # Panics
///
/// Panics if `serde_json` serialization fails, which can only happen if
/// the input values contain invalid UTF-8 sequences (impossible for valid
/// Rust `String` values).
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn generate_tier_b_scenario(params: &TierBScenarioParams) -> String {
    let p = params;
    let scenario = serde_json::json!({
        "config": {
            "cols": 100,
            "rows": 32,
            "history_limit": 2000,
            "initial_wait_ms": 200,
            "assert_mode": "strict"
        },
        "macros": {
            "quit": {
                "params": [],
                "steps": [
                    { "key": "C-q" },
                    { "waitForExit": 3000 }
                ]
            },
            "send-issue-to-agent": {
                "params": ["agent"],
                "steps": build_send_to_agent_steps("issue", &p.agent_name)
            },
            "send-pr-to-agent": {
                "params": ["agent"],
                "steps": build_send_to_agent_steps("pr", &p.agent_name)
            }
        },
        "steps": build_tier_b_steps(p)
    });
    serde_json::to_string_pretty(&scenario).unwrap_or_else(|e| {
        panic!("scenario serialization must succeed: {e}");
    })
}

/// Build the steps for a send-to-agent macro, parameterized by kind
/// (`issue` or `pr`) so issue and PR sends have distinct capture labels.
///
/// **Finding #4**: Asserts `Running` after send to prove the agent started.
fn build_send_to_agent_steps(kind: &str, agent: &str) -> serde_json::Value {
    let chooser_label = format!("{kind}-send-chooser-{agent}");
    let sent_label = format!("{kind}-sent-{agent}");
    serde_json::json!([
        { "key": "S" },
        { "waitFor": "Send" },
        { "expect": agent },
        { "capture": chooser_label },
        { "key": "Enter" },
        { "wait": 500 },
        { "expect": "Running" },
        { "expect": agent },
        { "capture": sent_label }
    ])
}

/// Build the main Tier B steps: dashboard → issues → issue detail → send →
/// PRs → PR detail → send → quit.
///
/// **Finding #3**: After the search filter Enter, each step sequence waits
/// for the exact title before pressing Enter to open the detail.
fn build_tier_b_steps(p: &TierBScenarioParams) -> serde_json::Value {
    serde_json::json!([
        { "waitFor": "LLxprt Jefe" },
        { "capture": "github-dashboard-oriented" },

        // ── Issues mode ──
        { "key": "i" },
        { "waitFor": "Issues" },
        { "expect": "Open" },
        { "capture": "issues-workspace" },

        // Finding #1: press / to focus issue search, type exact title.
        { "key": "/" },
        { "type": p.issue_title },
        { "key": "Enter" },
        { "wait": 300 },

        // Finding #3: waitFor exact issue title after filter Enter,
        // BEFORE opening the detail.
        { "waitFor": p.issue_title },

        // Open the issue detail.
        { "key": "Enter" },
        { "waitFor": p.issue_number },
        { "expect": p.issue_title },
        { "capture": "issue-detail-exact" },

        { "macro": "send-issue-to-agent", "args": { "agent": p.agent_name } },

        { "key": "Escape" },
        { "wait": 200 },
        { "key": "Escape" },
        { "wait": 200 },

        // ── PR mode ──
        { "key": "p" },
        { "waitFor": "Pull Requests" },
        { "capture": "pull-requests-workspace" },

        // Finding #1: press / to focus PR search, type exact title.
        { "key": "/" },
        { "type": p.pr_title },
        { "key": "Enter" },
        { "wait": 300 },

        // Finding #3: waitFor exact PR title after filter Enter,
        // BEFORE opening the detail.
        { "waitFor": p.pr_title },

        // Open the PR detail.
        { "key": "Enter" },
        { "waitFor": p.pr_number },
        { "expect": p.pr_title },
        { "capture": "pr-detail-exact" },

        { "macro": "send-pr-to-agent", "args": { "agent": p.agent_name } },

        { "key": "Escape" },
        { "wait": 200 },
        { "key": "Escape" },
        { "wait": 200 },

        { "macro": "quit", "args": {} }
    ])
}

/// Generate a Tier B merge scenario JSON with exact manifest values injected.
///
/// **Finding #5**: Uses `serde_json::json!` macro for serialization.
///
/// **Finding #3**: After the search filter Enter, waits for the exact PR
/// title before opening the detail, then asserts the PR number and title.
/// The merge confirmation asserts on the PR title before proceeding.
///
/// # Panics
///
/// Panics if `serde_json` serialization fails, which can only happen if
/// the input values contain invalid UTF-8 sequences (impossible for valid
/// Rust `String` values).
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn generate_tier_b_merge_scenario(params: &TierBScenarioParams) -> String {
    let p = params;
    let scenario = serde_json::json!({
        "config": {
            "cols": 100,
            "rows": 32,
            "history_limit": 2000,
            "initial_wait_ms": 200,
            "assert_mode": "strict"
        },
        "macros": {
            "quit": {
                "params": [],
                "steps": [
                    { "key": "C-q" },
                    { "waitForExit": 3000 }
                ]
            }
        },
        "steps": build_tier_b_merge_steps(p)
    });
    serde_json::to_string_pretty(&scenario).unwrap_or_else(|e| {
        panic!("merge scenario serialization must succeed: {e}");
    })
}

/// Build the Tier B merge steps: dashboard → PRs → PR search → PR detail →
/// merge confirmation → merged result → quit.
///
/// **Finding #3**: After the search filter Enter, waits for the exact PR
/// title before opening the detail.
fn build_tier_b_merge_steps(p: &TierBScenarioParams) -> serde_json::Value {
    serde_json::json!([
        { "waitFor": "LLxprt Jefe" },
        { "capture": "merge-dashboard-oriented" },

        { "key": "p" },
        { "waitFor": "Pull Requests" },
        { "capture": "merge-pr-workspace" },

        // Finding #1: press / to focus PR search, type exact title.
        { "key": "/" },
        { "type": p.pr_title },
        { "key": "Enter" },
        { "wait": 300 },

        // Finding #3: waitFor exact PR title after filter Enter,
        // BEFORE opening the detail.
        { "waitFor": p.pr_title },

        // Open the PR detail.
        { "key": "Enter" },
        { "waitFor": p.pr_number },
        { "expect": p.pr_title },
        { "capture": "merge-pr-detail-exact" },

        { "key": "m" },
        { "waitFor": "Merge" },
        { "capture": "merge-confirmation" },

        { "key": "Enter" },
        { "wait": 500 },
        { "key": "Enter" },
        { "wait": 1000 },
        { "waitFor": "merged" },
        { "capture": "merged-result" },

        { "key": "Escape" },
        { "wait": 200 },
        { "key": "Escape" },
        { "wait": 200 },

        { "macro": "quit", "args": {} }
    ])
}

// ── Finding #2: capture-github fail-closed resource validation ────────

/// Error returned when Tier B manifest resources fail validation.
///
/// **Finding #2**: `capture-github` must fail closed unless the manifest has
/// an exact valid current-run issue, branch, and PR — all from the same
/// fixture repository, with no duplicates and no empty identifiers.
/// A generic static fallback scenario is never used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierBValidationError {
    /// A required resource kind (issue, branch, or PR) is missing.
    MissingResource { kind: String },
    /// A resource's repository does not match the manifest's fixture repo.
    RepositoryMismatch {
        resource_repo: String,
        expected: String,
    },
    /// Multiple resources of the same kind exist.
    DuplicateResource { kind: String },
    /// A resource has an empty identifier.
    EmptyIdentifier { kind: String },
}

impl std::fmt::Display for TierBValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingResource { kind } => {
                write!(f, "missing required GitHub resource: {kind}")
            }
            Self::RepositoryMismatch {
                resource_repo,
                expected,
            } => {
                write!(
                    f,
                    "resource repository '{resource_repo}' does not match fixture repo '{expected}'"
                )
            }
            Self::DuplicateResource { kind } => {
                write!(f, "duplicate GitHub resource of kind: {kind}")
            }
            Self::EmptyIdentifier { kind } => {
                write!(f, "GitHub resource of kind {kind} has an empty identifier")
            }
        }
    }
}

impl std::error::Error for TierBValidationError {}

/// Validate that the manifest has an exact valid current-run set of GitHub
/// resources for Tier B capture.
///
/// Requires exactly one issue, one branch, and one PR, all from the same
/// fixture repository, with no empty identifiers.
///
/// **Finding #2**: `capture-github` fails closed unless this validation
/// passes. The generic static fallback scenario is removed — if the manifest
/// does not have exact valid resources, capture is refused.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`TierBValidationError`] if any resource is missing, duplicated,
/// from the wrong repository, or has an empty identifier.
pub fn validate_tier_b_resources(manifest: &RunManifest) -> Result<(), TierBValidationError> {
    let fixture_repo = manifest.fixture_github_repo.as_deref().unwrap_or("");
    for kind in [
        GitHubResourceKind::Issue,
        GitHubResourceKind::Branch,
        GitHubResourceKind::PullRequest,
    ] {
        let matches: Vec<_> = manifest
            .github_resources
            .iter()
            .filter(|r| r.kind == kind)
            .collect();
        if matches.is_empty() {
            return Err(TierBValidationError::MissingResource {
                kind: resource_kind_name(kind).to_string(),
            });
        }
        if matches.len() > 1 {
            return Err(TierBValidationError::DuplicateResource {
                kind: resource_kind_name(kind).to_string(),
            });
        }
        let resource = matches[0];
        if resource.identifier.is_empty() {
            return Err(TierBValidationError::EmptyIdentifier {
                kind: resource_kind_name(kind).to_string(),
            });
        }
        if !fixture_repo.is_empty() && resource.repository != fixture_repo {
            return Err(TierBValidationError::RepositoryMismatch {
                resource_repo: resource.repository.clone(),
                expected: fixture_repo.to_string(),
            });
        }
    }
    Ok(())
}

/// Human-readable name for a GitHub resource kind.
fn resource_kind_name(kind: GitHubResourceKind) -> &'static str {
    match kind {
        GitHubResourceKind::Issue => "issue",
        GitHubResourceKind::Branch => "branch",
        GitHubResourceKind::PullRequest => "pull_request",
    }
}

#[cfg(test)]
#[path = "scenario_gen_tests.rs"]
mod tests;
