//! Allowlist for GitHub fixture repositories with production-repo refusal.
//!
//! Tier-B (live GitHub) capture must never mutate the Jefe production
//! repository or any repository not explicitly allowlisted for fixture use.
//! This module owns the pure allowlist/refusal logic; the orchestration layer
//! consults it before any mutation.
//!
//! ## Boundary
//!
//! This module is pure: it evaluates whether a repository is allowed or
//! refused. It does not perform network I/O or call `gh`.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use std::collections::BTreeSet;

/// Repositories that must never be used as fixtures, regardless of allowlist.
/// The Jefe production repository is always refused.
const PRODUCTION_REPOS: &[&str] = &["vybestack/jefe", "vybestack/llxprt-jefe"];

/// The allowlist of fixture repositories.
///
/// A `FixtureAllowlist` is constructed from an explicit set of `owner/repo`
/// strings. An empty allowlist refuses all repositories.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone)]
pub struct FixtureAllowlist {
    repos: BTreeSet<String>,
}

/// Result of evaluating a fixture repository against the allowlist.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowlistDecision {
    /// The repository is explicitly allowlisted and not a production repo.
    Allowed,
    /// The repository is the Jefe production repository.
    ProductionRepoRefused { repo: String },
    /// The repository is not in the allowlist.
    NotAllowlisted { repo: String },
}

impl FixtureAllowlist {
    /// Create an allowlist from an iterator of `owner/repo` strings.
    /// Whitespace is trimmed and case is normalized to lowercase.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-004
    #[must_use]
    pub fn new<I, S>(repos: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let repos = repos
            .into_iter()
            .map(|s| normalize_repo(s.as_ref()))
            .collect();
        Self { repos }
    }

    /// Create an empty allowlist that refuses everything.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            repos: BTreeSet::new(),
        }
    }

    /// Evaluate whether the given `owner/repo` may be used as a fixture.
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-004
    #[must_use]
    pub fn evaluate(&self, repo: &str) -> AllowlistDecision {
        let normalized = normalize_repo(repo);
        if is_production_repo(&normalized) {
            return AllowlistDecision::ProductionRepoRefused { repo: normalized };
        }
        if self.repos.contains(&normalized) {
            AllowlistDecision::Allowed
        } else {
            AllowlistDecision::NotAllowlisted { repo: normalized }
        }
    }

    /// Whether the repository is allowed (convenience over `evaluate`).
    #[must_use]
    pub fn is_allowed(&self, repo: &str) -> bool {
        matches!(self.evaluate(repo), AllowlistDecision::Allowed)
    }

    /// Return the normalized (lowercase, trimmed) repos in this allowlist,
    /// sorted for deterministic serialization.
    ///
    /// Used to persist the creation-time allowlist provenance in the manifest
    /// so cleanup can revalidate resources against the immutable snapshot.
    #[must_use]
    pub fn normalized_repos(&self) -> Vec<String> {
        self.repos.iter().cloned().collect()
    }
}

impl AllowlistDecision {
    /// Whether this decision allows the repository.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Whether this decision refuses the repository.
    #[must_use]
    pub fn is_refused(&self) -> bool {
        !self.is_allowed()
    }

    /// Human-readable reason for the decision.
    #[must_use]
    pub fn reason(&self) -> String {
        match self {
            Self::Allowed => "repository is allowlisted for fixture use".to_string(),
            Self::ProductionRepoRefused { repo } => {
                format!(
                    "'{repo}' is the Jefe production repository and must never be used as a fixture"
                )
            }
            Self::NotAllowlisted { repo } => {
                format!("'{repo}' is not in the fixture allowlist")
            }
        }
    }
}

/// Normalize a repository string: trim whitespace, lowercase.
fn normalize_repo(repo: &str) -> String {
    repo.trim().to_ascii_lowercase()
}

/// Whether a normalized `owner/repo` is a known production repository.
fn is_production_repo(normalized: &str) -> bool {
    PRODUCTION_REPOS
        .iter()
        .any(|prod| prod.eq_ignore_ascii_case(normalized))
}

