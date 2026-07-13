//! Issue #266: configurable Issues / PRs Repo override.
//!
//! These tests prove that a fork can source all issue/PR reads and mutations
//! from an upstream repository via the `github_issue_pr_repo` override, while
//! cloning, origin checks, display, and GitHub Actions continue to use the
//! working/fork identity (`github_repo`).
//!
//! Coverage areas:
//! - **Resolver wiring**: `resolve_gh_repo` / `resolve_pr_gh_repo` honor the
//!   override and fall back to `github_repo` when blank. A malformed nonblank
//!   override fails visibly (never silently falls back).
//! - **Payload identity**: the issue/PR send payload's `repository` field
//!   carries the effective upstream `owner/repo`, not the fork slug.
//! - **Self-assignment decoupling**: issue self-assignment resolves its
//!   `owner`/`repo` from the effective tracker target, not the clone identity.
//! - **Actions regression**: Actions orchestration continues to use
//!   `github_repo` (the fork), NOT the override.

use super::issue_self_assignment::SelfAssignment;
use super::issues_dispatch::resolve_gh_repo;
use super::issues_send::issue_send_info_from_state;
use super::prs_dispatch::resolve_pr_gh_repo;
use super::tests::{TestOptionExt, sample_agent};
use super::tracker_resolver::resolve_tracker_for_repo;
use jefe::domain::{AgentId, IssueDetail, IssueState, Repository, RepositoryId};
use jefe::state::{AgentChooserState, AppState, IssuesState, ScreenMode};
use std::path::PathBuf;

fn tracker_target_or_panic(
    result: Result<
        Option<super::tracker_resolver::TrackerTarget>,
        super::tracker_resolver::ResolveTrackerError,
    >,
    context: &str,
) -> super::tracker_resolver::TrackerTarget {
    match result {
        Ok(Some(target)) => target,
        Ok(None) => panic!("{context}: expected target, got none"),
        Err(error) => panic!("{context}: {error}"),
    }
}

