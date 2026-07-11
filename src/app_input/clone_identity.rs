//! Validated GitHub clone identity for issue-driven agent launches.
//!
//! Clone identity derives **only** from a valid `Repository.github_repo`
//! `owner/repo` value — never from `Repository.slug`. The slug may be a
//! display name or an arbitrary local identifier and is not a safe clone
//! target. The canonical HTTPS clone URL is used regardless of whether the
//! agent runs locally or remotely; SSH transport is never inferred from
//! `remote.enabled` (issue #184).
//!
//! The validation here is intentionally strict: it must reject URLs, paths
//! with extra components, internal whitespace, option-like malformed values,
//! and anything that could be misinterpreted by `git clone`. This is a pure
//! module so it can be exhaustively unit-tested without spawning git.

/// A validated GitHub `owner/repo` identity safe to build a clone URL from.
///
/// Constructed via [`CloneIdentity::parse`], which performs all validation.
/// Once a `CloneIdentity` exists, its [`clone_url`] is the canonical HTTPS
/// form and is safe to pass to `git clone`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CloneIdentity {
    /// Validated `owner/repo` (no surrounding whitespace, exactly two
    /// nonempty path components).
    owner_repo: String,
}

impl CloneIdentity {
    /// Parse a raw `github_repo` string into a validated clone identity.
    ///
    /// Validation rules (issue #184):
    /// - Trim outer whitespace.
    /// - Exactly two nonempty path components separated by a single `/`.
    /// - Reject URLs (`http://`, `https://`, `git@`, `ssh://`).
    /// - Reject values with internal whitespace.
    /// - Reject option-like values (components starting with `-`).
    /// - Reject values with additional `/` separators (more than two
    ///   components).
    ///
    /// Returns `None` when any rule is violated or the input is empty after
    /// trimming. This never falls back to any other source (e.g. `slug`).
    #[must_use]
    pub(super) fn parse(raw_github_repo: &str) -> Option<Self> {
        let trimmed = raw_github_repo.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Reject anything that looks like a URL or SSH clone string. Only the
        // bare `owner/repo` form is accepted.
        if is_url_or_ssh_form(trimmed) {
            return None;
        }
        // Reject internal whitespace (a valid owner/repo has none).
        if trimmed.chars().any(char::is_whitespace) {
            return None;
        }
        let components: Vec<&str> = trimmed.split('/').collect();
        if components.len() != 2 {
            return None;
        }
        let [owner, repo] = [components[0], components[1]];
        // Both components must be nonempty (rejects `owner/` and `/repo`).
        if owner.is_empty() || repo.is_empty() {
            return None;
        }
        // Reject option-like malformed components (option-injection guard).
        if owner.starts_with('-') || repo.starts_with('-') {
            return None;
        }
        // Enforce valid GitHub component characters: alphanumerics, hyphens,
        // underscores, and dots. This rejects `@` and other shell/URL
        // metacharacters that have no place in a GitHub owner/repo name.
        // Uses the shared `domain::is_valid_github_component`.
        if !jefe::domain::is_valid_github_component(owner)
            || !jefe::domain::is_valid_github_component(repo)
        {
            return None;
        }
        Some(Self {
            owner_repo: trimmed.to_owned(),
        })
    }

    /// Resolve the clone identity from a repository definition.
    ///
    /// Uses **only** `Repository.github_repo`. Never falls back to `slug`.
    /// Returns `None` when `github_repo` is absent or invalid.
    #[must_use]
    pub(super) fn from_repository(repo: &jefe::domain::Repository) -> Option<Self> {
        Self::parse(&repo.github_repo)
    }

    /// Build the canonical HTTPS clone URL.
    ///
    /// Always HTTPS regardless of local/remote execution (issue #184).
    #[must_use]
    pub(super) fn clone_url(&self) -> String {
        format!("https://github.com/{}.git", self.owner_repo)
    }

    /// The validated `owner/repo` string.
    #[cfg(test)]
    #[must_use]
    pub(super) fn owner_repo(&self) -> &str {
        &self.owner_repo
    }
}

/// Detect whether `value` is a URL or SSH clone string rather than a bare
/// `owner/repo`.
fn is_url_or_ssh_form(value: &str) -> bool {
    value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("git@")
        || value.starts_with("ssh://")
        || value.starts_with("git://")
}

#[cfg(test)]
mod tests {
    use super::*;

