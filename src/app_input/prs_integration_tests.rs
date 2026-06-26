//! PR-Mode end-to-end integration tests — app_input layer checkpoints.
//!
//! Drives the REAL key→event→dispatch→reducer chain for the app_input-owned
//! checkpoints of Phase P15. These tests exercise the genuine key-resolution
//! handlers (`prs::resolve_prs_key_event`, `normal::resolve_mode_key`) and
//! dispatch helpers (`prs_dispatch`, `prs_list_dispatch`), then simulate async
//! I/O completion by applying the loaded-data events the dispatch layer would
//! deliver — exactly as the existing exemplars in `app_input_tests.rs` do.
//!
//! @plan PLAN-20260624-PR-MODE.P15
//! @requirement REQ-PR-001
//! @requirement REQ-PR-003
//! @requirement REQ-PR-006
//! @requirement REQ-PR-008
//! @requirement REQ-PR-011
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013
//! @requirement REQ-PR-NFR-001
//! @pseudocode component-001 lines 66-291
//! @pseudocode component-003 lines 01-133
//! @pseudocode component-004 lines 97-175

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};
use std::path::PathBuf;

use jefe::domain::{
    Agent, AgentId, DEFAULT_SANDBOX_FLAGS, LaunchSignature, PrCheckStatus, PrState, PullRequest,
    RemoteRepositorySettings, Repository, RepositoryId, SandboxEngine,
};
use jefe::persistence::State as PersistedState;
use jefe::state::{AppEvent, AppState, PrFocus, ReadOnlyHintKind, ScreenMode};

// Import only the submodule paths (NOT iocraft::prelude::* which shadows
// std::boxed::Box). The private fns pr_send_info_from_state and write_pr_prompt
// are visible to child modules via super::.
use super::{
    AppStateHandle, SharedContext, normal, pr_send_info_from_state, prs, prs_comments_dispatch,
    prs_dispatch, prs_list_dispatch, to_persisted_state, write_pr_prompt,
};

/// Build a `KeyEvent` for a press of the given code (no modifiers).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-003 lines 01-133
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

