//! Validated GitHub repository reference (`owner/repo`) for issue/PR tracker
//! routing.
//!
//! A [`GitHubRepoRef`] is a parsed, validated `owner/repo` pair. It is the
//! single resolution path for "which GitHub repository do I read issues/PRs
//! from?" The resolver ([`Repository::effective_issue_pr_repo`]) selects a
//! nonblank override ([`Repository::github_issue_pr_repo`]) when present, and
//! falls back to the working/fork identity ([`Repository::github_repo`])
//! otherwise.
//!
//! This type is intentionally separate from the clone-identity logic: clone
//! identity derives **only** from `github_repo` (the fork the agent clones),
//! while [`GitHubRepoRef`] identifies the **upstream tracker** where
//! issues/PRs live. A fork can source issues from upstream without changing
//! its clone target.
//!
//! This module is in the `domain/` layer and depends on nothing
//! project-internal except the shared [`is_valid_github_component`] predicate,
//! so every layer (state, app-input, persistence, UI) can resolve the
//! effective tracker through one typed contract.

use super::is_valid_github_component;

/// A validated GitHub `owner/repo` reference.
///
/// Constructed via [`GitHubRepoRef::parse`], which performs all validation.
/// Once a `GitHubRepoRef` exists, its [`owner`] and [`repo`] are safe to pass
/// to `gh` API calls (`--repo owner/repo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepoRef {
    owner: String,
    repo: String,
}

/// Parse error for a malformed `owner/repo` override (issue #266).
///
/// A malformed nonblank override must fail visibly — it is never silently
/// mutated to the fallback fork identity. This error carries the original
/// value so the caller can surface it in a user-facing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepoRefError {
    /// The raw value that failed parsing.
    pub raw: String,
    /// Human-readable reason.
    pub reason: GitHubRepoRefErrorReason,
}

/// Categorized reason a `owner/repo` value failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitHubRepoRefErrorReason {
    /// Too many components (more than two path segments).
    TooManyComponents,
    /// Missing the `/` separator entirely.
    MissingSeparator,
    /// Owner component is empty (e.g. `/repo`).
    EmptyOwner,
    /// Repo component is empty (e.g. `owner/`).
    EmptyRepo,
    /// A component contains invalid characters.
    InvalidComponent,
    /// The owner starts with `-` (option-injection risk).
    OptionLike,
    /// The value is a URL or SSH clone string, not a bare `owner/repo`.
    UrlOrSshForm,
    /// The value contains internal whitespace.
    InternalWhitespace,
}

impl GitHubRepoRefErrorReason {
    /// User-facing description of the validation failure.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::TooManyComponents => "expected exactly two components (owner/repo)",
            Self::MissingSeparator => "missing '/' separator",
            Self::EmptyOwner => "owner is empty",
            Self::EmptyRepo => "repo is empty",
            Self::InvalidComponent => {
                "contains invalid characters (only letters, digits, '-', '_', '.' are allowed)"
            }
            Self::OptionLike => "owner starts with '-' (option-like)",
            Self::UrlOrSshForm => "must be a bare owner/repo, not a URL or SSH string",
            Self::InternalWhitespace => "contains internal whitespace",
        }
    }
}

impl std::fmt::Display for GitHubRepoRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid Issues / PRs Repo '{}': {}",
            self.raw,
            self.reason.description()
        )
    }
}

impl std::error::Error for GitHubRepoRefError {}