/// Validate a GitHub `owner/repo` string format.
///
/// Returns `true` if the string matches `owner/repo` where both parts are
/// non-empty and contain only GitHub-valid characters (alphanumeric, hyphens,
/// underscores, dots).
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn is_valid_repo_format(repo: &str) -> bool {
    let Some((owner, name)) = repo.trim().split_once('/') else {
        return false;
    };
    is_valid_github_segment(owner) && is_valid_github_segment(name)
}

/// Error returned when building an allowlist from sources fails.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowlistBuildError {
    /// The explicit allowlist file could not be read.
    AllowlistFileUnreadable { path: String, reason: String },
}

impl std::fmt::Display for AllowlistBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllowlistFileUnreadable { path, reason } => {
                write!(f, "cannot read allowlist file '{path}': {reason}")
            }
        }
    }
}

impl std::error::Error for AllowlistBuildError {}

/// Build a fixture allowlist from independently configured sources:
/// environment variable, file, and CLI flag.
///
/// The allowlist is the union of all sources. The production repository is
/// always refused regardless of allowlist membership.
///
/// - `env_var`: name of an environment variable holding a colon-separated
///   list of `owner/repo` strings (e.g. `FIXTURE_REPO_ALLOWLIST`).
/// - `file_path`: optional path to a file with one `owner/repo` per line.
///   Lines starting with `#` are comments.
/// - `cli_repos`: explicit repos passed via CLI flags.
///
/// **Finding**: If an explicit allowlist file is provided but cannot be read,
/// a typed error is returned rather than silently ignoring the file.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
///
/// # Errors
///
/// Returns [`AllowlistBuildError::AllowlistFileUnreadable`] if an explicit
/// allowlist file path is provided but the file cannot be read.
pub fn build_allowlist_from_sources_checked(
    env_var: Option<&str>,
    file_path: Option<&std::path::Path>,
    cli_repos: &[&str],
) -> Result<FixtureAllowlist, AllowlistBuildError> {
    let mut repos: Vec<String> = Vec::new();
    // CLI flags take highest priority.
    for repo in cli_repos {
        repos.push((*repo).to_string());
    }
    // Environment variable: colon-separated.
    if let Some(var_name) = env_var
        && let Ok(value) = std::env::var(var_name)
    {
        for repo in value.split(':') {
            let trimmed = repo.trim();
            if !trimmed.is_empty() {
                repos.push(trimmed.to_string());
            }
        }
    }
    // File: one per line, # comments. If the file is explicitly provided,
    // a read failure is a typed error (fail closed).
    if let Some(path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        repos.push(trimmed.to_string());
                    }
                }
            }
            Err(e) => {
                return Err(AllowlistBuildError::AllowlistFileUnreadable {
                    path: path.to_string_lossy().into_owned(),
                    reason: e.to_string(),
                });
            }
        }
    }
    Ok(FixtureAllowlist::new(repos))
}

/// Backward-compatible allowlist builder that silently ignores file read
/// errors. Prefer [`build_allowlist_from_sources_checked`] for new code.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn build_allowlist_from_sources(
    env_var: Option<&str>,
    file_path: Option<&std::path::Path>,
    cli_repos: &[&str],
) -> FixtureAllowlist {
    build_allowlist_from_sources_checked(env_var, file_path, cli_repos)
        .unwrap_or_else(|_| FixtureAllowlist::empty())
}

/// Whether a single `owner` or `repo` segment uses valid GitHub characters.
fn is_valid_github_segment(segment: &str) -> bool {
    if segment.is_empty() || segment.len() > 100 {
        return false;
    }
    segment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Plan for a GitHub fixture mutation: what will be created.
///
/// This is a pure plan checked by tests; the orchestration layer prints it
/// before any mutation and the CLI exposes a `--dry-run` flag that stops here.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureMutationPlan {
    /// The target `owner/repo`.
    pub repository: String,
    /// The issue title (includes the run ID).
    pub issue_title: String,
    /// The branch name (includes the run ID).
    pub branch_name: String,
    /// The PR title (includes the run ID).
    pub pr_title: String,
    /// Whether the merge will be attempted.
    pub merge: bool,
}