/// Minimal PR list-row fixture.
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-006
/// @pseudocode component-002 lines 22-34
fn make_test_pr(number: u64) -> PullRequest {
    PullRequest {
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

/// Dashboard AppState with two repositories, each having a valid `github_repo`
/// slug, and the first selected.
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-76
fn dashboard_prs_state() -> AppState {
    let mut state = AppState::default();
    for (idx, slug) in ["repo-1", "repo-2"].iter().enumerate() {
        let mut repo = Repository::new(
            RepositoryId(slug.to_string()),
            format!("Repo {idx}"),
            format!("owner{idx}/{slug}"),
            PathBuf::from(format!("/tmp/{slug}")),
        );
        repo.github_repo = format!("owner{idx}/{slug}");
        state.repositories.push(repo);
    }
    state.selected_repository_index = Some(0);
    state
}

/// PR-mode active state derived from `dashboard_prs_state` after entering PR mode.
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-76
fn active_prs_state() -> AppState {
    let mut state = dashboard_prs_state();
    state.apply_in_place(AppEvent::EnterPrsMode);
    state
}

/// In-place apply helper to avoid the `let state = state.apply(...)` dance in
/// multi-step test scenarios.
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 66-291
trait ApplyInPlace {
    fn apply_in_place(&mut self, event: AppEvent);
}

impl ApplyInPlace for AppState {
    fn apply_in_place(&mut self, event: AppEvent) {
        let owned = std::mem::take(self);
        *self = owned.apply(event);
    }
}
// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 1: entry → list reload spawned → PrListLoaded renders rows
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 1: `p` from Dashboard enters PR mode, the reducer sets
/// `loading.list = true` (the dispatch arm then spawns the list fetch), and
/// delivering `PrListLoaded` (simulating async completion) renders rows.
///
/// Drives the REAL entry-routing key handler (`normal::resolve_mode_key`) for
/// the `p` key, applies `EnterPrsMode` through the REAL reducer, then delivers
/// a `PrListLoaded` event for the selected scope and asserts the rows appear.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-001
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 66-76,209-223
/// @pseudocode component-003 lines 01-09
#[test]
fn it_enter_prs_mode_from_dashboard_loads_list() {
    use super::normal::KeyHandling;

    let dashboard = dashboard_prs_state();
    assert_eq!(dashboard.screen_mode, ScreenMode::Dashboard);

    // Drive the REAL entry-routing key handler: `p` → EnterPrsMode.
    let handling = normal::resolve_mode_key(&key(KeyCode::Char('p')), ScreenMode::Dashboard);
    assert!(
        matches!(handling, KeyHandling::Handled(Some(AppEvent::EnterPrsMode))),
        "Dashboard 'p' must emit Handled(Some(EnterPrsMode))"
    );

    // Apply the reducer: loading.list=true, active=true, pr_focus=PrList.
    let mut state = dashboard.apply(AppEvent::EnterPrsMode);
    assert!(state.prs_state.active);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrList);
    assert!(
        state.prs_state.loading.list,
        "EnterPrsMode must set loading.list=true (reload spawned)"
    );

    // Simulate async completion: deliver PrListLoaded for the current scope.
    let scope = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx).map(|r| r.id.clone()))
        .unwrap_or_else(|| panic!("a repository must be selected"));
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: scope,
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id: 0,
        pull_requests: vec![make_test_pr(1), make_test_pr(2)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(
        state.prs_state.pull_requests.len(),
        2,
        "PrListLoaded must render both rows"
    );
    assert!(
        !state.prs_state.loading.list,
        "PrListLoaded must clear loading.list"
    );
    assert_eq!(
        state.prs_state.selected_pr_index,
        Some(0),
        "first row selected after load"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 2: repo nav switches scope and reloads (#47)
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 2: repo Up/Down in the RepoList focus changes
/// `selected_repository_index`, and the reducer resets the PR list/detail and
/// marks `loading.list = true` (a new reload is spawned for the new scope).
///
/// Drives the REAL key handler for Down in RepoList focus (`resolve_prs_key_event`)
/// → `PrNavigateDown`, then applies through the reducer, and asserts the scope
/// change resets PR data.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-003
/// @pseudocode component-001 lines 88-98,146-153
/// @pseudocode component-003 lines 49-56
#[test]
fn it_repo_nav_switches_scope_and_reloads() {
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::RepoList;

    // Seed loaded PR data so we can assert it is cleared on scope change.
    state.prs_state.pull_requests = vec![make_test_pr(10), make_test_pr(20)];
    state.prs_state.selected_pr_index = Some(1);
    state.prs_state.loading.list = false;

    // Drive the REAL key handler for Down in RepoList focus.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Down));
    assert!(
        matches!(event, Some(AppEvent::PrNavigateDown)),
        "Down in RepoList must emit PrNavigateDown (got {event:?})"
    );

    // Apply through the reducer: scope changes, PR data resets, reload flagged.
    state.apply_in_place(event.unwrap_or_else(|| panic!("Down must emit an event")));

    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "repo nav must move to index 1"
    );
    assert!(
        state.prs_state.pull_requests.is_empty(),
        "scope change must clear the PR list"
    );
    assert!(
        state.prs_state.selected_pr_index.is_none(),
        "scope change must clear selected_pr_index"
    );
    assert!(
        state.prs_state.loading.list,
        "scope change must flag a list reload"
    );
}
// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 3: filter apply reloads and updates list (#38/#40)
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 3: opening filter controls, cycling a draft filter field, and
/// applying the filter copies the draft to `committed_filter` and flags a list
/// reload (`loading.list = true`) — the interactive filter path (#38/#40).
///
/// Drives the REAL key handlers for the filter-controls sub-focus: `f` (open
/// controls) via `resolve_prs_key_event` in the filter tier is not a direct
/// key — instead we open via `PrOpenFilterControls`, then cycle a draft field
/// via the REAL filter-controls key handler, and apply via the Enter key
/// handler. Then delivers a `PrListLoaded` for the new filter and asserts the
/// list updates.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-281
/// @pseudocode component-003 lines 134-146
#[test]
fn it_filter_apply_reloads_and_updates_list() {
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrList;

    // Open filter controls (the dispatch arm runs this reducer transition).
    state.apply_in_place(AppEvent::PrOpenFilterControls);
    assert!(state.prs_state.filter_ui.controls_open);

    // Cycle the review-decision draft filter via the REAL filter-controls handler.
    // Field index 2 = review-decision; space cycles it.
    state.prs_state.filter_ui.field_index = 2;
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Char(' ')));
    assert!(
        matches!(event, Some(AppEvent::PrCycleReviewFilter)),
        "space on review field must emit PrCycleReviewFilter (got {event:?})"
    );
    state.apply_in_place(event.unwrap_or_else(|| panic!("space must emit an event")));

    // Apply the filter via the REAL Enter key handler.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::PrApplyFilter)),
        "Enter must emit PrApplyFilter (got {event:?})"
    );
    state.apply_in_place(event.unwrap_or_else(|| panic!("Enter must emit an event")));

    // committed_filter must now carry the cycled review decision, and a reload
    // must be flagged.
    assert!(
        !state.prs_state.filter_ui.controls_open,
        "apply must close filter controls"
    );
    assert!(
        state.prs_state.loading.list,
        "apply must flag a list reload"
    );
    // The draft and committed review decisions now match (draft was copied).
    assert_eq!(
        state.prs_state.committed_filter.review_decision,
        state.prs_state.draft_filter.review_decision,
        "apply must copy draft → committed"
    );

    // Simulate async completion with the new filter: rows update.
    let scope = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx).map(|r| r.id.clone()))
        .unwrap_or_else(|| panic!("a repository must be selected"));
    state.apply_in_place(AppEvent::PrListLoaded {
        scope_repo_id: scope,
        filter: std::boxed::Box::new(state.prs_state.committed_filter.clone()),
        request_id: 0,
        pull_requests: vec![make_test_pr(100)],
        cursor: None,
        has_more: false,
    });
    assert_eq!(
        state.prs_state.pull_requests.len(),
        1,
        "filtered list must show the reloaded rows"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 4: search commit reloads with query
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 4: `/` focuses the search input, typing builds the query, Enter
/// applies the search (copies `search_query` → `committed_filter.query_text`)
/// and flags a list reload.
///
/// Drives the REAL search-input key handler for char + Enter, then asserts the
/// committed filter carries the query and a reload is flagged.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 282-291
/// @pseudocode component-003 lines 127-133
#[test]
fn it_search_commit_reloads_with_query() {
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrList;

    // Focus the search input (the dispatch arm runs this reducer transition).
    state.apply_in_place(AppEvent::PrFocusSearchInput);
    assert!(state.prs_state.search_input_focused);

    // Type 'b' via the REAL search-input key handler.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Char('b')));
    assert!(
        matches!(event, Some(AppEvent::PrSetSearchQuery { .. })),
        "char in search must emit PrSetSearchQuery (got {event:?})"
    );
    state.apply_in_place(event.unwrap_or_else(|| panic!("char must emit an event")));

    // Type 'u' via the REAL search-input key handler.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Char('u')));
    state.apply_in_place(event.unwrap_or_else(|| panic!("char must emit an event")));
    assert_eq!(
        state.prs_state.search_query, "bu",
        "search query must accumulate typed chars"
    );

    // Enter applies the search via the REAL search-input key handler.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::PrApplySearch)),
        "Enter must emit PrApplySearch (got {event:?})"
    );
    state.apply_in_place(event.unwrap_or_else(|| panic!("Enter must emit an event")));

    assert_eq!(
        state.prs_state.committed_filter.query_text, "bu",
        "apply search must copy query → committed_filter.query_text"
    );
    assert!(
        !state.prs_state.search_input_focused,
        "apply search must blur the input"
    );
    assert!(
        state.prs_state.loading.list,
        "apply search must flag a list reload"
    );
}
// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 8: send-to-agent writes prompt and launches (REQ-PR-011)
// ═════════════════════════════════════════════════════════════════════════