impl GitHubRepoRef {
    /// Parse a raw `owner/repo` string into a validated reference.
    ///
    /// An empty string (after trimming) is valid and represents "no override"
    /// — the caller should fall back to `github_repo`. A nonblank value must
    /// be exactly `"owner/repo"`: a single forward slash with non-empty parts
    /// on both sides, each containing only valid GitHub name characters.
    ///
    /// # Errors
    ///
    /// Returns [`GitHubRepoRefError`] when the value is nonblank but
    /// malformed (URL form, extra components, invalid characters, etc.).
    pub fn parse(raw: &str) -> Result<Option<Self>, GitHubRepoRefError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if is_url_or_ssh_form(trimmed) {
            return Err(GitHubRepoRefError {
                raw: raw.to_owned(),
                reason: GitHubRepoRefErrorReason::UrlOrSshForm,
            });
        }
        if trimmed.chars().any(char::is_whitespace) {
            return Err(GitHubRepoRefError {
                raw: raw.to_owned(),
                reason: GitHubRepoRefErrorReason::InternalWhitespace,
            });
        }
        match trimmed.split_once('/') {
            None => Err(GitHubRepoRefError {
                raw: raw.to_owned(),
                reason: GitHubRepoRefErrorReason::MissingSeparator,
            }),
            Some((owner, repo)) => {
                if owner.is_empty() {
                    return Err(GitHubRepoRefError {
                        raw: raw.to_owned(),
                        reason: GitHubRepoRefErrorReason::EmptyOwner,
                    });
                }
                if repo.is_empty() {
                    return Err(GitHubRepoRefError {
                        raw: raw.to_owned(),
                        reason: GitHubRepoRefErrorReason::EmptyRepo,
                    });
                }
                if repo.contains('/') {
                    return Err(GitHubRepoRefError {
                        raw: raw.to_owned(),
                        reason: GitHubRepoRefErrorReason::TooManyComponents,
                    });
                }
                if owner.starts_with('-') {
                    return Err(GitHubRepoRefError {
                        raw: raw.to_owned(),
                        reason: GitHubRepoRefErrorReason::OptionLike,
                    });
                }
                if !is_valid_github_component(owner) || !is_valid_github_component(repo) {
                    return Err(GitHubRepoRefError {
                        raw: raw.to_owned(),
                        reason: GitHubRepoRefErrorReason::InvalidComponent,
                    });
                }
                Ok(Some(Self {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                }))
            }
        }
    }

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

    /// The validated `owner/repo` string.
    #[must_use]
    pub fn full(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// Detect whether `value` is a URL or SSH clone string rather than a bare
/// `owner/repo`.
fn is_url_or_ssh_form(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    [
        "http://",
        "https://",
        "git@",
        "ssh://",
        "git://",
        "git+ssh://",
        "git+https://",
        "git+http://",
        "git+file://",
    ]
    .iter()
    .any(|prefix| lowercase.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    trait ResultExt {
        fn some_or_panic(self, context: &str) -> GitHubRepoRef;
        fn error_or_panic(self, context: &str) -> GitHubRepoRefError;
    }

    impl ResultExt for Result<Option<GitHubRepoRef>, GitHubRepoRefError> {
        fn some_or_panic(self, context: &str) -> GitHubRepoRef {
            match self {
                Ok(Some(value)) => value,
                Ok(None) => panic!("{context}: expected Some, got None"),
                Err(e) => panic!("{context}: expected Ok, got Err({e})"),
            }
        }

        fn error_or_panic(self, context: &str) -> GitHubRepoRefError {
            match self {
                Err(error) => error,
                Ok(value) => panic!("{context}: expected Err, got Ok({value:?})"),
            }
        }
    }

    // ── Blank / fallback ────────────────────────────────────────────────

    #[test]
    fn blank_yields_none_preserving_fallback_semantics() {
        assert!(
            GitHubRepoRef::parse("")
                .unwrap_or_else(|e| panic!("blank: {e}"))
                .is_none()
        );
        assert!(
            GitHubRepoRef::parse("   ")
                .unwrap_or_else(|e| panic!("whitespace-only: {e}"))
                .is_none()
        );
    }

    // ── Valid overrides ────────────────────────────────────────────────

    #[test]
    fn parses_valid_owner_repo() {
        let r = GitHubRepoRef::parse("vybestack/llxprt-jefe").some_or_panic("parse upstream");
        assert_eq!(r.owner(), "vybestack");
        assert_eq!(r.repo(), "llxprt-jefe");
        assert_eq!(r.full(), "vybestack/llxprt-jefe");
    }

    #[test]
    fn trims_outer_whitespace() {
        let r = GitHubRepoRef::parse("  vybestack/llxprt-jefe  ").some_or_panic("trim");
        assert_eq!(r.full(), "vybestack/llxprt-jefe");
    }

    #[test]
    fn accepts_dashes_dots_underscores() {
        let r = GitHubRepoRef::parse("my-org/my.repo_name").some_or_panic("dashes/dots");
        assert_eq!(r.owner(), "my-org");
        assert_eq!(r.repo(), "my.repo_name");
    }

    // ── Malformed overrides ────────────────────────────────────────────

    #[test]
    fn rejects_url_form() {
        let err = GitHubRepoRef::parse("https://github.com/a/b")
            .error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::UrlOrSshForm);
    }

    #[test]
    fn rejects_ssh_form() {
        let err = GitHubRepoRef::parse("git@github.com:a/b.git")
            .error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::UrlOrSshForm);
    }

    #[test]
    fn rejects_internal_whitespace() {
        let err = GitHubRepoRef::parse("vybe stack/jefe")
            .error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::InternalWhitespace);
    }

    #[test]
    fn rejects_missing_separator() {
        let err = GitHubRepoRef::parse("noslash").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::MissingSeparator);
    }

    #[test]
    fn rejects_too_many_components() {
        let err = GitHubRepoRef::parse("a/b/c").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::TooManyComponents);
    }

    #[test]
    fn rejects_empty_owner() {
        let err = GitHubRepoRef::parse("/repo").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::EmptyOwner);
    }

    #[test]
    fn rejects_empty_repo() {
        let err = GitHubRepoRef::parse("owner/").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::EmptyRepo);
    }

    #[test]
    fn rejects_option_like_component() {
        let err =
            GitHubRepoRef::parse("--evil/repo").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::OptionLike);
    }

    #[test]
    fn accepts_repository_name_starting_with_hyphen() {
        let reference = GitHubRepoRef::parse("owner/-repo")
            .some_or_panic("repository component may start with a hyphen");
        assert_eq!(reference.owner(), "owner");
        assert_eq!(reference.repo(), "-repo");
    }

    #[test]
    fn rejects_invalid_characters() {
        let err = GitHubRepoRef::parse("a@org/b").error_or_panic("malformed repository must fail");
        assert_eq!(err.reason, GitHubRepoRefErrorReason::InvalidComponent);
    }

    #[test]
    fn error_message_includes_raw_value_and_reason() {
        let err = GitHubRepoRef::parse("a/b/c").error_or_panic("malformed repository must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("a/b/c"),
            "message must include raw value: {msg}"
        );
        assert!(
            msg.contains("two components"),
            "message must include reason: {msg}"
        );
    }
}
