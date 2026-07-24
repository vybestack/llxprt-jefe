//! Tests for issue #266: `github_issue_pr_repo` form validation on the
//! New/Edit Repository form (create + update paths).

use super::*;
use crate::domain::{Repository, RepositoryId};

fn seed_repository() -> Repository {
    Repository::new(
        RepositoryId("repo-1".to_owned()),
        "Repo 1".to_owned(),
        "repo-1".to_owned(),
        std::path::PathBuf::from("/tmp/repo-1"),
    )
}

fn repository_or_panic(repository: Option<Repository>, context: &str) -> Repository {
    match repository {
        Some(repository) => repository,
        None => panic!("{context}"),
    }
}

fn issue266_valid_fields() -> RepositoryFormFields {
    RepositoryFormFields {
        name: "Repo".to_owned(),
        base_dir: String::new(),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_agent_kind: "LLxprt".to_owned(),
        github_repo: "owner/repo".to_owned(),
        github_issue_pr_repo: String::new(),
        remote_enabled: false,
        login_user: String::new(),

        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    }
}

/// A blank `github_issue_pr_repo` is accepted (preserves existing behavior).
#[test]
fn create_repository_accepts_blank_issue_pr_repo() {
    let fields = issue266_valid_fields();
    let repo = repository_or_panic(
        AppState::create_repository_from_fields(&fields),
        "blank issue_pr_repo must be accepted",
    );
    assert!(repo.github_issue_pr_repo.is_empty());
}

/// A valid `owner/repo` override is accepted and persisted.
#[test]
fn create_repository_accepts_well_formed_issue_pr_repo() {
    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "vybestack/llxprt-jefe".to_owned();
    let repo = repository_or_panic(
        AppState::create_repository_from_fields(&fields),
        "valid issue_pr_repo must be accepted",
    );
    assert_eq!(repo.github_issue_pr_repo, "vybestack/llxprt-jefe");
}

/// Surrounding whitespace is trimmed on save.
#[test]
fn create_repository_trims_issue_pr_repo_whitespace() {
    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "  vybestack/llxprt-jefe  ".to_owned();
    let repo = repository_or_panic(
        AppState::create_repository_from_fields(&fields),
        "trimmed issue_pr_repo must be accepted",
    );
    assert_eq!(repo.github_issue_pr_repo, "vybestack/llxprt-jefe");
}

/// A malformed override (missing slash) is rejected visibly (returns None).
#[test]
fn create_repository_rejects_malformed_issue_pr_repo_no_slash() {
    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "noslash".to_owned();
    assert!(
        AppState::create_repository_from_fields(&fields).is_none(),
        "malformed issue_pr_repo must be rejected"
    );
}

/// A URL-shaped override is rejected.
#[test]
fn create_repository_rejects_url_shaped_issue_pr_repo() {
    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "https://github.com/a/b".to_owned();
    assert!(
        AppState::create_repository_from_fields(&fields).is_none(),
        "URL-shaped issue_pr_repo must be rejected"
    );
}

/// An override with too many components is rejected.
#[test]
fn create_repository_rejects_issue_pr_repo_with_extra_slash() {
    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "a/b/c".to_owned();
    assert!(
        AppState::create_repository_from_fields(&fields).is_none(),
        "issue_pr_repo with extra slash must be rejected"
    );
}

/// Updating a repository with a valid override persists it.
#[test]
fn update_repository_persists_valid_issue_pr_repo() {
    let mut repo = seed_repository();
    repo.github_repo = "owner/existing".to_owned();
    repo.github_issue_pr_repo = String::new();

    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "upstream/tracker".to_owned();
    assert!(
        AppState::update_repository_from_fields(&mut repo, &fields),
        "valid issue_pr_repo must be accepted on update"
    );
    assert_eq!(repo.github_issue_pr_repo, "upstream/tracker");
}

/// Updating with a malformed override keeps the existing value (visible reject).
#[test]
fn update_repository_rejects_malformed_issue_pr_repo_keeping_existing() {
    let mut repo = seed_repository();
    repo.github_repo = "owner/existing".to_owned();
    repo.github_issue_pr_repo = "upstream/existing".to_owned();

    let mut fields = issue266_valid_fields();
    fields.github_issue_pr_repo = "not-valid".to_owned();
    assert!(
        !AppState::update_repository_from_fields(&mut repo, &fields),
        "malformed issue_pr_repo must reject update"
    );
    assert_eq!(
        repo.github_issue_pr_repo, "upstream/existing",
        "existing override must be preserved on rejected update"
    );
}