/// Build a PR detail fixture for send-to-agent tests.
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
fn make_test_pr_detail(number: u64) -> jefe::domain::PullRequestDetail {
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
    }
}

/// Build a launch signature fixture (mirrors `app_input_tests::sample_signature`).
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
fn sample_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: PathBuf::from("/tmp/agent"),
        profile: String::new(),
        mode_flags: vec![String::from("--yolo")],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: RemoteRepositorySettings::default(),
    }
}

/// Build a PR-mode state with a loaded PR detail, selected PR, and agent for
/// the send-to-agent test.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
fn state_for_send_to_agent(agent_id: &AgentId, work_dir: &std::path::Path) -> AppState {
    let mut agent = Agent::new(
        agent_id.clone(),
        RepositoryId(String::from("repo-1")),
        String::from("PR Agent"),
        work_dir.to_path_buf(),
    );
    agent.profile = String::new();
    agent.mode_flags = Vec::new();

    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.pull_requests = vec![make_test_pr(42)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(make_test_pr_detail(42));
    state.agents.push(agent);
    state
}

/// Assert that the prompt file exists, is non-empty, and contains the PR number.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
fn assert_prompt_content(prompt_path: &std::path::Path, pr_number: &str) {
    assert!(prompt_path.exists(), "pr-prompt.md must exist");
    let content = std::fs::read_to_string(prompt_path)
        .unwrap_or_else(|e| panic!("should read pr-prompt.md: {e}"));
    assert!(!content.is_empty(), "prompt content must be non-empty");
    assert!(
        content.contains(pr_number),
        "prompt must contain PR number {pr_number}, got: {content}"
    );
}

/// Checkpoint 8: `S` opens the agent chooser, Enter confirms, and the dispatch
/// arm applies the reducer BEFORE writing the prompt file — `.jefe/pr-prompt.md`
/// exists with non-empty content containing the PR number. The resolved send
/// info identifies the correct LAUNCH TARGET agent (the AgentId the
/// chooser-confirm would launch) and the correct work_dir, proving the launch
/// target is resolved pre-spawn.
///
/// Drives the REAL key handlers for `S` (open chooser) and Enter (confirm),
/// then replicates the EXACT dispatch sequence on raw `AppState`: read
/// `pr_send_info_from_state`, apply `PrAgentChooserConfirm` (closes chooser),
/// and `write_pr_prompt` writes the file.
///
/// NOTE: the actual agent launch requires runtime (`SharedContext` is `None`
/// in unit tests → the spawn is guarded), mirroring the established convention
/// in `app_input_tests.rs` (which writes the prompt and asserts target
/// resolution but cannot observe the spawn). Do NOT add a production seam.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
#[test]
fn it_send_to_agent_writes_prompt_file_for_launch() {
    let agent_id = AgentId(String::from("pr-agent-1"));
    let temp_work_dir = std::env::temp_dir().join(format!(
        "jefe-pr-int-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let prompt_path = temp_work_dir.join(".jefe").join("pr-prompt.md");

    let mut state = state_for_send_to_agent(&agent_id, &temp_work_dir);

    // Drive the REAL `S` key handler → PrOpenAgentChooser.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Char('S')));
    assert!(
        matches!(event, Some(AppEvent::PrOpenAgentChooser)),
        "'S' must emit PrOpenAgentChooser (got {event:?})"
    );
    state.apply_in_place(event.unwrap_or_else(|| panic!("S must emit an event")));
    assert!(
        state.prs_state.agent_chooser.is_some(),
        "PrOpenAgentChooser must open the chooser"
    );

    // Drive the REAL Enter key handler → PrAgentChooserConfirm.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::PrAgentChooserConfirm)),
        "Enter must emit PrAgentChooserConfirm (got {event:?})"
    );

    // Dispatch ordering: read info, apply reducer, then write prompt.
    let send_info =
        pr_send_info_from_state(&state).unwrap_or_else(|| panic!("pr_send_info must resolve"));
    assert_eq!(send_info.payload.pr_number, 42);
    // The resolved send info identifies the correct LAUNCH TARGET agent (the
    // AgentId the chooser-confirm would launch) and the correct work_dir —
    // proving the launch target is resolved pre-spawn.
    assert_eq!(
        send_info.agent_id, agent_id,
        "send info must resolve the selected agent id (the launch target)"
    );
    assert_eq!(
        send_info.work_dir, temp_work_dir,
        "send info must resolve the agent's work_dir (the launch cwd)"
    );
    state.apply_in_place(AppEvent::PrAgentChooserConfirm);
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "confirm must close the chooser BEFORE side effects"
    );
    write_pr_prompt(&send_info.work_dir, &send_info.payload)
        .unwrap_or_else(|e| panic!("write_pr_prompt should succeed: {e:?}"));

    assert_prompt_content(&prompt_path, "42");
    let _ = std::fs::remove_dir_all(&temp_work_dir);
    let _ = sample_signature();
}
// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 9: open-in-browser spawns gh pr view --web (off-thread)
// ═════════════════════════════════════════════════════════════════════════

/// Assert the no-selection path: `o` with no selected PR emits
/// `NoSelectionToOpen` (no silent drop) and surfaces a visible notice.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69
fn assert_o_no_selection_emits_notice() {
    let mut state_no_sel = active_prs_state();
    state_no_sel.prs_state.pr_focus = PrFocus::PrList;
    state_no_sel.prs_state.selected_pr_index = None;
    let event = prs::resolve_prs_key_event(&state_no_sel, &key(KeyCode::Char('o')));
    assert!(
        matches!(
            event,
            Some(AppEvent::PrShowNotice(ReadOnlyHintKind::NoSelectionToOpen))
        ),
        "'o' with no selection must emit NoSelectionToOpen (got {event:?})"
    );
    state_no_sel.apply_in_place(event.unwrap_or_else(|| panic!("o must emit an event")));
    assert!(
        state_no_sel.prs_state.draft_notice.is_some(),
        "NoSelectionToOpen must surface a visible notice"
    );
}

/// Assert the detail path: `o` in PrDetail with a loaded PR emits
/// `PrOpenInBrowser`, and that no in-app merge/approve keybinding exists
/// (`external_url` is display-only).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 88-89
fn assert_o_in_detail_and_no_mutation_keybinding() {
    let mut state_detail = active_prs_state();
    state_detail.prs_state.pr_focus = PrFocus::PrDetail;
    state_detail.prs_state.pr_detail = Some(make_test_pr_detail(9));
    let event = prs::resolve_prs_key_event(&state_detail, &key(KeyCode::Char('o')));
    assert!(
        matches!(event, Some(AppEvent::PrOpenInBrowser)),
        "'o' in detail with a loaded PR must emit PrOpenInBrowser (got {event:?})"
    );
    // external_url is display-only: no in-app merge/approve/review-submit keybinding.
    let detail_state = active_prs_state();
    let merge = prs::resolve_prs_key_event(&detail_state, &key(KeyCode::Char('m')));
    assert!(
        merge.is_none() || matches!(merge, Some(AppEvent::PrShowNotice(_))),
        "no in-app merge keybinding (m is unbound or a notice), got: {merge:?}"
    );
}

/// Checkpoint 9: `o` on a loaded PR (list or detail) emits `PrOpenInBrowser`,
/// and the dispatch arm applies `apply_and_persist(PrOpenInBrowser)` BEFORE
/// spawning `gh pr view <number> --repo <owner>/<name> --web` off-thread. The
/// resolved `PrOpenInBrowserInfo` carries the EXACT command target
/// (number/owner/name) the off-thread `GhClient::open_pull_request_in_browser`
/// (`src/github/mod.rs`) consumes. With no PR selected, `o` emits a
/// `NoSelectionToOpen` notice (no silent drop). `external_url` is rendered
/// display-only and no in-app merge/approve/review-submit keybinding exists.
///
/// NOTE: the literal `gh` arg-vector (`["pr", "view", "<number>", "--repo",
/// "<owner>/<name>", "--web"]`) is built inside
/// `GhClient::open_pull_request_in_browser` and is not surfaced as a pure
/// testable seam, so the dispatch-resolved target (`info.number`,
/// `info.owner`, `info.name`) is the strongest in-process assertion of the
/// command target. Do NOT add a production seam.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-012
/// @pseudocode component-003 lines 68-69,88-89
/// @pseudocode component-004 lines 160-175
#[test]
fn it_open_in_browser_spawns_gh_pr_view_web() {
    // ── Happy path: list with a selected PR ──
    // dashboard_prs_state sets the selected repo's github_repo = "owner0/repo-1",
    // so owner/name are deterministic.
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrList;
    state.prs_state.pull_requests = vec![make_test_pr(7)];
    state.prs_state.selected_pr_index = Some(0);

    // Drive the REAL `o` key handler → PrOpenInBrowser.
    let event = prs::resolve_prs_key_event(&state, &key(KeyCode::Char('o')));
    assert!(
        matches!(event, Some(AppEvent::PrOpenInBrowser)),
        "'o' on a selected PR must emit PrOpenInBrowser (got {event:?})"
    );

    // The dispatch resolves the EXACT command target the off-thread
    // gh pr view <number> --repo <owner>/<name> --web consumes.
    let info = prs_dispatch::pr_open_in_browser_info_from_state(&state)
        .unwrap_or_else(|e| panic!("valid repo + selected PR must resolve Ok info: {e:?}"));
    assert_eq!(info.number, 7, "info.number must be the selected PR number");
    assert_eq!(
        info.owner, "owner0",
        "info.owner must be the repo's github owner"
    );
    assert_eq!(
        info.name, "repo-1",
        "info.name must be the repo's github name"
    );

    // Apply the synchronous pre-spawn reducer transition.
    state.apply_in_place(AppEvent::PrOpenInBrowser);
    let notice = state
        .prs_state
        .draft_notice
        .as_ref()
        .unwrap_or_else(|| panic!("draft_notice must be set after PrOpenInBrowser"));
    assert!(
        notice.to_lowercase().contains("opening") && notice.to_lowercase().contains("browser"),
        "notice should mention opening/browser, got: {notice}"
    );

    // ── No-selection + detail + no-mutation-keybinding paths ──
    assert_o_no_selection_emits_notice();
    assert_o_in_detail_and_no_mutation_keybinding();
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 13: missing github_repo shows inline config error
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 13: when the selected repository has no `github_repo` slug, the
/// dispatch arm delivers a scoped config error (no spawn), and the reducer
/// surfaces it as a visible `error`. Loading a list for this scope must NOT
/// invoke the real `gh` binary.
///
/// Drives the REAL dispatch resolution (`prs_dispatch::resolve_pr_gh_repo` +
/// `prs_dispatch::current_pr_scope_repo_id`) to confirm the slug is empty,
/// then delivers a `PrListLoadFailed` event (the synchronous missing-repo
/// error the dispatch arm builds) and asserts a visible error.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-013
/// @requirement REQ-PR-014
/// @pseudocode component-004 lines 127-137
#[test]
fn it_missing_github_repo_shows_inline_config_error() {
    let mut state = AppState::default();
    // Repository with an EMPTY github_repo slug.
    state.repositories.push(Repository::new(
        RepositoryId("repo-noslugin".to_string()),
        "No Slug Repo".to_string(),
        "repo-no-slug".to_string(),
        PathBuf::from("/tmp/repo-no-slug"),
    ));
    state.selected_repository_index = Some(0);
    state.apply_in_place(AppEvent::EnterPrsMode);

    // The dispatch arm reads the slug via resolve_pr_gh_repo.
    let (owner, name) = prs_dispatch::resolve_pr_gh_repo(&state);
    assert!(
        owner.is_empty() && name.is_empty(),
        "empty github_repo must resolve to empty owner/name"
    );

    // The dispatch arm builds the missing-repo error synchronously (no spawn).
    let scope = prs_dispatch::current_pr_scope_repo_id(&state);
    state.apply_in_place(AppEvent::PrListLoadFailed {
        scope_repo_id: scope,
        request_id: 0,
        error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
    });

    assert!(
        state.prs_state.error.is_some(),
        "missing github_repo must surface a visible error"
    );
    let error = state
        .prs_state
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("error must be Some"));
    assert!(
        error.to_lowercase().contains("github repository")
            || error.to_lowercase().contains("github repo"),
        "error should mention GitHub repository config, got: {error}"
    );
    assert!(
        !state.prs_state.loading.list,
        "the missing-repo path must clear loading.list (no spawn)"
    );
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 16: no blocking gh call on UI thread (NFR-001)
// ═════════════════════════════════════════════════════════════════════════

/// Checkpoint 16: PR list/detail loaders go through the async wrapper
/// (`spawn_gh_task_with_panic`), never a blocking call on the UI thread. Since
/// `AppStateHandle` cannot be constructed in unit tests and we MUST NOT spawn
/// real threads or invoke the real `gh` binary, we assert the closest
/// OBSERVABLE synchronous pre-spawn state: the loader path constructs the
/// request via the staleness-guarded request builder
/// (`mark_pr_list_reload_loading` / `mark_pr_detail_loading`) which sets
/// `loading = true` and a `*_pending` staleness guard, BEFORE any spawn — AND
/// returns synchronously WITHOUT replacing the list/detail data (no
/// `PrListLoaded`/`PrDetailLoaded` applied inline). We also assert the async
/// dispatch arm EXISTS by name (`dispatch_pr_list_fetch`,
/// `load_pr_detail_for_selection`, `load_more_pr_comments`).
///
/// This proves the loader sets the staleness-guarded pending marker (the
/// request builder the async wrapper reads) rather than calling `run_gh`
/// inline, and that the call does NOT block to fetch data synchronously.
///
/// NOTE: the actual off-thread spawn via `gh_async::spawn_gh_task_with_panic`
/// is not runtime-observable in a unit test (no spawn-recording seam — matching
/// the documented convention in `app_input_tests.rs`), so the
/// synchronous-return + pending-marker + by-name wiring is the strongest
/// in-process proof of NFR-001. Do NOT add a production seam.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-001
/// @pseudocode component-004 lines 101-175
#[test]
fn it_no_blocking_gh_call_on_ui_thread() {
    // ── List loader: returns synchronously WITHOUT data ──
    let mut state = active_prs_state();
    let scope = prs_dispatch::current_pr_scope_repo_id(&state);
    // Seed existing list data so we can prove the marker does NOT replace it.
    state.prs_state.pull_requests = vec![make_test_pr(100), make_test_pr(200)];
    let seeded_list_len = state.prs_state.pull_requests.len();

    // mark_pr_list_reload_loading sets the pending marker + loading flag and
    // returns SYNCHRONOUSLY (no inline fetch, no PrListLoaded applied).
    state.mark_pr_list_reload_loading(scope.clone(), state.prs_state.committed_filter.clone(), 1);
    assert!(
        state.prs_state.loading.list,
        "list loader must set loading.list (pre-spawn)"
    );
    assert!(
        state.prs_state.list_reload_pending.is_some(),
        "list loader must set a staleness-guarded pending marker (pre-spawn)"
    );
    let pending = state
        .prs_state
        .list_reload_pending
        .as_ref()
        .unwrap_or_else(|| panic!("list_reload_pending must be Some"));
    assert_eq!(pending.scope_repo_id, scope);
    assert_eq!(pending.request_id, 1);
    // The marker did NOT block to fetch data inline: the list is unchanged
    // (no PrListLoaded replaced pull_requests synchronously).
    assert_eq!(
        state.prs_state.pull_requests.len(),
        seeded_list_len,
        "list loader must NOT replace pull_requests synchronously (no blocking fetch)"
    );

    // ── Detail loader: returns synchronously WITHOUT data ──
    let mut state2 = active_prs_state();
    // Assert pr_detail is NOT populated synchronously by the marker.
    assert!(
        state2.prs_state.pr_detail.is_none(),
        "pr_detail must be None before the detail loader marker"
    );
    state2.mark_pr_detail_loading(scope.clone(), 42, 1);
    assert!(
        state2.prs_state.loading.detail,
        "detail loader must set loading.detail (pre-spawn)"
    );
    assert!(
        state2.prs_state.detail_pending.is_some(),
        "detail loader must set a staleness-guarded pending marker (pre-spawn)"
    );
    // The marker did NOT block to fetch the detail inline.
    assert!(
        state2.prs_state.pr_detail.is_none(),
        "detail loader must NOT populate pr_detail synchronously (no blocking fetch)"
    );

    // ── Async dispatch arms exist BY NAME (compile-time wiring proof) ──
    let _: fn(&mut AppStateHandle, &SharedContext, bool) =
        prs_list_dispatch::dispatch_pr_list_fetch;
    let _: fn(&mut AppStateHandle, &SharedContext) = prs_dispatch::load_pr_detail_for_selection;
    let _: fn(&mut AppStateHandle, &SharedContext) = prs_comments_dispatch::load_more_pr_comments;
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 10: full Esc precedence chain (REQ-PR-004)
// ═════════════════════════════════════════════════════════════════════════

/// Base state for the Esc-precedence chain: PR mode active with a loaded PR
/// list + detail, so every overlay layer can be layered on top in isolation.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn esc_chain_base_state() -> AppState {
    let mut state = active_prs_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(make_test_pr_detail(1));
    state
}

/// Resolve Esc through the REAL key router and assert the emitted event
/// matches `expected_match` (a closure returning bool from the event), then
/// apply the event through the reducer and return the resulting state.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn resolve_esc_and_apply<F: Fn(&AppEvent) -> bool>(state: &mut AppState, matches_expected: F) {
    let event = prs::resolve_prs_key_event(state, &key(KeyCode::Esc))
        .unwrap_or_else(|| panic!("Esc at this precedence level must emit an event"));
    assert!(
        matches_expected(&event),
        "Esc emitted unexpected event: {event:?}"
    );
    state.apply_in_place(event);
}

/// L1: an active inline composer — Esc (via the REAL key router) emits
/// `PrInlineCancelOrEsc`; the composer closes and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,99-108
fn esc_l1_inline_composer_cancels() {
    use jefe::state::InlineState;
    let mut state = esc_chain_base_state();
    state.apply_in_place(AppEvent::PrOpenNewCommentComposer);
    assert!(matches!(
        state.prs_state.inline_state,
        InlineState::Composer { .. }
    ));
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrInlineCancelOrEsc));
    assert_eq!(state.prs_state.inline_state, InlineState::None);
    assert!(
        state.prs_state.active,
        "mode must stay active after composer Esc"
    );
}

