//! Repository-form validation tests for GitHub `owner/repo` fields and SSH
//! transport fields.
//!
//! Extracted from `form_ops_tests.rs` to keep that file under the source-file
//! size hard limit. Covers the `github_repo` and `github_issue_pr_repo`
//! validation rules (issue #266) as well as SSH transport field preservation
//! for remote repositories.

use super::*;
use crate::domain::Repository;

fn seed_repository() -> Repository {
    Repository {
        id: RepositoryId("repo-1".to_owned()),
        name: "Repo 1".to_owned(),
        slug: "repo-1".to_owned(),
        base_dir: std::path::PathBuf::from("/tmp/repo-1"),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_llxprt_version: String::new(),
        github_issue_pr_repo: String::new(),
        github_repo: String::new(),
        remote: crate::domain::RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        agent_ids: Vec::new(),
    }
}

// ── Issue #266: github_issue_pr_repo form validation ────────────────────

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
        remote_enabled: false,
        login_user: String::new(),

        host: String::new(),
        run_as_user: String::new(),
        setup_env_default: false,
        ..RepositoryFormFields::default()
    }
}

#[test]
fn remote_repository_form_preserves_validated_ssh_transport_fields() {
    let fields = RepositoryFormFields {
        name: "Remote SSH".to_owned(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        ssh_port: "2222".to_owned(),
        identity_file: r"C:\Keys Ω\agent key".to_owned(),
        ssh_options: "Compression=yes LogLevel=ERROR".to_owned(),
        ..RepositoryFormFields::default()
    };
    let Some(repository) = AppState::create_repository_from_fields(&fields) else {
        panic!("valid SSH fields should create a repository");
    };
    assert_eq!(repository.remote.port, Some(2222));
    assert_eq!(
        repository.remote.identity_file,
        std::path::PathBuf::from(r"C:\Keys Ω\agent key")
    );
    assert_eq!(
        repository.remote.options,
        vec!["Compression=yes", "LogLevel=ERROR"]
    );
}

#[test]
fn remote_repository_form_rejects_invalid_port_and_unsafe_option() {
    let mut fields = RepositoryFormFields {
        name: "Remote SSH".to_owned(),
        remote_enabled: true,
        login_user: "ubuntu".to_owned(),
        host: "linux.example".to_owned(),
        ssh_port: "not-a-port".to_owned(),
        ..RepositoryFormFields::default()
    };
    assert!(AppState::create_repository_from_fields(&fields).is_none());

    fields.ssh_port = "22".to_owned();
    fields.ssh_options = "ProxyCommand=credential-helper".to_owned();
    assert!(AppState::create_repository_from_fields(&fields).is_none());
}

#[test]
fn local_repository_ignores_stale_invalid_ssh_port() {
    let fields = RepositoryFormFields {
        name: "Local Repository".to_owned(),
        remote_enabled: false,
        ssh_port: "not-a-port".to_owned(),
        ..RepositoryFormFields::default()
    };
    let Some(repository) = AppState::create_repository_from_fields(&fields) else {
        panic!("disabled remote settings must not block a local repository");
    };
    assert_eq!(repository.remote.port, None);
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
