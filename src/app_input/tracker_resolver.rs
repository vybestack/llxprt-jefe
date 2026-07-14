//! Central resolution of the effective issue/PR tracker target (issue #266).
//!
//! Every issue/PR read and mutation path routes through
//! [`resolve_tracker_outcome`] so that a fork can source issues/PRs
//! from an upstream repository while cloning, origin checks, dashboard/git
//! display, and GitHub Actions continue to use the configured fork
//! (`github_repo`).
//!
//! The resolver delegates to [`Repository::effective_issue_pr_repo`], which
//! selects a nonblank `github_issue_pr_repo` override when valid, and falls
//! back to `github_repo` otherwise. A malformed nonblank override surfaces
//! as [`ResolvedTracker::Malformed`] so it fails visibly — it is
//! never silently mutated to the fallback fork identity.

use jefe::domain::{GitHubRepoRef, GitHubRepoRefError, Repository};

/// Resolved tracker target: the validated `owner/repo` to route issue/PR
/// reads and mutations against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerTarget {
    owner: String,
    repo: String,
}

impl TrackerTarget {
    /// The validated owner component.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The validated repo component.
    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

impl From<&GitHubRepoRef> for TrackerTarget {
    fn from(reference: &GitHubRepoRef) -> Self {
        Self {
            owner: reference.owner().to_owned(),
            repo: reference.repo().to_owned(),
        }
    }
}

/// Error surfaced when the effective tracker target cannot be resolved.
#[cfg(test)]
#[derive(Debug, Clone)]
pub enum ResolveTrackerError {
    /// A nonblank `github_issue_pr_repo` override is malformed. The original
    /// error carries the raw value and categorized reason.
    Malformed(GitHubRepoRefError),
}

#[cfg(test)]
impl std::fmt::Display for ResolveTrackerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(error) => write!(f, "{error}"),
        }
    }
}

#[cfg(test)]
impl std::error::Error for ResolveTrackerError {}

/// Source-aware resolved tracker outcome (issue #266 defect remediation).
///
/// Distinguishes a successfully resolved target from a genuinely absent
/// configuration and malformed input in either the nonblank override or the
/// fallback `github_repo`. Callers that surface user-visible errors must match
/// on [`Self::Malformed`] so the offending raw value and reason reach the UI,
/// rather than collapsing into an indistinguishable missing-repository message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTracker {
    /// A valid `owner/repo` was resolved (override or fallback).
    Resolved(TrackerTarget),
    /// No tracker is configured at all (both override and fallback are blank).
    Absent,
    /// The selected override or fallback is malformed. Carries the original
    /// parse error (raw value + categorized reason) so the UI can surface it.
    Malformed(GitHubRepoRefError),
}

/// Resolve the effective tracker target for a specific repository as a
/// source-aware outcome.
///
/// Returns [`ResolvedTracker`] so the caller can distinguish malformed input
/// from either selected source (with its raw value and reason) from a genuinely
/// absent configuration. This is the preferred entry point for paths that
/// build user-visible error messages.
#[must_use]
pub(super) fn resolve_tracker_outcome(repo: &Repository) -> ResolvedTracker {
    match repo.effective_issue_pr_repo() {
        Ok(Some(reference)) => ResolvedTracker::Resolved(TrackerTarget::from(&reference)),
        Ok(None) => ResolvedTracker::Absent,
        Err(error) => ResolvedTracker::Malformed(error),
    }
}

/// Resolve the effective tracker target for a specific repository.
///
/// Delegates to [`Repository::effective_issue_pr_repo`] so the override
/// selection logic lives in the domain layer. Returns `Ok(Some(target))`
/// when a valid `owner/repo` is available, `Ok(None)` when no tracker is
/// configured, and `Err` when the selected override or fallback is malformed.
#[cfg(test)]
pub(super) fn resolve_tracker_for_repo(
    repo: &Repository,
) -> Result<Option<TrackerTarget>, ResolveTrackerError> {
    match resolve_tracker_outcome(repo) {
        ResolvedTracker::Resolved(target) => Ok(Some(target)),
        ResolvedTracker::Absent => Ok(None),
        ResolvedTracker::Malformed(error) => Err(ResolveTrackerError::Malformed(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{RemoteRepositorySettings, Repository, RepositoryId};

    fn make_repo(github_repo: &str, github_issue_pr_repo: &str) -> Repository {
        Repository {
            id: RepositoryId("repo-1".to_owned()),
            name: "Test".to_owned(),
            slug: "test".to_owned(),
            base_dir: std::path::PathBuf::from("/tmp/test"),
            default_profile: String::new(),
            default_code_puppy_model: String::new(),
            github_repo: github_repo.to_owned(),
            github_issue_pr_repo: github_issue_pr_repo.to_owned(),
            remote: RemoteRepositorySettings::default(),
            issue_base_prompt: String::new(),
            default_agent_kind: jefe::domain::AgentKind::Llxprt,
            agent_ids: Vec::new(),
        }
    }

    fn target_or_panic(
        result: Result<Option<TrackerTarget>, ResolveTrackerError>,
        context: &str,
    ) -> TrackerTarget {
        match result {
            Ok(Some(target)) => target,
            Ok(None) => panic!("{context}: expected target, got none"),
            Err(error) => panic!("{context}: {error}"),
        }
    }

    fn error_or_panic(
        result: Result<Option<TrackerTarget>, ResolveTrackerError>,
        context: &str,
    ) -> ResolveTrackerError {
        match result {
            Err(error) => error,
            Ok(target) => panic!("{context}: expected error, got {target:?}"),
        }
    }

    #[test]
    fn blank_override_falls_back_to_github_repo() {
        let repo = make_repo("acme/widgets", "");
        let target = target_or_panic(resolve_tracker_for_repo(&repo), "valid fallback");
        assert_eq!(target.owner(), "acme");
        assert_eq!(target.repo(), "widgets");
    }

    #[test]
    fn nonblank_override_takes_precedence() {
        let repo = make_repo("acme/widgets", "vybestack/llxprt-jefe");
        let target = target_or_panic(resolve_tracker_for_repo(&repo), "valid override");
        assert_eq!(target.owner(), "vybestack");
        assert_eq!(target.repo(), "llxprt-jefe");
    }

    #[test]
    fn both_blank_yields_none() {
        let repo = make_repo("", "");
        assert!(matches!(resolve_tracker_for_repo(&repo), Ok(None)));
    }

    #[test]
    fn malformed_override_errors_visibly() {
        let repo = make_repo("acme/widgets", "not-a-valid-repo");
        let err = error_or_panic(
            resolve_tracker_for_repo(&repo),
            "malformed override must fail",
        );
        match err {
            ResolveTrackerError::Malformed(error) => {
                assert!(error.raw.contains("not-a-valid-repo"));
            }
        }
    }

    #[test]
    fn malformed_override_does_not_silently_use_fallback() {
        let repo = make_repo("acme/widgets", "https://github.com/a/b");
        let result = resolve_tracker_for_repo(&repo);
        assert!(
            result.is_err(),
            "malformed override must not silently fall back to github_repo"
        );
    }

    #[test]
    fn whitespace_only_fields_are_treated_as_blank() {
        let repo = make_repo("  ", "  ");
        assert!(matches!(resolve_tracker_for_repo(&repo), Ok(None)));
    }

    #[test]
    fn malformed_fallback_with_blank_override_errors_visibly() {
        let repo = make_repo("not-a-valid-repo", "");
        assert!(resolve_tracker_for_repo(&repo).is_err());
    }
}