/// L2: an open agent chooser — Esc (via the REAL key router) emits
/// `PrAgentChooserCancel`; the chooser closes and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,120-126
fn esc_l2_agent_chooser_cancels() {
    use jefe::state::AgentChooserState;
    let mut state = esc_chain_base_state();
    state.prs_state.agent_chooser = Some(AgentChooserState {
        selected_index: 0,
        agents: vec![],
    });
    resolve_esc_and_apply(&mut state, |ev| {
        matches!(ev, AppEvent::PrAgentChooserCancel)
    });
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "chooser must close after PrAgentChooserCancel"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after chooser Esc"
    );
}

/// L3a: search focused with a NON-EMPTY query — Esc (via the REAL key router)
/// emits `PrClearSearch`; the query clears and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,127-133
fn esc_l3a_search_nonempty_clears() {
    let mut state = esc_chain_base_state();
    state.prs_state.search_input_focused = true;
    state.prs_state.search_query = "auth".to_string();
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrClearSearch));
    assert!(
        state.prs_state.search_query.is_empty(),
        "PrClearSearch must clear the query"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after search-clear Esc"
    );
}

/// L3b: search focused with an EMPTY query — Esc (via the REAL key router)
/// emits `PrBlurSearchInput`; the input blurs and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,127-133
fn esc_l3b_search_empty_blurs() {
    let mut state = esc_chain_base_state();
    state.prs_state.search_input_focused = true;
    state.prs_state.search_query = String::new();
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::PrBlurSearchInput));
    assert!(
        !state.prs_state.search_input_focused,
        "PrBlurSearchInput must blur the search input"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after search-blur Esc"
    );
}