/// Build a fixture mutation plan for a given repository and run ID.
///
/// The run ID is embedded in issue titles, branch names, and PR titles so
/// resources created by a run are uniquely identifiable for cleanup.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[must_use]
pub fn build_mutation_plan(repository: &str, run_id: &str, merge: bool) -> FixtureMutationPlan {
    FixtureMutationPlan {
        repository: normalize_repo(repository),
        issue_title: format!("[tutorial-capture:{run_id}] fixture issue for documentation capture"),
        branch_name: format!("tutorial-capture/{run_id}"),
        pr_title: format!("[tutorial-capture:{run_id}] fixture pull request"),
        merge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Production repo refusal ───────────────────────────────────────────

    #[test]
    fn production_repo_vybestack_jefe_is_always_refused() {
        let allowlist = FixtureAllowlist::new(["vybestack/jefe"]);
        let decision = allowlist.evaluate("vybestack/jefe");
        assert!(matches!(
            decision,
            AllowlistDecision::ProductionRepoRefused { .. }
        ));
    }

    #[test]
    fn production_repo_llxprt_jefe_is_always_refused() {
        let allowlist = FixtureAllowlist::new(["vybestack/llxprt-jefe"]);
        let decision = allowlist.evaluate("vybestack/llxprt-jefe");
        assert!(matches!(
            decision,
            AllowlistDecision::ProductionRepoRefused { .. }
        ));
    }

    #[test]
    fn production_repo_refusal_is_case_insensitive() {
        let allowlist = FixtureAllowlist::new(["fixture/repo"]);
        let decision = allowlist.evaluate("VYBESTACK/JEFE");
        assert!(matches!(
            decision,
            AllowlistDecision::ProductionRepoRefused { .. }
        ));
    }

    #[test]
    fn production_repo_refusal_overrides_allowlist() {
        let allowlist = FixtureAllowlist::new(["vybestack/jefe"]);
        assert!(!allowlist.is_allowed("vybestack/jefe"));
    }

    #[test]
    fn production_repo_refused_even_if_explicitly_added() {
        // Even if someone mistakenly adds the production repo to the allowlist,
        // it must still be refused.
        let allowlist = FixtureAllowlist::new(["vybestack/jefe", "fixture/test"]);
        assert!(allowlist.is_allowed("fixture/test"));
        assert!(!allowlist.is_allowed("vybestack/jefe"));
    }

    // ── Allowlist evaluation ─────────────────────────────────────────────

    #[test]
    fn allowlisted_repo_is_allowed() {
        let allowlist = FixtureAllowlist::new(["fixture/test-repo"]);
        let decision = allowlist.evaluate("fixture/test-repo");
        assert!(decision.is_allowed());
    }

    #[test]
    fn non_allowlisted_repo_is_refused() {
        let allowlist = FixtureAllowlist::new(["fixture/test-repo"]);
        let decision = allowlist.evaluate("other/repo");
        assert!(matches!(decision, AllowlistDecision::NotAllowlisted { .. }));
    }

    #[test]
    fn empty_allowlist_refuses_everything() {
        let allowlist = FixtureAllowlist::new::<[&str; 0], &str>([]);
        assert!(!allowlist.is_allowed("fixture/test"));
    }

    #[test]
    fn allowlist_is_case_insensitive() {
        let allowlist = FixtureAllowlist::new(["Fixture/TestRepo"]);
        assert!(allowlist.is_allowed("fixture/testrepo"));
        assert!(allowlist.is_allowed("FIXTURE/TESTREPO"));
    }

    #[test]
    fn allowlist_trims_whitespace() {
        let allowlist = FixtureAllowlist::new(["  fixture/test  "]);
        assert!(allowlist.is_allowed("fixture/test"));
    }

    #[test]
    fn decision_reason_is_human_readable() {
        let allowlist = FixtureAllowlist::new::<[&str; 0], &str>([]);
        let decision = allowlist.evaluate("random/repo");
        assert!(decision.reason().contains("not in the fixture allowlist"));
    }

    #[test]
    fn production_refusal_reason_mentions_production() {
        let allowlist = FixtureAllowlist::new::<[&str; 0], &str>([]);
        let decision = allowlist.evaluate("vybestack/jefe");
        assert!(decision.reason().contains("production repository"));
    }

    // ── Repo format validation ───────────────────────────────────────────

    #[test]
    fn valid_repo_format_accepted() {
        assert!(is_valid_repo_format("owner/repo"));
        assert!(is_valid_repo_format("owner-name/repo-name"));
        assert!(is_valid_repo_format("owner_name/repo.name"));
    }

    #[test]
    fn invalid_repo_format_rejected() {
        assert!(!is_valid_repo_format("owner"));
        assert!(!is_valid_repo_format("/repo"));
        assert!(!is_valid_repo_format("owner/"));
        assert!(!is_valid_repo_format(""));
        assert!(!is_valid_repo_format("owner/repo/extra"));
    }

    #[test]
    fn repo_format_rejects_spaces() {
        assert!(!is_valid_repo_format("owner /repo"));
    }

    // ── build_allowlist_from_sources_checked ──────────────────────────────

    #[test]
    fn checked_builder_returns_typed_error_for_unreadable_file() {
        let result = build_allowlist_from_sources_checked(
            None,
            Some(std::path::Path::new("/nonexistent/allowlist.txt")),
            &[],
        );
        assert!(
            matches!(
                result,
                Err(AllowlistBuildError::AllowlistFileUnreadable { .. })
            ),
            "unreadable allowlist file must return typed error: {result:?}"
        );
    }

    #[test]
    fn checked_builder_succeeds_when_file_exists() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
        let path = dir.path().join("allowlist.txt");
        std::fs::write(&path, "fixture/test\n# comment\n")
            .unwrap_or_else(|e| panic!("write allowlist: {e}"));
        let result = build_allowlist_from_sources_checked(None, Some(&path), &[]);
        assert!(result.is_ok(), "readable file should succeed");
        let al = result.unwrap_or_else(|e| panic!("checked ok: {e:?}"));
        assert!(al.is_allowed("fixture/test"));
    }

    #[test]
    fn checked_builder_succeeds_without_file() {
        let result = build_allowlist_from_sources_checked(None, None, &["fixture/from-cli"]);
        assert!(result.is_ok());
        let al = result.unwrap_or_else(|e| panic!("checked ok: {e:?}"));
        assert!(al.is_allowed("fixture/from-cli"));
    }

    // ── Mutation plan ─────────────────────────────────────────────────────

    #[test]
    fn mutation_plan_embeds_run_id() {
        let plan = build_mutation_plan("fixture/test", "run-001", false);
        assert!(plan.issue_title.contains("run-001"));
        assert!(plan.branch_name.contains("run-001"));
        assert!(plan.pr_title.contains("run-001"));
    }

    #[test]
    fn mutation_plan_branch_name_uses_safe_format() {
        let plan = build_mutation_plan("fixture/test", "run-001", false);
        assert!(plan.branch_name.starts_with("tutorial-capture/"));
        assert!(!plan.branch_name.contains(' '));
    }

    #[test]
    fn mutation_plan_normalizes_repository() {
        let plan = build_mutation_plan("  Fixture/Test  ", "run-001", false);
        assert_eq!(plan.repository, "fixture/test");
    }

    #[test]
    fn mutation_plan_merge_flag_preserved() {
        let plan_with_merge = build_mutation_plan("fixture/test", "run-001", true);
        let plan_without_merge = build_mutation_plan("fixture/test", "run-001", false);
        assert!(plan_with_merge.merge);
        assert!(!plan_without_merge.merge);
    }
}
