use super::prs_orchestration::{pr_send_info_from_state, write_pr_prompt};
use super::*;
use std::path::PathBuf;

use super::issues_send::{issue_send_info_from_state, prepare_issue_launch_signature};
use jefe::domain::{
    Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature, RemoteRepositorySettings,
    RepositoryId, RuntimeBinding, SandboxEngine,
};
use jefe::domain::{IssueDetail, IssueState};
use jefe::state::{AgentChooserState, ModalState, ScreenMode};

pub(super) trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

pub(super) trait TestOptionExt<T> {
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

pub(super) fn sample_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: PathBuf::from("/tmp/agent"),
        profile: String::new(),
        code_puppy_model: String::new(),
        llxprt_version: String::new(),
        code_puppy_yolo: Some(false),
        code_puppy_quick_resume: false,
        mode_flags: vec![String::from("--yolo")],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: jefe::domain::AgentKind::Llxprt,
    }
}

pub(super) fn sample_agent(agent_id: &AgentId) -> Agent {
    Agent::new(
        agent_id.clone(),
        RepositoryId(String::from("repo-1")),
        String::from("Agent One"),
        PathBuf::from("/tmp/agent"),
    )
}

#[test]
fn filter_and_search_messages_are_fresh_issue_list_reloads() {
    use super::issues_list_dispatch::is_fresh_issue_list_reload;

    assert!(is_fresh_issue_list_reload(&IssuesMessage::EnterMode));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::RefocusList));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ApplyFilter));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ClearFilter));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ApplySearch));
    assert!(!is_fresh_issue_list_reload(
        &IssuesMessage::NavigatePageDown
    ));
}

#[test]
fn repository_focus_toggles_checkbox_for_expected_fields() {
    assert!(repository_focus_toggles_checkbox(
        RepositoryFormFocus::RemoteEnabled
    ));
    assert!(repository_focus_toggles_checkbox(
        RepositoryFormFocus::SetupEnvDefault
    ));
    assert!(!repository_focus_toggles_checkbox(
        RepositoryFormFocus::Name
    ));
}

#[test]
fn clear_runtime_warning_clears_only_ssh_agent_warnings() {
    let mut state = AppState {
        warning_message: Some(String::from("SSH_AUTH_SOCK is missing")),
        ..AppState::default()
    };
    clear_runtime_warning(&mut state);
    assert!(state.warning_message.is_none());

    state.warning_message = Some(String::from("regular warning"));
    clear_runtime_warning(&mut state);
    assert_eq!(state.warning_message, Some(String::from("regular warning")));
}

#[test]
fn set_agent_runtime_binding_sets_session_and_signature() {
    let agent_id = AgentId(String::from("agent-1"));
    let mut state = AppState::default();
    state.agents.push(sample_agent(&agent_id));

    let signature = sample_signature();
    set_agent_runtime_binding(
        &mut state,
        &agent_id,
        String::from("jefe-agent-1"),
        signature.clone(),
        None,
        None,
    );

    let binding = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .and_then(|agent| agent.runtime_binding.as_ref());

    assert!(binding.is_some());
    if let Some(binding) = binding {
        assert_eq!(binding.session_name, String::from("jefe-agent-1"));
        assert_eq!(binding.launch_signature, signature);
        assert!(!binding.attached);
        assert!(binding.pid.is_none());
    }
}

#[test]
fn set_agent_runtime_binding_persists_pid() {
    let agent_id = AgentId(String::from("agent-pid"));
    let mut state = AppState::default();
    state.agents.push(sample_agent(&agent_id));

    let signature = sample_signature();
    set_agent_runtime_binding(
        &mut state,
        &agent_id,
        String::from("jefe-agent-pid"),
        signature.clone(),
        Some(12345),
        Some(jefe::domain::ProcessIdentity::new(12345, 67890)),
    );

    let binding = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .and_then(|agent| agent.runtime_binding.as_ref());

    assert!(binding.is_some());
    if let Some(binding) = binding {
        assert_eq!(binding.session_name, String::from("jefe-agent-pid"));
        assert_eq!(binding.launch_signature, signature);
        assert!(!binding.attached);
        assert_eq!(binding.pid, Some(12345));
    }
}