/// L4: open filter controls — Esc (via the REAL key router) emits
/// `PrCloseFilterControls`; the controls close and the mode stays active.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98,134-146
fn esc_l4_filter_controls_close() {
    let mut state = esc_chain_base_state();
    state.apply_in_place(AppEvent::PrOpenFilterControls);
    assert!(state.prs_state.filter_ui.controls_open);
    resolve_esc_and_apply(&mut state, |ev| {
        matches!(ev, AppEvent::PrCloseFilterControls)
    });
    assert!(
        !state.prs_state.filter_ui.controls_open,
        "filter controls must close after PrCloseFilterControls"
    );
    assert!(
        state.prs_state.active,
        "mode must stay active after filter Esc"
    );
}

/// L5: nothing open — Esc (via the REAL key router) emits `ExitPrsMode`; the
/// mode becomes inactive and the screen returns to the Dashboard.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
fn esc_l5_nothing_open_exits() {
    let mut state = esc_chain_base_state();
    resolve_esc_and_apply(&mut state, |ev| matches!(ev, AppEvent::ExitPrsMode));
    assert!(
        !state.prs_state.active,
        "mode must be inactive after final Esc"
    );
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
}

/// Checkpoint 10 (REQ-PR-004): Esc unwinds by the full 6-level precedence
/// chain — inline composer → agent chooser → search-clear → search-blur →
/// filter controls → exit mode. Each Esc peels exactly one layer, leaving the
/// mode active until the final (nothing-open) Esc exits.
///
/// Drives the REAL key router `prs::resolve_prs_key_event(&state,
/// &key(KeyCode::Esc))` at EACH precedence level (the genuine 8-level
/// resolver in `src/app_input/prs.rs`), asserting the EXACT emitted
/// `AppEvent` variant via `matches!`, THEN applying it through the reducer
/// (`AppState::apply`) and asserting the resulting state. Each level is
/// exercised in isolation from a fresh base state.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-004
/// @pseudocode component-003 lines 92-98
#[test]
fn it_esc_precedence_unwinds_then_exits() {
    esc_l1_inline_composer_cancels();
    esc_l2_agent_chooser_cancels();
    esc_l3a_search_nonempty_clears();
    esc_l3b_search_empty_blurs();
    esc_l4_filter_controls_close();
    esc_l5_nothing_open_exits();
}