fn repository_from_json_or_panic(json: &str) -> Repository {
    match serde_json::from_str(json) {
        Ok(repository) => repository,
        Err(error) => panic!("old schema-v1 data must deserialize: {error}"),
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────

fn fork_repo() -> Repository {
    let mut repo = Repository::new(
        RepositoryId("repo-1".to_owned()),
        "Fork".to_owned(),
        "acme/llxprt-jefe".to_owned(),
        PathBuf::from("/tmp/fork"),
    );
    repo.github_repo = "acme/llxprt-jefe".to_owned();
    repo
}

fn fork_with_upstream_override() -> Repository {
    let mut repo = fork_repo();
    repo.github_issue_pr_repo = "vybestack/llxprt-jefe".to_owned();
    repo
}

fn app_state_with_repo(repo: Repository) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state
}

// ── Resolver wiring: resolve_gh_repo honors the override ────────────────

/// A nonblank `github_issue_pr_repo` override must cause `resolve_gh_repo`
/// to return the upstream `owner/repo`, not the fork `github_repo`.
#[test]
fn resolve_gh_repo_honors_upstream_override() {
    let state = app_state_with_repo(fork_with_upstream_override());
    let (owner, repo) = resolve_gh_repo(&state);
    assert_eq!(owner, "vybestack");
    assert_eq!(repo, "llxprt-jefe");
}

/// A blank override falls back to `github_repo` (the fork identity).
#[test]
fn resolve_gh_repo_falls_back_to_github_repo_when_override_blank() {
    let state = app_state_with_repo(fork_repo());
    let (owner, repo) = resolve_gh_repo(&state);
    assert_eq!(owner, "acme");
    assert_eq!(repo, "llxprt-jefe");
}

/// A malformed nonblank override must NOT silently fall back to `github_repo`.
/// It must yield empty strings (visible failure) rather than the fork identity.
#[test]
fn resolve_gh_repo_malformed_override_does_not_silently_use_fallback() {
    let mut repo = fork_repo();
    repo.github_issue_pr_repo = "not-a-valid-repo".to_owned();
    let state = app_state_with_repo(repo);
    let (owner, repo_name) = resolve_gh_repo(&state);
    assert!(
        owner.is_empty() && repo_name.is_empty(),
        "malformed override must fail visibly (empty), not silently use fork: {owner}/{repo_name}"
    );
}

// ── Resolver wiring: resolve_pr_gh_repo honors the override ─────────────

/// A nonblank `github_issue_pr_repo` override must cause `resolve_pr_gh_repo`
/// to return the upstream `owner/repo`, not the fork `github_repo`.
#[test]
fn resolve_pr_gh_repo_honors_upstream_override() {
    let state = app_state_with_repo(fork_with_upstream_override());
    let (owner, repo) = resolve_pr_gh_repo(&state);
    assert_eq!(owner, "vybestack");
    assert_eq!(repo, "llxprt-jefe");
}

/// A blank override falls back to `github_repo` (the fork identity) for PRs.
#[test]
fn resolve_pr_gh_repo_falls_back_to_github_repo_when_override_blank() {
    let state = app_state_with_repo(fork_repo());
    let (owner, repo) = resolve_pr_gh_repo(&state);
    assert_eq!(owner, "acme");
    assert_eq!(repo, "llxprt-jefe");
}

/// A malformed nonblank override must NOT silently fall back for PRs either.
#[test]
fn resolve_pr_gh_repo_malformed_override_does_not_silently_use_fallback() {
    let mut repo = fork_repo();
    repo.github_issue_pr_repo = "https://github.com/a/b".to_owned();
    let state = app_state_with_repo(repo);
    let (owner, repo_name) = resolve_pr_gh_repo(&state);
    assert!(
        owner.is_empty() && repo_name.is_empty(),
        "malformed override must fail visibly for PRs: {owner}/{repo_name}"
    );
}

// ── Domain resolver: effective_issue_pr_repo ────────────────────────────

/// The domain resolver returns the override when present and valid.
#[test]
fn effective_issue_pr_repo_returns_override_when_set() {
    let repo = fork_with_upstream_override();
    let target = tracker_target_or_panic(resolve_tracker_for_repo(&repo), "valid override");
    assert_eq!(target.owner(), "vybestack");
    assert_eq!(target.repo(), "llxprt-jefe");
}

/// The domain resolver falls back to `github_repo` when the override is blank.
#[test]
fn effective_issue_pr_repo_falls_back_when_blank() {
    let repo = fork_repo();
    let target = tracker_target_or_panic(resolve_tracker_for_repo(&repo), "valid fallback");
    assert_eq!(target.owner(), "acme");
    assert_eq!(target.repo(), "llxprt-jefe");
}

/// A malformed override must produce an error, not a silent fallback.
#[test]
fn effective_issue_pr_repo_malformed_override_errors() {
    let mut repo = fork_repo();
    repo.github_issue_pr_repo = "not-valid".to_owned();
    assert!(
        resolve_tracker_for_repo(&repo).is_err(),
        "malformed override must error visibly"
    );
}

// ── Payload identity: issue send payload uses upstream ──────────────────

fn issue_send_state(repo: Repository) -> AppState {
    let agent_id = AgentId("issue-agent".to_owned());
    let mut agent = sample_agent(&agent_id);
    agent.work_dir = PathBuf::from("/tmp/issue-send-test");
    let detail_repo = repo.effective_issue_pr_repo().ok().flatten().map_or_else(
        || repo.github_repo.clone(),
        |target| format!("{}/{}", target.owner(), target.repo()),
    );

    let detail = IssueDetail {
        repo_owner_name: detail_repo,
        number: 266,
        node_id: String::new(),
        title: "Support fork repositories".to_owned(),
        state: IssueState::Open,
        author_login: "reporter".to_owned(),
        created_at: "2024-01-01T00:00:00Z".to_owned(),
        updated_at: "2024-01-02T00:00:00Z".to_owned(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Fork should source issues from upstream".to_owned(),
        external_url: String::new(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    };

    let issues_state = IssuesState {
        active: true,
        issue_detail: Some(detail),
        agent_chooser: Some(AgentChooserState {
            selected_index: 0,
            agents: vec![(agent_id.clone(), "Agent One".to_owned())],
        }),
        ..IssuesState::default()
    };

    let mut state = AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state,
        ..AppState::default()
    };
    state.agents.push(agent);
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state
}

/// The issue send payload's `repository` field must carry the effective
/// upstream `owner/repo`, not the fork slug, when an override is set.
#[test]
fn issue_send_payload_repository_uses_upstream_override() {
    let state = issue_send_state(fork_with_upstream_override());
    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must resolve");
    assert_eq!(
        send_info.payload.repository, "vybestack/llxprt-jefe",
        "payload.repository must be the upstream tracker, not the fork slug"
    );
}

/// The clone identity in the send info must remain the fork (`github_repo`),
/// NOT the upstream override. Clone/prep is decoupled from issue/PR routing.
#[test]
fn issue_send_clone_identity_remains_fork_not_upstream() {
    let state = issue_send_state(fork_with_upstream_override());
    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must resolve");
    let identity = send_info
        .clone_identity
        .as_ref()
        .value_or_panic("fork github_repo must yield a clone identity");
    assert_eq!(
        identity.clone_url(),
        "https://github.com/acme/llxprt-jefe.git",
        "clone identity must remain the fork, not the upstream override"
    );
}

/// When the override is blank, the payload falls back to the fork identity
/// (preserving existing behavior).
#[test]
fn issue_send_payload_repository_falls_back_to_fork_when_blank() {
    let state = issue_send_state(fork_repo());
    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must resolve");
    // When no override, the payload repository is the effective tracker
    // (which is github_repo = the fork).
    assert_eq!(
        send_info.payload.repository, "acme/llxprt-jefe",
        "blank override must fall back to the fork github_repo for payload"
    );
}

// ── Self-assignment decoupled from clone identity ───────────────────────

/// The self-assignment must resolve its `owner`/`repo` from the effective
/// tracker target (upstream), NOT from the clone identity (fork). This proves
/// the decoupling: a fork clones from `acme/llxprt-jefe` but assigns the
/// issue on `vybestack/llxprt-jefe`.
#[test]
fn self_assignment_uses_upstream_tracker_not_clone_identity() {
    let state = issue_send_state(fork_with_upstream_override());
    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must resolve");

    let tracker = jefe::domain::GitHubRepoRef::parse(&send_info.payload.repository)
        .unwrap_or_else(|error| panic!("valid tracker must parse: {error}"))
        .unwrap_or_else(|| panic!("valid tracker must not be blank"));
    let assignment =
        SelfAssignment::from_send_context(Some(&tracker), send_info.payload.issue_number)
            .value_or_panic("must produce a self-assignment");

    // owner/repo come from the effective tracker (upstream), not the clone identity.
    assert_eq!(
        assignment.owner, "vybestack",
        "self-assignment owner must be the upstream tracker, not the fork"
    );
    assert_eq!(
        assignment.repo, "llxprt-jefe",
        "self-assignment repo must be the upstream tracker, not the fork"
    );
    assert_eq!(
        assignment.owner_repo, "vybestack/llxprt-jefe",
        "owner_repo must be the upstream tracker shortform"
    );
}

// ── Actions regression: Actions continue to use github_repo (fork) ──────

#[test]
fn actions_uses_fork_github_repo_not_override() {
    let repo = fork_with_upstream_override();
    let (owner, name) = super::actions_orchestration::actions_repository_target(&repo)
        .unwrap_or_else(|error| panic!("working repository must resolve: {error}"));
    assert_eq!((owner, name), ("acme", "llxprt-jefe"));
}

// ── Schema-v1 compatibility ─────────────────────────────────────────────

/// A repository deserialized from old schema-v1 data (which lacks
/// `github_issue_pr_repo`) must default to an empty string, preserving
/// existing behavior.
#[test]
fn schema_v1_without_override_field_defaults_to_blank() {
    let json = r#"{
        "id": "repo-1",
        "name": "Old Repo",
        "slug": "acme/llxprt-jefe",
        "base_dir": "/tmp/old",
        "default_profile": "",
        "github_repo": "acme/llxprt-jefe",
        "agent_ids": []
    }"#;
    let repo = repository_from_json_or_panic(json);
    assert!(
        repo.github_issue_pr_repo.is_empty(),
        "missing override field must default to blank (schema-v1 compat)"
    );
}
