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
fn tracker_ref(value: &str) -> jefe::domain::GitHubRepoRef {
    jefe::domain::GitHubRepoRef::parse(value)
        .unwrap_or_else(|error| panic!("valid tracker must parse: {error}"))
        .unwrap_or_else(|| panic!("valid tracker must not be blank"))
}

use super::issue_self_assignment::{IssueAssignment, SelfAssignment};
use super::issues_send::{issue_send_info_from_state, prepare_issue_launch_signature};
use jefe::domain::{AgentId, IssueDetail, IssueState, RepositoryId};
use jefe::state::{AgentChooserState, ModalState, ScreenMode};

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
        node_id: String::new(),
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
/// so the confirm key handler routes Enter/Esc/n correctly.
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

// ── Issue #186: self-assign the issue to the viewer on send-to-agent ────

/// On a successful send-to-agent, the issue must be self-assigned to the
/// authenticated viewer. The assignment derives its `owner`/`repo` and issue
/// number from the loaded issue detail, independently of clone identity.
#[test]
fn self_assignment_resolves_owner_repo_and_issue_from_send_context() {
    let agent_id = AgentId(String::from("issue-self-assign"));
    let work_dir = PathBuf::from("/tmp/jefe-issue-self-assign");
    let mut state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);
    state.repositories[0].github_repo = "acme/widgets".to_owned();

    let send_info = issue_send_info_from_state(&state)
        .value_or_panic("issue send info must resolve for self-assignment");

    let tracker = tracker_ref(&send_info.payload.repository);
    let assignment =
        SelfAssignment::from_send_context(Some(&tracker), send_info.payload.issue_number)
            .value_or_panic("a valid tracker must produce a self-assignment");

    // The loaded detail records owner/repo; changing clone configuration
    // after loading must not retarget the assignment.
    assert_eq!(assignment.owner, "owner");
    assert_eq!(assignment.repo, "repo");
    assert_eq!(assignment.owner_repo, "owner/repo");
    // issue_number comes from the loaded issue detail (fixture sets 166).
    assert_eq!(assignment.issue_number, 166);
}

/// Assignment uses the loaded detail even when the working repository has no
/// clone identity, because tracker identity and clone identity are independent.
#[test]
fn self_assignment_uses_loaded_detail_without_clone_identity() {
    let agent_id = AgentId(String::from("issue-no-clone"));
    let work_dir = PathBuf::from("/tmp/jefe-issue-no-clone");
    let state = state_for_issue_agent_chooser_send(&agent_id, &work_dir);

    let send_info = issue_send_info_from_state(&state)
        .value_or_panic("issue send info must resolve without github_repo");

    let tracker = tracker_ref(&send_info.payload.repository);
    let assignment =
        SelfAssignment::from_send_context(Some(&tracker), send_info.payload.issue_number)
            .value_or_panic("loaded issue source must produce assignment context");
    assert_eq!(assignment.owner_repo, "owner/repo");
    assert!(send_info.clone_identity.is_none());
}

/// The self-assignment context survives the to_state/from_state round-trip
/// through the preflight modal (issue #186). After a post-preflight launch,
/// `from_state` must reconstruct the same owner/repo/issue_number so the
/// non-blocking assignment still fires.
#[test]
fn self_assignment_survives_preflight_modal_round_trip() {
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;

    let tracker = tracker_ref("vybestack/llxprt-jefe");
    let assignment =
        SelfAssignment::from_send_context(Some(&tracker), 186).value_or_panic("tracker is valid");

    let carried = assignment.to_state();
    let (owner_repo, issue_number) = match &carried {
        FollowUp::Resolved {
            owner_repo,
            issue_number,
        } => (owner_repo.clone(), *issue_number),
        FollowUp::Unavailable { .. } => panic!("resolved identity must carry as Resolved"),
    };
    assert_eq!(owner_repo, "vybestack/llxprt-jefe");
    assert_eq!(issue_number, 186);

    let reconstructed = SelfAssignment::from_state(&carried)
        .value_or_panic("round-trip must reconstruct from the carried shortform");
    assert_eq!(reconstructed.owner, assignment.owner);
    assert_eq!(reconstructed.repo, assignment.repo);
    assert_eq!(reconstructed.owner_repo, assignment.owner_repo);
    assert_eq!(reconstructed.issue_number, assignment.issue_number);
}

/// `from_state` rejects a malformed carried shortform so a corrupted modal
/// payload cannot trigger an assignment against an unintended target.
#[test]
fn self_assignment_from_state_rejects_malformed_shortform() {
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;

    let malformed = FollowUp::Resolved {
        owner_repo: "not-a-valid-shortform".to_string(),
        issue_number: 186,
    };
    assert!(
        SelfAssignment::from_state(&malformed).is_none(),
        "a shortform without exactly one '/' must not reconstruct"
    );
}

/// `IssueAssignment::carried` distinguishes a resolved target from an
/// unavailable one, so the post-preflight path can still warn instead of
/// silently skipping (issue #186).
#[test]
fn issue_assignment_carried_unavailable_when_no_identity() {
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;

    let intent = IssueAssignment::from_send_context(None, 186);
    let carried = intent.carried();
    match carried {
        FollowUp::Unavailable {
            issue_number,
            reason,
        } => {
            assert_eq!(issue_number, 186);
            assert!(
                reason.contains("No valid GitHub repo"),
                "unavailable reason must explain the missing repo: {reason}"
            );
        }
        other @ FollowUp::Resolved { .. } => {
            panic!("missing identity must carry as Unavailable, got {other:?}")
        }
    }
}

/// A resolved identity carries as `Resolved` with the validated shortform.
#[test]
fn issue_assignment_carried_resolved_when_identity_present() {
    use jefe::state::IssueSelfAssignmentFollowUp as FollowUp;

    let tracker = tracker_ref("vybestack/llxprt-jefe");
    let intent = IssueAssignment::from_send_context(Some(&tracker), 186);
    let carried = intent.carried();
    match carried {
        FollowUp::Resolved {
            owner_repo,
            issue_number,
        } => {
            assert_eq!(owner_repo, "vybestack/llxprt-jefe");
            assert_eq!(issue_number, 186);
        }
        other @ FollowUp::Unavailable { .. } => {
            panic!("resolved identity must carry as Resolved, got {other:?}")
        }
    }
}