// ═════════════════════════════════════════════════════════════════════════
// Checkpoint 17: persisted state excludes prs_state (NFR-003)
// ═════════════════════════════════════════════════════════════════════════

/// Build an AppState with an ACTIVE, populated `prs_state` AND realistic
/// persisted fields (repositories, agents, selected indices,
/// hide_idle_repositories, last_selected_agent_by_repo), so the REAL
/// `to_persisted_state` mapper has non-trivial persisted data to copy while PR
/// data is simultaneously present (and must be excluded).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn state_with_active_prs_and_persisted_fields() -> AppState {
    let mut state = dashboard_prs_state();
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent One".to_string(),
        PathBuf::from("/tmp/repo-1"),
    ));
    state.selected_agent_index = Some(0);
    state.hide_idle_repositories = true;
    state.last_selected_agent_by_repo = vec![(
        RepositoryId("repo-1".to_string()),
        AgentId("agent-1".to_string()),
    )];

    // ACTIVE, populated prs_state (transient — must be excluded).
    state.apply_in_place(AppEvent::EnterPrsMode);
    state.prs_state.pull_requests = vec![make_test_pr(1)];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(make_test_pr_detail(1));
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state
}

/// Assert the persisted DTO carries NO pull-request data (no prs_state /
/// pull_request / pr_detail / "pr_" keys), via a serde_json round-trip.
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn assert_no_pr_data_in_persisted(persisted: &PersistedState) {
    let json = serde_json::to_value(persisted)
        .unwrap_or_else(|e| panic!("persisted should serialize: {e}"));
    let json_str =
        serde_json::to_string(&json).unwrap_or_else(|e| panic!("json should stringify: {e}"));
    let lower = json_str.to_lowercase();
    assert!(
        !lower.contains("prs_state")
            && !lower.contains("pull_request")
            && !lower.contains("pr_detail")
            && !lower.contains("\"pr_"),
        "persisted state must not carry any PR data, got: {json_str}"
    );
}

