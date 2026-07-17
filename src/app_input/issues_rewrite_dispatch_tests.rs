//! Tests for the issue-rewrite orchestration resolver (issue #214).
//!
//! These exercise the pure `resolve_rewrite_context_from_state` resolver — the
//! decision logic that turns the NewIssue composer draft + focused repository
//! into the agent instruction + launch signature. The live subprocess run is
//! boundary I/O and is not unit-tested here.

use super::resolve_rewrite_context_from_state;
use jefe::domain::{AgentKind, Repository, RepositoryId};
use jefe::state::AppEvent;
use jefe::state::AppState;
use jefe::state::{ComposerTarget, InlineState};
use std::path::PathBuf;

fn base_state() -> AppState {
    let mut state = AppState::default();
    let mut repo = Repository::new(
        RepositoryId("repo-1".to_string()),
        "Test Repo".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/test"),
    );
    repo.default_agent_kind = AgentKind::Llxprt;
    repo.github_repo = "owner/repo".to_string();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode)
}

fn with_new_issue_draft(state: AppState, text: &str) -> AppState {
    let mut state = state;
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: text.to_string(),
        cursor: text.len(),
    };
    state
}

/// Unwrap the resolver's `Result` (a precondition failure is unexpected in
/// these tests) so the caller asserts directly on the `Option`.
fn resolved(state: &AppState) -> Option<super::RewriteContext> {
    match resolve_rewrite_context_from_state(state) {
        Ok(opt) => opt,
        Err(error) => panic!("resolver precondition should not fail in tests: {error}"),
    }
}

#[test]
fn resolves_none_without_composer() {
    let state = base_state();
    assert!(resolved(&state).is_none());
}

#[test]
fn resolves_none_for_new_comment_composer() {
    let mut state = with_new_issue_draft(base_state(), "draft");
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    assert!(resolved(&state).is_none());
}

#[test]
fn resolves_none_for_empty_draft() {
    let state = with_new_issue_draft(base_state(), "   ");
    assert!(resolved(&state).is_none());
}

#[test]
fn resolves_none_when_rewrite_already_pending() {
    let mut state = with_new_issue_draft(base_state(), "fix the bug");
    state.issues_state.rewrite_pending = true;
    assert!(resolved(&state).is_none());
}

#[test]
fn resolves_none_without_selected_repo() {
    let mut state = with_new_issue_draft(base_state(), "fix the bug");
    state.selected_repository_index = None;
    assert!(resolved(&state).is_none());
}

#[test]
fn resolves_instruction_and_signature_from_draft_and_repo() {
    let state = with_new_issue_draft(base_state(), "fix the bug\nsome details");
    let ctx = resolved(&state).unwrap_or_else(|| {
        panic!("a NewIssue draft with a selected repo must resolve a rewrite context")
    });
    assert!(
        ctx.instruction.contains("fix the bug"),
        "instruction must embed the draft: {}",
        ctx.instruction
    );
    assert!(
        ctx.instruction.contains("owner/repo"),
        "instruction must reference the github repo: {}",
        ctx.instruction
    );
    assert_eq!(ctx.signature.agent_kind, AgentKind::Llxprt);
}

#[test]
fn resolves_signature_for_code_puppy_default() {
    let mut state = with_new_issue_draft(base_state(), "draft");
    state.repositories[0].default_agent_kind = AgentKind::CodePuppy;
    let ctx = resolved(&state)
        .unwrap_or_else(|| panic!("Code Puppy default repo must resolve a rewrite context"));
    assert_eq!(ctx.signature.agent_kind, AgentKind::CodePuppy);
}