#[test]
fn mark_and_clear_runtime_attachment_flags() {
    let agent_a = AgentId(String::from("agent-a"));
    let agent_b = AgentId(String::from("agent-b"));

    let mut first = sample_agent(&agent_a);
    first.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess-a"),
        launch_signature: sample_signature(),
        attached: false,
        last_seen: None,
        process_identity: None,
        pid: None,
    });

    let mut second = sample_agent(&agent_b);
    second.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess-b"),
        launch_signature: sample_signature(),
        attached: true,
        last_seen: None,
        process_identity: None,
        pid: None,
    });

    let mut state = AppState::default();
    state.agents.push(first);
    state.agents.push(second);

    mark_agent_runtime_attached(&mut state, &agent_a, true);
    assert!(
        state.agents[0]
            .runtime_binding
            .as_ref()
            .is_some_and(|binding| binding.attached)
    );

    clear_agent_runtime_attachment(&mut state);
    assert!(state.agents.iter().all(|agent| {
        agent
            .runtime_binding
            .as_ref()
            .is_none_or(|binding| !binding.attached)
    }));
}

#[test]
fn mark_runtime_session_dead_sets_dead_and_detaches() {
    let agent_id = AgentId(String::from("agent-1"));
    let mut agent = sample_agent(&agent_id);
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess"),
        launch_signature: sample_signature(),
        attached: true,
        last_seen: None,
        process_identity: None,
        pid: None,
    });

    let mut state = AppState::default();
    state.agents.push(agent);

    mark_runtime_session_dead_if_present(&mut state, &agent_id);

    assert_eq!(state.agents[0].status, AgentStatus::Dead);
    assert!(
        state.agents[0]
            .runtime_binding
            .as_ref()
            .is_some_and(|binding| !binding.attached)
    );
}

#[test]
fn to_persisted_state_carries_hide_idle_toggle() {
    let state = AppState {
        hide_idle_repositories: true,
        ..AppState::default()
    };

    let persisted = to_persisted_state(&state);
    assert!(persisted.hide_idle_repositories);
}

#[test]
fn to_persisted_state_carries_pane_focus_and_terminal_focused() {
    let state = AppState {
        pane_focus: PaneFocus::Terminal,
        terminal_focused: true,
        ..AppState::default()
    };

    let persisted = to_persisted_state(&state);
    assert_eq!(persisted.pane_focus, "terminal");
    assert!(persisted.terminal_focused);
}

#[test]
fn pane_focus_round_trip_all_variants() {
    for focus in [
        PaneFocus::Repositories,
        PaneFocus::Agents,
        PaneFocus::Terminal,
    ] {
        let s = pane_focus_to_persisted(focus);
        assert_eq!(
            pane_focus_from_persisted(&s),
            focus,
            "round-trip for {focus:?}"
        );
    }
}

#[test]
fn pane_focus_from_persisted_unknown_defaults_to_repositories() {
    // Older state files written before this field existed have "" or an
    // unrecognized value; both must fall back to Repositories.
    assert_eq!(pane_focus_from_persisted(""), PaneFocus::Repositories);
    assert_eq!(pane_focus_from_persisted("bogus"), PaneFocus::Repositories);
}

