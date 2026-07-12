//! Issue send-to-agent tests: launch-signature overrides (#166), validated
//! clone identity (#184), and the origin-mismatch / dirty-copy confirm modals
//! (#190).
//!
//! Split from `app_input_tests.rs` to keep that file under the 1000-line
//! source-file limit. Shared fixtures (`sample_signature`, `sample_agent`,
//! `TestResultExt`, `TestOptionExt`) come from the sibling `tests` module via
//! `super::tests`.

use super::tests::{TestOptionExt, sample_agent, sample_signature};
use super::*;

use std::path::PathBuf;

use super::issues_send::{issue_send_info_from_state, prepare_issue_launch_signature};
use jefe::domain::{AgentId, IssueDetail, IssueState, RepositoryId};
use jefe::state::{AgentChooserState, ScreenMode};

// ── Issue send-to-agent: default-branch prep + dirty-copy guard (issue #166) ─

/// Build an AppState for the issue agent-chooser send path: an open chooser +
/// issue detail + an agent (with `pass_continue = true`) whose work_dir is a
/// temp dir. Mirrors `state_for_pr_agent_chooser_confirm`.
fn state_for_issue_agent_chooser_send(
    agent_id: &AgentId,
    work_dir: &std::path::Path,
) -> jefe::state::AppState {
    let mut agent = sample_agent(agent_id);
    agent.work_dir = work_dir.to_path_buf();
    // sample_agent uses Agent::new which defaults pass_continue = true.
    assert!(
        agent.pass_continue,
        "test fixture: agent must default to pass_continue = true"
    );

    let detail = IssueDetail {
        repo_owner_name: "owner/repo".to_owned(),
        number: 166,
        title: "Issue send should checkout+pull main".to_owned(),
        state: IssueState::Open,
        author_login: "reporter".to_owned(),
        created_at: "2024-01-01T00:00:00Z".to_owned(),
        updated_at: "2024-01-02T00:00:00Z".to_owned(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Send to Agent".to_owned(),
        external_url: "https://github.com/owner/repo/issues/166".to_owned(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    };

    let issues_state = jefe::state::IssuesState {
        active: true,
        issue_detail: Some(detail),
        agent_chooser: Some(AgentChooserState {
            selected_index: 0,
            agents: vec![(agent_id.clone(), String::from("Agent One"))],
        }),
        ..jefe::state::IssuesState::default()
    };

    let mut state = jefe::state::AppState {
        screen_mode: ScreenMode::DashboardIssues,
        issues_state,
        ..AppState::default()
    };
    state.agents.push(agent);
    state.repositories.push(jefe::domain::Repository::new(
        RepositoryId(String::from("repo-1")),
        String::from("Repo 1"),
        String::from("owner/repo"),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state
}

/// The issue-driven launch path must force `pass_continue = false` on the
/// constructed launch signature, even though the agent's configured
/// `pass_continue` defaults to `true`. This test resolves the send info
/// (which copies the agent's `pass_continue`) and then applies the SAME
/// override `dispatch_agent_chooser_confirm` applies, asserting `--continue`
/// would never reach the agent. The git prep + spawn require a runtime/git
/// repo and are guarded out in unit tests (SharedContext is None).
#[test]
fn issue_send_forces_pass_continue_false_on_launch_signature() {
    let agent_id = AgentId(String::from("issue-agent-1"));
    // This test only exercises pure struct transforms — the work_dir is
    // never materialized on disk — so a static path suffices.
    let work_dir = std::path::PathBuf::from("/tmp/jefe-issue-send-test");
    let state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);

    let send_info = issue_send_info_from_state(&state)
        .unwrap_or_else(|| panic!("issue_send_info must resolve with chooser + detail + agent"));

    // The send info copies the agent's configured pass_continue (true).
    assert!(
        send_info.signature.pass_continue,
        "send info should inherit the agent's pass_continue = true"
    );

    // dispatch_agent_chooser_confirm forces pass_continue = false before launch.
    // This calls the REAL production helper so removing the override would
    // cause this test to fail.
    let launch_sig = prepare_issue_launch_signature(send_info.signature);
    assert!(
        !launch_sig.pass_continue,
        "issue-driven launches must force pass_continue = false"
    );
    let instruction = launch_sig
        .mode_flags
        .iter()
        .find(|arg| arg.contains(".jefe/issue-prompt.md"))
        .value_or_panic("issue launch signature must include an instruction");
    assert!(instruction.contains(".jefe/issue-prompt.md"));
    assert!(instruction.contains("create a dedicated issue branch"));
    assert!(instruction.contains("create a detailed pull request"));
    assert!(instruction.contains("continuing to poll with a bounded delay"));
    assert!(instruction.contains("ordinary reviews, inline threads"));
    assert!(instruction.contains("reply in the corresponding review thread"));
    assert!(instruction.contains("no actionable unresolved review feedback remains"));
}

#[test]
fn code_puppy_issue_send_carries_kind_and_uses_positional_instruction() {
    let agent_id = AgentId(String::from("code-puppy-issue-agent"));
    let work_dir = std::path::PathBuf::from("/tmp/jefe-code-puppy-issue-send");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) else {
        panic!("fixture agent should exist");
    };
    agent.agent_kind = jefe::domain::AgentKind::CodePuppy;

    let send_info = issue_send_info_from_state(&state)
        .unwrap_or_else(|| panic!("CodePuppy issue send info should resolve"));
    assert_eq!(
        send_info.signature.agent_kind,
        jefe::domain::AgentKind::CodePuppy
    );

    let launch_sig = prepare_issue_launch_signature(send_info.signature);
    assert!(!launch_sig.pass_continue);
    assert!(!launch_sig.mode_flags.iter().any(|arg| arg == "-i"));
    assert!(
        launch_sig
            .mode_flags
            .iter()
            .any(|arg| arg.contains(".jefe/issue-prompt.md"))
    );
}

/// The `ConfirmIssueDirtyCopy` modal must resolve to `InputMode::Confirm` so
/// the confirm key handler routes Enter/Esc/n correctly.
#[test]
fn confirm_issue_dirty_copy_modal_routes_to_confirm_input_mode() {
    use jefe::input::input_mode_for_state;

    let state = AppState {
        modal: ModalState::ConfirmIssueDirtyCopy {
            agent_id: AgentId(String::from("a1")),
            work_dir: PathBuf::from("/tmp/x"),
            signature: sample_signature(),
            payload: jefe::github::SendPayload::default(),
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
        },
        ..AppState::default()
    };

    assert_eq!(
        input_mode_for_state(&state),
        jefe::input::InputMode::Confirm,
        "ConfirmIssueDirtyCopy must use InputMode::Confirm"
    );
}

/// The `ConfirmIssueOriginMismatch` modal must resolve to `InputMode::Confirm`
/// so the confirm key handler routes Enter/Esc correctly.
#[test]
fn confirm_issue_origin_mismatch_modal_routes_to_confirm_input_mode() {
    use jefe::input::input_mode_for_state;

    let state = AppState {
        modal: ModalState::ConfirmIssueOriginMismatch {
            agent_id: AgentId(String::from("a1")),
            work_dir: PathBuf::from("/tmp/x"),
            signature: sample_signature(),
            payload: jefe::github::SendPayload::default(),
            actual: String::from("other/repo"),
            expected: String::from("acme/widgets"),
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
        },
        ..AppState::default()
    };

    assert_eq!(
        input_mode_for_state(&state),
        jefe::input::InputMode::Confirm,
        "ConfirmIssueOriginMismatch must use InputMode::Confirm"
    );
}

/// Esc/n on the `ConfirmIssueOriginMismatch` modal dispatch `CloseModal`,
/// which must clear the modal non-destructively (no reclone, no launch).
/// This verifies the default-halt contract: the mismatched workdir is left
/// untouched unless the user explicitly presses Enter.
#[test]
fn close_modal_dismisses_origin_mismatch_non_destructively() {
    use jefe::state::AppEvent;

    // Seed non-modal state so we can prove CloseModal leaves it untouched.
    let seeded = AppState {
        repositories: vec![jefe::domain::Repository::new(
            jefe::domain::RepositoryId("r1".to_owned()),
            "Repo".to_owned(),
            "acme/widgets".to_owned(),
            PathBuf::from("/tmp/repo"),
        )],
        screen_mode: jefe::state::ScreenMode::DashboardIssues,
        ..AppState::default()
    };
    let state = AppState {
        modal: ModalState::ConfirmIssueOriginMismatch {
            agent_id: AgentId(String::from("a1")),
            work_dir: PathBuf::from("/tmp/x"),
            signature: sample_signature(),
            payload: jefe::github::SendPayload::default(),
            actual: String::from("other/repo"),
            expected: String::from("acme/widgets"),
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
        },
        repositories: seeded.repositories.clone(),
        screen_mode: seeded.screen_mode,
        ..AppState::default()
    };

    let next = state.apply(AppEvent::CloseModal);
    assert_eq!(
        next.modal,
        ModalState::None,
        "CloseModal must dismiss ConfirmIssueOriginMismatch without side effects"
    );
    // Non-modal state is preserved: CloseModal is a pure transition that
    // only touches `modal`, never agents/repositories/screen_mode.
    assert_eq!(next.repositories.len(), 1, "repositories must be preserved");
    assert_eq!(
        next.screen_mode,
        jefe::state::ScreenMode::DashboardIssues,
        "screen_mode must be preserved"
    );
    assert!(
        next.agents.is_empty(),
        "agents must be untouched (no launch fired)"
    );
}

// ── Issue #184: validated clone identity in issue-send info ─────────────

/// Issue-send info must carry a valid clone identity ONLY when
/// `github_repo` is a valid `owner/repo`, and NEVER fall back to `slug`.
#[test]
fn issue_send_info_carries_valid_clone_identity_only() {
    let agent_id = AgentId(String::from("issue-valid-id"));
    let work_dir = PathBuf::from("/tmp/jefe-issue-valid-id");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    // Set a valid github_repo.
    state.repositories[0].github_repo = "acme/widgets".to_owned();

    let send_info = issue_send_info_from_state(&state)
        .value_or_panic("issue send info must resolve with a valid github_repo");
    let identity = send_info
        .clone_identity
        .as_ref()
        .value_or_panic("a valid github_repo must yield a clone identity");
    assert_eq!(identity.clone_url(), "https://github.com/acme/widgets.git");
}

/// When `github_repo` is empty but `slug` is set, issue-send info must NOT
/// carry a clone identity (no fallback to slug).
#[test]
fn issue_send_info_no_clone_identity_when_github_repo_empty() {
    let agent_id = AgentId(String::from("issue-no-id"));
    let work_dir = PathBuf::from("/tmp/jefe-issue-no-id");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    // slug is "owner/repo" (set by the fixture) but github_repo is empty.
    state.repositories[0].github_repo = String::new();
    assert!(
        !state.repositories[0].slug.is_empty(),
        "fixture: slug must be non-empty to prove no fallback"
    );

    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must still resolve");
    assert!(
        send_info.clone_identity.is_none(),
        "no clone identity when github_repo is empty (must not fall back to slug)"
    );
}

/// When `github_repo` is a URL (invalid), issue-send info must NOT carry a
/// clone identity, even though slug looks valid.
#[test]
fn issue_send_info_no_clone_identity_for_url_shaped_github_repo() {
    let agent_id = AgentId(String::from("issue-url-id"));
    let work_dir = PathBuf::from("/tmp/jefe-issue-url-id");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    state.repositories[0].github_repo = "https://github.com/acme/widgets".to_owned();

    let send_info =
        issue_send_info_from_state(&state).value_or_panic("issue send info must still resolve");
    assert!(
        send_info.clone_identity.is_none(),
        "URL-shaped github_repo must not yield a clone identity"
    );
}

/// CodePuppy issue send uses the SAME prep path (identical launch signature
/// structure) and a fresh no-resume signature. This proves CodePuppy and
/// LLxprt share identical prep; only the runtime args differ.
#[test]
fn code_puppy_issue_uses_identical_prep_and_fresh_no_resume_signature() {
    let agent_id = AgentId(String::from("cp-identical-prep"));
    let work_dir = PathBuf::from("/tmp/jefe-cp-identical-prep");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    state.repositories[0].github_repo = "acme/widgets".to_owned();
    if let Some(agent) = state.agents.iter_mut().find(|a| a.id == agent_id) {
        agent.agent_kind = jefe::domain::AgentKind::CodePuppy;
    }

    let send_info =
        issue_send_info_from_state(&state).value_or_panic("CodePuppy issue send info must resolve");

    // Same validated clone identity as LLxprt would get (identical prep).
    let identity = send_info
        .clone_identity
        .as_ref()
        .value_or_panic("CodePuppy must carry the same validated clone identity");
    assert_eq!(identity.clone_url(), "https://github.com/acme/widgets.git");

    // Fresh no-resume signature: pass_continue forced off, positional
    // instruction (no -i).
    let launch_sig = prepare_issue_launch_signature(send_info.signature);
    assert!(
        !launch_sig.pass_continue,
        "CodePuppy issue send must be fresh (no resume)"
    );
    assert!(
        !launch_sig.mode_flags.iter().any(|arg| arg == "-i"),
        "CodePuppy must not receive -i (runtime prepends it)"
    );
    assert!(
        launch_sig
            .mode_flags
            .iter()
            .any(|arg| arg.contains(".jefe/issue-prompt.md")),
        "CodePuppy issue signature must reference the issue prompt"
    );
}