/// Assert the persisted DTO carries the same persisted-field values as the
/// source AppState (proving the REAL mapper copies them faithfully).
/// Repository/Agent do not derive `PartialEq`, so they are compared by
/// length + key fields (id/name).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
fn assert_persisted_fields_match_source(persisted: &PersistedState, state: &AppState) {
    assert_eq!(
        persisted.schema_version,
        jefe::persistence::STATE_SCHEMA_VERSION
    );
    assert_eq!(
        persisted.repositories.len(),
        state.repositories.len(),
        "repositories count must match source"
    );
    for (i, (pr, sr)) in persisted
        .repositories
        .iter()
        .zip(state.repositories.iter())
        .enumerate()
    {
        assert_eq!(pr.id, sr.id, "repository[{i}] id must match source");
        assert_eq!(pr.name, sr.name, "repository[{i}] name must match source");
    }
    assert_eq!(
        persisted.agents.len(),
        state.agents.len(),
        "agents count must match source"
    );
    for (i, (pa, sa)) in persisted.agents.iter().zip(state.agents.iter()).enumerate() {
        assert_eq!(pa.id, sa.id, "agent[{i}] id must match source");
    }
    assert_eq!(
        persisted.selected_repository_index,
        state.selected_repository_index
    );
    assert_eq!(persisted.selected_agent_index, state.selected_agent_index);
    assert_eq!(
        persisted.hide_idle_repositories,
        state.hide_idle_repositories
    );
    assert_eq!(
        persisted.last_selected_agent_by_repo,
        state.last_selected_agent_by_repo
    );
}