/// to_persisted_state must EXCLUDE all prs_state data — no PR key appears in
/// the serialized JSON.
///
/// Build a PullRequest populated with non-default data.
fn test_pr(number: u64) -> jefe::domain::PullRequest {
    use jefe::domain::{PrCheckStatus, PrState};
    jefe::domain::PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// Build a PullRequestDetail populated with non-default data.
fn test_pr_detail(number: u64) -> jefe::domain::PullRequestDetail {
    use jefe::domain::{PrCheckStatus, PrState};
    jefe::domain::PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}

/// Build an AppState populated with non-default PR data.
fn state_with_active_prs() -> jefe::state::AppState {
    use jefe::domain::{Repository, RepositoryId};
    use jefe::state::ScreenMode;
    use std::path::PathBuf;

    let mut prs_state = jefe::state::PullRequestsState {
        active: true,
        pr_detail: Some(test_pr_detail(1)),
        ..jefe::state::PullRequestsState::default()
    };
    prs_state.list.replace_items(vec![test_pr(1)]);
    prs_state.list.set_selected_index(Some(0));
    let mut state = jefe::state::AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state
}

/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 66-76
/// NOTE: this test lives in src/app_input/app_input_tests.rs (alongside the
/// to_persisted_state_carries_hide_idle_toggle precedent) because
/// to_persisted_state is module-private to app_input (declared in main.rs as
/// `mod app_input`, NOT `pub mod app_input` in lib.rs), so it is NOT reachable
/// from a test in the src/state module without changing production visibility.
#[test]
fn test_to_persisted_state_excludes_prs_state() {
    let state = state_with_active_prs();

    let persisted = to_persisted_state(&state);
    let json = serde_json::to_value(&persisted).value_or_panic("persisted should serialize");

    let json_str = serde_json::to_string(&json).value_or_panic("json should stringify");
    let lower = json_str.to_lowercase();
    assert!(
        !lower.contains("prs_state")
            && !lower.contains("pull_request")
            && !lower.contains("pr_detail"),
        "persisted state must not carry any PR data, got: {json_str}"
    );

    assert!(json.get("repositories").is_some());
    assert!(json.get("agents").is_some());
    assert!(json.get("selected_repository_index").is_some());
    assert!(json.get("selected_agent_index").is_some());
    assert!(json.get("hide_idle_repositories").is_some());
    assert!(json.get("last_selected_agent_by_repo").is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// P11 Dispatch-Ordering Tests
//
// These assert OBSERVABLE STATE from the synchronous pre-spawn portion of
// dispatch (not spawn counts — no spawn-recording seam exists). They mirror
// how the issues async-dispatch tests assert state + the written prompt file.
// ═══════════════════════════════════════════════════════════════════════════

/// `PullRequests(OpenInBrowser)` dispatch with a valid repo + selected PR:
/// the reducer sets `draft_notice == "Opening pull request in browser..."`
/// synchronously BEFORE the async spawn (reducer-before-spawn ordering).
///
/// Exercises the dispatch path's synchronous reducer portion (the same
/// `apply_and_persist(PrOpenInBrowser)` that the `mod.rs` dispatch arm runs
/// BEFORE calling `dispatch_pr_open_in_browser`). Since `AppStateHandle`
/// cannot be constructed in unit tests, we apply through `state.apply()` —
/// the exact reducer transition the dispatch runs synchronously before spawn.
/// The `pr_open_in_browser_info_from_state` call proves the dispatch would
/// resolve a valid info (proceed to spawn), and the notice proves the
/// reducer-before-spawn effect.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 113-115
/// @pseudocode component-003 lines 190-215
#[test]
fn test_open_in_browser_sets_opening_notice_through_dispatch() {
    use jefe::state::AppEvent;

    let mut state = state_with_active_prs();
    // Set a valid GitHub repo slug so the dispatch resolves a valid info.
    if let Some(idx) = state.selected_repository_index {
        state.repositories[idx].github_repo = "owner/repo".to_string();
    }

    // Prove the dispatch would proceed to spawn (valid info resolved) — read
    // this BEFORE the apply (which takes ownership).
    let info = prs_dispatch::pr_open_in_browser_info_from_state(&state);
    assert!(
        info.is_ok(),
        "valid repo + selected PR must resolve Ok info for dispatch spawn"
    );

    // The dispatch arm runs apply_and_persist(PrOpenInBrowser) BEFORE the spawn.
    // Exercise that exact reducer transition.
    let after = state.apply(AppEvent::PrOpenInBrowser);

    let notice = after
        .prs_state
        .draft_notice
        .as_ref()
        .unwrap_or_else(|| panic!("draft_notice must be set after PrOpenInBrowser"));
    assert!(
        notice.to_lowercase().contains("opening") && notice.to_lowercase().contains("browser"),
        "notice should mention opening/browser, got: {notice}"
    );
}

/// `o` with no selection: the HANDLER emits `PrShowNotice(NoSelectionToOpen)`,
/// so `draft_notice` is the no-selection message AND no loading/pending flag
/// is set (the NoSelection path never reaches the dispatch spawn).
///
/// Exercises the REAL handler (`resolve_prs_key_event` → `handle_pr_list_key`)
/// which yields the event, then applies it through the reducer — NOT a
/// hand-applied PrShowNotice.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 200-201
#[test]
fn test_open_in_browser_no_selection_sets_notice_through_handler() {
    use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};
    use jefe::state::{PullRequestsState, ReadOnlyHintKind, ScreenMode};

    let state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state: {
            let mut ps = PullRequestsState {
                active: true,
                pr_focus: jefe::state::PrFocus::PrList,
                ..PullRequestsState::default()
            };
            ps.list.set_selected_index(None);
            ps
        },
        ..AppState::default()
    };

    // Drive the `o` key through the REAL handler (the same path the UI uses).
    let key_event = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('o'));
    let event = prs::resolve_prs_key_event(&state, &key_event);
    assert!(
        matches!(
            event,
            Some(jefe::state::AppEvent::PrShowNotice(
                ReadOnlyHintKind::NoSelectionToOpen
            ))
        ),
        "handler must emit PrShowNotice(NoSelectionToOpen) for no-selection, got {event:?}"
    );

    // Apply the handler-emitted event through the reducer (observable state).
    let event = event.unwrap_or_else(|| panic!("handler must emit an event for 'o' key"));
    let after = state.apply(event);

    let notice = after
        .prs_state
        .draft_notice
        .as_ref()
        .unwrap_or_else(|| panic!("draft_notice must be set after NoSelectionToOpen"));
    assert!(
        notice.to_lowercase().contains("no") && notice.to_lowercase().contains("pull request"),
        "notice should mention no pull request selected, got: {notice}"
    );
    // No loading/pending flags set — the no-selection path never reaches the
    // dispatch spawn.
    assert!(
        !after.prs_state.list_loading(),
        "no-selection path must not set loading.list"
    );
    assert!(
        !after.prs_state.loading.detail,
        "no-selection path must not set loading.detail"
    );
    assert!(
        !after.prs_state.loading.comments,
        "no-selection path must not set loading.comments"
    );
}