    trait TestOptionExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T> TestOptionExt<T> for Option<T> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Some(value) => value,
                None => panic!("{context}: expected Some, got None"),
            }
        }
    }

    // ── Valid identities ───────────────────────────────────────────────

    #[test]
    fn parses_valid_owner_repo() {
        let id = CloneIdentity::parse("acme/widgets").value_or_panic("parse acme/widgets");
        assert_eq!(id.owner_repo(), "acme/widgets");
        assert_eq!(id.clone_url(), "https://github.com/acme/widgets.git");
    }

    #[test]
    fn trims_outer_whitespace() {
        let id = CloneIdentity::parse("  acme/widgets  ").value_or_panic("parse trimmed");
        assert_eq!(id.owner_repo(), "acme/widgets");
    }

    #[test]
    fn accepts_dashes_and_dots_in_components() {
        let id = CloneIdentity::parse("my-org/my.repo-name").value_or_panic("parse my-org");
        assert_eq!(id.owner_repo(), "my-org/my.repo-name");
    }

    // ── Invalid identities ─────────────────────────────────────────────

    #[test]
    fn rejects_empty() {
        assert!(CloneIdentity::parse("").is_none());
        assert!(CloneIdentity::parse("   ").is_none());
    }

    #[test]
    fn rejects_https_url() {
        assert!(CloneIdentity::parse("https://github.com/acme/widgets").is_none());
    }

    #[test]
    fn rejects_http_url() {
        assert!(CloneIdentity::parse("http://github.com/acme/widgets").is_none());
    }

    #[test]
    fn rejects_ssh_form() {
        assert!(CloneIdentity::parse("git@github.com:acme/widgets.git").is_none());
        assert!(CloneIdentity::parse("ssh://git@github.com/acme/widgets").is_none());
        assert!(CloneIdentity::parse("git://github.com/acme/widgets").is_none());
    }

    #[test]
    fn rejects_internal_whitespace() {
        assert!(CloneIdentity::parse("ac me/widgets").is_none());
        assert!(CloneIdentity::parse("acme/wid gets").is_none());
        assert!(CloneIdentity::parse("acme\t/widgets").is_none());
    }

    #[test]
    fn rejects_single_component() {
        assert!(CloneIdentity::parse("justrepo").is_none());
    }

    #[test]
    fn rejects_three_components() {
        assert!(CloneIdentity::parse("acme/widgets/extra").is_none());
    }

    #[test]
    fn rejects_missing_owner() {
        assert!(CloneIdentity::parse("/widgets").is_none());
    }

    #[test]
    fn rejects_missing_repo() {
        assert!(CloneIdentity::parse("acme/").is_none());
    }

    #[test]
    fn rejects_option_like_owner() {
        assert!(CloneIdentity::parse("--upload-pack/x").is_none());
    }

    #[test]
    fn rejects_option_like_repo() {
        assert!(CloneIdentity::parse("acme/--config").is_none());
    }

    #[test]
    fn rejects_at_sign_in_component() {
        // `@` is not valid in GitHub owner/repo names.
        assert!(CloneIdentity::parse("acme@org/widgets").is_none());
        assert!(CloneIdentity::parse("acme/wid@gets").is_none());
    }

    #[test]
    fn rejects_shell_metacharacters_in_component() {
        // Shell/URL metacharacters are not valid GitHub name characters.
        assert!(CloneIdentity::parse("ac me/widgets").is_none());
        assert!(CloneIdentity::parse("acme/wid;gets").is_none());
        assert!(CloneIdentity::parse("acme/$(whoami)").is_none());
    }

    #[test]
    fn always_uses_https_clone_url() {
        let id = CloneIdentity::parse("acme/widgets").value_or_panic("parse for https check");
        assert!(id.clone_url().starts_with("https://"));
        assert!(!id.clone_url().contains("git@"));
    }

    // ── from_repository: no fallback to slug ───────────────────────────

    fn repo_with(github_repo: &str, slug: &str) -> jefe::domain::Repository {
        let mut repo = jefe::domain::Repository::new(
            jefe::domain::RepositoryId("r1".to_owned()),
            "Repo".to_owned(),
            slug.to_owned(),
            std::path::PathBuf::from("/tmp/repo"),
        );
        repo.github_repo = github_repo.to_owned();
        repo
    }

    #[test]
    fn from_repository_uses_github_repo_not_slug() {
        let repo = repo_with("owner/repo", "some-local-slug");
        let id = CloneIdentity::from_repository(&repo).value_or_panic("from_repository");
        assert_eq!(id.owner_repo(), "owner/repo");
        assert_eq!(id.clone_url(), "https://github.com/owner/repo.git");
    }

    #[test]
    fn from_repository_returns_none_when_github_repo_empty() {
        // slug is set but MUST NOT be used.
        let repo = repo_with("", "owner/repo");
        assert!(CloneIdentity::from_repository(&repo).is_none());
    }

    #[test]
    fn from_repository_returns_none_when_github_repo_invalid() {
        // A URL-shaped github_repo must not yield an identity even though the
        // slug looks valid.
        let repo = repo_with("https://github.com/owner/repo", "owner/repo");
        assert!(CloneIdentity::from_repository(&repo).is_none());
    }

    #[test]
    fn from_repository_returns_none_when_slug_only() {
        // slug is "owner/repo" but github_repo is empty: no identity.
        let mut repo = jefe::domain::Repository::new(
            jefe::domain::RepositoryId("r1".to_owned()),
            "Repo".to_owned(),
            "owner/repo".to_owned(),
            std::path::PathBuf::from("/tmp/repo"),
        );
        repo.github_repo = String::new();
        assert!(CloneIdentity::from_repository(&repo).is_none());
    }
}