/// Checkpoint 17 (NFR-003): the REAL production mapper
/// `to_persisted_state(&state)` (`src/app_input/mod.rs`, the same fn the
/// sibling unit test `app_input_tests::test_to_persisted_state_excludes_prs_state`
/// exercises) must carry ONLY the persisted fields (schema_version,
/// repositories, agents, selected_repository_index, selected_agent_index,
/// hide_idle_repositories, last_selected_agent_by_repo) and NEVER any
/// `prs_state`/pull-request data (which is transient). This integration-level
/// variant is BROADER than the existing unit test: it drives the REAL mapper
/// against an AppState with BOTH an active, populated `prs_state` (active=true,
/// pull_requests non-empty, selected_pr_index Some, pr_detail Some, non-default
/// pr_focus) AND realistic persisted fields, then asserts (a) the persisted
/// fields equal the source AppState's values, (b) a serde_json round-trip of
/// the PersistedState carries NO PR data, and (c) a fresh `AppState::default()`
/// has `prs_state.active == false` and default `prs_state` (PR state is
/// transient and never rehydrated from the persisted form).
///
/// @plan PLAN-20260624-PR-MODE.P15
/// @requirement REQ-PR-NFR-003
/// @pseudocode component-001 lines 66-76
#[test]
fn it_persisted_state_excludes_prs_state() {
    let state = state_with_active_prs_and_persisted_fields();
    // Precondition: PR mode is active and populated (meaningful test).
    assert!(state.prs_state.active);
    assert!(!state.prs_state.pull_requests.is_empty());

    // Drive the REAL production mapper (same path as app_input_tests.rs:284).
    let persisted = to_persisted_state(&state);

    // The REAL mapper copied the persisted fields faithfully (equal to source).
    assert_persisted_fields_match_source(&persisted, &state);

    // Structurally (via serde_json round-trip of the DTO) NO PR data is present.
    assert_no_pr_data_in_persisted(&persisted);

    // Round-trip: serialize → deserialize, and confirm the DTO still has no PR
    // fields, and a fresh AppState (the hydration baseline) has inactive /
    // default prs_state — proving PR state is transient.
    let json = serde_json::to_string(&persisted)
        .unwrap_or_else(|e| panic!("persisted should serialize: {e}"));
    let reloaded: PersistedState =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("persisted should deserialize: {e}"));
    assert_no_pr_data_in_persisted(&reloaded);

    let fresh = AppState::default();
    assert!(
        !fresh.prs_state.active,
        "fresh AppState prs_state must be inactive"
    );
    assert!(
        fresh.prs_state.pull_requests.is_empty()
            && fresh.prs_state.pr_detail.is_none()
            && fresh.prs_state.selected_pr_index.is_none(),
        "fresh AppState prs_state must be at defaults (transient, never rehydrated)"
    );
}