/// Build an AppState for the PR agent-chooser confirm test: an open chooser +
/// selected PR + a PR detail + an agent whose work_dir is a temp dir.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
fn state_for_pr_agent_chooser_confirm(
    agent_id: &AgentId,
    work_dir: &std::path::Path,
) -> jefe::state::AppState {
    use jefe::domain::{Agent, RepositoryId};
    use jefe::state::{AgentChooserState, ScreenMode};
    use std::path::PathBuf;

    let mut agent = Agent::new(
        agent_id.clone(),
        RepositoryId(String::from("repo-1")),
        String::from("PR Agent"),
        work_dir.to_path_buf(),
    );
    agent.profile = String::new();
    agent.mode_flags = Vec::new();

    let mut prs_state = jefe::state::PullRequestsState {
        active: true,
        pr_detail: Some(test_pr_detail(42)),
        agent_chooser: Some(AgentChooserState {
            selected_index: 0,
            agents: vec![(agent_id.clone(), String::from("PR Agent"))],
        }),
        ..jefe::state::PullRequestsState::default()
    };
    prs_state.list.replace_items(vec![test_pr(42)]);
    prs_state.list.set_selected_index(Some(0));
    let mut state = jefe::state::AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state,
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

/// Agent-chooser confirm applies the reducer BEFORE the side effects: after
/// the confirm dispatch the agent chooser is CLOSED in state, the send is
/// recorded, and `{work_dir}/.jefe/pr-prompt.md` exists with non-empty content
/// containing the PR number — proving `apply_and_persist(PrAgentChooserConfirm)`
/// ran BEFORE `write_pr_prompt`/`launch_pr_agent`.
///
/// Exercises the dispatch ordering through observable state + filesystem
/// effects. Since `AppStateHandle` cannot be constructed in unit tests, the
/// test replicates the EXACT dispatch sequence on raw `AppState`:
/// (1) `pr_send_info_from_state` reads send info,
/// (2) `state.apply(PrAgentChooserConfirm)` closes the chooser (reducer-before-side-effect),
/// (3) `write_pr_prompt` writes the prompt file.
/// The `ctx` is `None` so `launch_pr_agent` would be guarded (no real spawn).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-156
#[test]
fn test_pr_agent_chooser_confirm_applies_reducer_before_side_effects() {
    use jefe::state::AppEvent;

    let agent_id = AgentId(String::from("pr-agent-1"));
    let temp_work_dir = std::env::temp_dir().join(format!(
        "jefe-pr-agent-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let prompt_path = temp_work_dir.join(".jefe").join("pr-prompt.md");

    let state = state_for_pr_agent_chooser_confirm(&agent_id, &temp_work_dir);

    // (1) Read send info BEFORE applying the reducer (mirrors dispatch ordering).
    let send_info = pr_send_info_from_state(&state);
    let send_info = send_info
        .unwrap_or_else(|| panic!("pr_send_info must resolve with chooser + detail + agent"));

    // Verify the PrSendPayload carries structured fields.
    assert_eq!(send_info.payload.pr_number, 42);
    assert_eq!(send_info.payload.repository, "owner/repo");
    assert!(!send_info.payload.pr_title.is_empty());

    // (2) Apply the PrAgentChooserConfirm reducer (closes chooser) — this runs
    // BEFORE write_pr_prompt/launch in the real dispatch.
    let after_confirm = state.apply(AppEvent::PrAgentChooserConfirm);
    assert!(
        after_confirm.prs_state.agent_chooser.is_none(),
        "PrAgentChooserConfirm must close the agent chooser BEFORE side effects"
    );

    // (3) Write the prompt file (mirrors the dispatch's write_pr_prompt call).
    let result = write_pr_prompt(&send_info.work_dir, &send_info.payload);
    assert!(
        result.is_ok(),
        "write_pr_prompt should succeed: {:?}",
        result.err()
    );

    // The prompt file exists with non-empty content containing the PR number.
    assert!(
        prompt_path.exists(),
        "pr-prompt.md must exist at {prompt_path:?}"
    );
    let content = std::fs::read_to_string(&prompt_path)
        .unwrap_or_else(|e| panic!("should read pr-prompt.md: {e}"));
    assert!(
        !content.is_empty(),
        "pr-prompt.md content must be non-empty"
    );
    assert!(
        content.contains('#') && content.contains("42"),
        "pr-prompt.md should contain the PR number and a heading, got: {content}"
    );

    // Cleanup.
    let _ = std::fs::remove_dir_all(&temp_work_dir);
}

/// Inline-submit dispatch applies the reducer BEFORE the mutation side effect.
///
/// The `PullRequests(Inline(Submit))` dispatch arm runs
/// `apply_and_persist(message)` BEFORE `prs_mutation::handle_pr_inline_submit`
/// (component-004 line 112). The PR design (unlike issues mode) relies on the
/// reducer's `pr_inline_submit` to set `prs_state.mutation_pending`; the
/// mutation helper's `resolve_pr_inline_submit` then REQUIRES that pending
/// marker to reach `create_pr_comment`. If the reducer-apply were skipped,
/// `mutation_pending` would stay `None` and the create path would be
/// unreachable through real dispatch (GREEN-for-wrong-reason).
///
/// Since `AppStateHandle` cannot be constructed in unit tests, this replicates
/// the EXACT dispatch sequence on raw `AppState`: a non-blank composer is open,
/// then `state.apply(PrInlineSubmit)` runs the same reducer transition the
/// dispatch arm performs, and the test asserts the resulting state satisfies
/// the mutation precondition (composer preserved + `mutation_pending` set).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 110-115
#[test]
fn test_inline_submit_dispatch_applies_reducer_before_mutation() {
    use jefe::state::AppEvent;
    use jefe::state::{ComposerTarget, InlineState};

    let mut state = state_with_active_prs();
    // An open composer holding non-blank text — the precondition for a submit.
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "ship it".to_string(),
        cursor: 7,
    };
    assert!(
        state.prs_state.mutation_pending.is_none(),
        "precondition: no mutation pending before submit"
    );

    // The dispatch arm runs apply_and_persist(PrInlineSubmit) BEFORE the
    // mutation helper. Exercise that exact reducer transition.
    let after = state.apply(AppEvent::PrInlineSubmit);

    // The reducer set mutation_pending — this is the marker that
    // resolve_pr_inline_submit requires to reach create_pr_comment. Without the
    // apply, this would be None and the mutation would never fire.
    let pending = after
        .prs_state
        .mutation_pending
        .as_ref()
        .unwrap_or_else(|| {
            panic!("PrInlineSubmit must set mutation_pending before the side effect")
        });
    assert!(
        matches!(pending.target, ComposerTarget::NewComment),
        "pending target should carry the composer target"
    );
    // The composer text is preserved through the reducer so the dispatch helper
    // can read it for the create payload.
    assert!(
        matches!(
            after.prs_state.inline_state,
            InlineState::Composer { ref text, .. } if text == "ship it"
        ),
        "composer text must be preserved for the mutation helper to read"
    );
}

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
        body: "Send to agent".to_owned(),
        external_url: "https://github.com/owner/repo/issues/166".to_owned(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
        issue_type_name: None,
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
    assert!(
        launch_sig
            .mode_flags
            .iter()
            .any(|flag| flag.contains(".jefe/issue-prompt.md")),
        "issue launch signature must include the issue prompt instruction"
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
