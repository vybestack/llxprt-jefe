//! Pull Requests Mode component tests — ShowNotice (ReadOnlyHintKind),
//! open-in-browser reducer purity, open-in-browser-failed, and the three
//! AppEvent↔PullRequestsMessage conversion round-trip tests.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-002
//! @requirement REQ-PR-010
//! @requirement REQ-PR-012
//! @requirement REQ-PR-013

use crate::domain::{PrCheckStatus, PrState, PullRequest, Repository, RepositoryId};
use crate::messages::{AppMessage, PullRequestsMessage};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{ReadOnlyHintKind, ScreenMode};

/// Helper: PR-mode state with a selected PR.
fn prs_mode_state_with_selected_pr(repo_id: &str, pr_number: u64) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state.prs_state.list.replace_items(vec![PullRequest {
        number: pr_number,
        title: format!("PR #{pr_number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }]);
    state.prs_state.list.set_selected_index(Some(0));
    state
}

/// PrShowNotice must set prs_state.draft_notice = Some(text) for EACH
/// ReadOnlyHintKind variant and the message must be handled (routed through
/// the real apply_message hub → apply_prs_message).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 344-348
#[test]
fn test_show_notice_sets_draft_notice_for_each_readonly_hint_kind() {
    for kind in [
        ReadOnlyHintKind::ReadOnlyReplyOnComment,
        ReadOnlyHintKind::ReadOnlyNoComment,
        ReadOnlyHintKind::ReadOnlyNotEditable,
        ReadOnlyHintKind::NoSelectionToOpen,
    ] {
        let mut state = AppState::default();
        state.prs_state.active = true;

        // Drive through the REAL dispatch hub (apply_message → apply_prs_message)
        // so the test proves the runtime path is wired, not just the reducer.
        state = state.apply_message(AppMessage::PullRequests(PullRequestsMessage::ShowNotice(
            kind,
        )));

        // Handled is proven by the observable effect: a handled ShowNotice sets
        // a non-empty draft_notice. A no-op/unrouted message would leave it None.
        assert!(
            state.prs_state.draft_notice.is_some(),
            "ShowNotice({kind:?}) routed through apply_message must set a non-empty draft_notice"
        );
        let notice = state
            .prs_state
            .draft_notice
            .clone()
            .unwrap_or_else(|| panic!("draft_notice must be Some for {kind:?}"));
        assert!(
            !notice.is_empty(),
            "notice text must be non-empty for {kind:?}"
        );
    }
}

/// PrOpenInBrowser on a state WITH a selected PR must set an "opening…" notice
/// (not a NoSelectionToOpen notice), perform NO I/O / no list/detail mutation,
/// and be handled (routed through the real apply_message hub → apply_prs_message).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 349-357
#[test]
fn test_open_in_browser_reducer_is_pure_sets_opening_notice() {
    // WITH a selected PR.
    let state = prs_mode_state_with_selected_pr("repo-1", 7);
    let detail_was_present = state.prs_state.pr_detail.is_some();
    let list_len = state.prs_state.pull_requests().len();
    let selection_snapshot = state.prs_state.selected_pr_index();

    // Drive through the REAL dispatch hub (apply_message → apply_prs_message).
    let state = state.apply_message(AppMessage::PullRequests(PullRequestsMessage::OpenInBrowser));

    // Handled is proven by the observable effect: a handled OpenInBrowser sets
    // an opening notice. A no-op/unrouted message would leave it None.
    let notice = state
        .prs_state
        .draft_notice
        .clone()
        .unwrap_or_else(|| panic!("opening notice must be Some"));
    assert!(
        notice.to_lowercase().contains("opening") || notice.to_lowercase().contains("browser"),
        "notice should mention opening/browser, got: {notice}"
    );
    // Pure: no I/O, no list/detail mutation (counts and selection preserved).
    assert_eq!(state.prs_state.pull_requests().len(), list_len);
    assert_eq!(state.prs_state.pr_detail.is_some(), detail_was_present);
    assert_eq!(state.prs_state.selected_pr_index(), selection_snapshot);
}

/// PrOpenInBrowser WITHOUT a selected PR must set a NoSelectionToOpen-style
/// notice and be handled (routed through the real apply_message hub).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 349-357
#[test]
fn test_open_in_browser_no_selection_sets_notice() {
    let mut state = AppState::default();
    state.prs_state.active = true;
    state.prs_state.list.set_selected_index(None);
    state.prs_state.list.clear_items();

    // Drive through the REAL dispatch hub (apply_message → apply_prs_message).
    state = state.apply_message(AppMessage::PullRequests(PullRequestsMessage::OpenInBrowser));

    // Handled is proven by the observable effect: NoSelectionToOpen sets a
    // notice (no silent drop). A no-op/unrouted message would leave it None.
    assert!(
        state.prs_state.draft_notice.is_some(),
        "NoSelectionToOpen routed through apply_message must set a notice (no silent drop)"
    );
}

/// PrOpenInBrowserFailed must set a scoped error notice (no silent drop),
/// routed through the real apply_message hub → apply_prs_message.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-012
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 362-365
#[test]
fn test_open_in_browser_failed_sets_scoped_error_notice() {
    let state = prs_mode_state_with_selected_pr("repo-1", 3);

    // Drive through the REAL dispatch hub (apply_message → apply_prs_message).
    let state = state.apply_message(AppMessage::PullRequests(
        PullRequestsMessage::OpenInBrowserFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 3,
            error: "browser launch failed".to_string(),
        },
    ));

    // Handled is proven by the observable effect: a scoped error notice is set.
    assert!(
        state.prs_state.error.is_some() || state.prs_state.draft_notice.is_some(),
        "OpenInBrowserFailed routed through apply_message must surface a scoped error notice (no silent drop)"
    );
}

// =============================================================================
// Message↔Event conversion round-trip tests (finding #1)
//
// The round-trip is exercised through the PUBLIC `From<AppEvent> for AppMessage`
// and `From<AppMessage> for AppEvent` impls (the `from_app_event` / `into_app_event`
// helpers are `pub(super)` and not callable from external test modules). For a
// PR `AppEvent` E, the invariant is:
//   AppEvent::from(AppMessage::from(E)) == E
// because `AppMessage::from(E)` routes PR events through
// `from_prs_event -> PullRequestsMessage::from_app_event(E)` and
// `AppEvent::from(AppMessage::PullRequests(m))` calls `m.into()` (the
// `From<PullRequestsMessage> for AppEvent` impl in prs_conversion.rs).
// =============================================================================

/// PrShowNotice(kind) ↔ PullRequestsMessage::ShowNotice(kind) round-trips and
/// apply_prs_message sets draft_notice (no-silent-drop pipeline proof).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-013
/// @pseudocode component-004 lines 27,62,81
#[test]
fn test_pr_show_notice_round_trips_and_sets_draft_notice() {
    let kind = ReadOnlyHintKind::ReadOnlyNotEditable;
    let event = AppEvent::PrShowNotice(kind);

    // Round-trip via the public AppMessage conversion path.
    let message: AppMessage = event.into();
    let round_trip: AppEvent = message.into();
    assert!(
        matches!(round_trip, AppEvent::PrShowNotice(k) if k == kind),
        "PrShowNotice should round-trip through AppMessage, got {round_trip:?}"
    );

    // The reducer (apply_prs_message) must set draft_notice.
    let mut state = AppState::default();
    state.prs_state.active = true;
    let handled = state.apply_prs_message(PullRequestsMessage::ShowNotice(kind));
    assert!(handled);
    assert!(state.prs_state.draft_notice.is_some());
}

/// PrOpenInBrowser / PrOpenedInBrowser / PrOpenInBrowserFailed ↔ the matching
/// PullRequestsMessage variants round-trip.
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 32-34,63-65,82-83
#[test]
fn test_open_in_browser_events_round_trip() {
    let scope = RepositoryId("repo-1".to_string());

    // OpenInBrowser.
    let message: AppMessage = AppEvent::PrOpenInBrowser.into();
    let round_trip: AppEvent = message.into();
    assert!(
        matches!(round_trip, AppEvent::PrOpenInBrowser),
        "PrOpenInBrowser should round-trip, got {round_trip:?}"
    );

    // OpenedInBrowser.
    let message: AppMessage = AppEvent::PrOpenedInBrowser {
        scope_repo_id: scope.clone(),
        pr_number: 5,
    }
    .into();
    let round_trip: AppEvent = message.into();
    let rt_debug = format!("{round_trip:?}");
    assert!(
        rt_debug.contains("PrOpenedInBrowser")
            && rt_debug.contains("repo-1")
            && rt_debug.contains("pr_number: 5"),
        "PrOpenedInBrowser should round-trip, got {rt_debug}"
    );

    // OpenInBrowserFailed.
    let message: AppMessage = AppEvent::PrOpenInBrowserFailed {
        scope_repo_id: scope.clone(),
        pr_number: 5,
        error: "boom".to_string(),
    }
    .into();
    let round_trip: AppEvent = message.into();
    assert!(
        matches!(round_trip, AppEvent::PrOpenInBrowserFailed { pr_number: 5, ref error, .. } if error == "boom"),
        "PrOpenInBrowserFailed should round-trip, got {round_trip:?}"
    );
}

fn assert_pr_event_round_trip(original: &AppEvent) {
    let message: AppMessage = original.clone().into();
    let round_trip: AppEvent = message.into();
    let orig_debug = format!("{original:?}");
    let rt_debug = format!("{round_trip:?}");
    assert_eq!(orig_debug, rt_debug, "round-trip failed");
}

/// AppEvent::from(AppMessage::from(E)) structurally matches E for sampled PR
/// AppEvents (the round-trip invariant, REQ-PR-002).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-002
/// @pseudocode component-004 round-trip invariant
#[test]
fn test_appevent_pullrequestsmessage_round_trip() {
    // Unit variants — round-trip must yield the same variant.
    let mut unit_samples: Vec<_> = std::iter::once(AppEvent::EnterPrsMode).collect();
    macro_rules! push_events {
        ($($event:expr),+ $(,)?) => {
            $(unit_samples.push($event);)+
        };
    }
    push_events!(
        AppEvent::ExitPrsMode,
        AppEvent::RefocusPrList,
        AppEvent::PrNavigateUp,
        AppEvent::PrNavigateDown,
        AppEvent::PrListEnter,
        AppEvent::PrCycleFocus,
        AppEvent::PrCycleFocusReverse,
        AppEvent::PrOpenFilterControls,
        AppEvent::PrCloseFilterControls,
        AppEvent::PrApplyFilter,
        AppEvent::PrClearFilter,
        AppEvent::PrFilterNavigateNext,
        AppEvent::PrFilterNavigatePrev,
        AppEvent::PrCycleFilterState,
        AppEvent::PrCycleDraftFilter,
        AppEvent::PrCycleReviewFilter,
        AppEvent::PrCycleChecksFilter,
        AppEvent::PrFocusSearchInput,
        AppEvent::PrBlurSearchInput,
        AppEvent::PrApplySearch,
        AppEvent::PrClearSearch,
        AppEvent::PrOpenNewCommentComposer,
        AppEvent::PrScrollDetailUp,
        AppEvent::PrScrollDetailDown,
        AppEvent::PrDetailSubfocusNext,
        AppEvent::PrDetailSubfocusPrev,
        AppEvent::PrOpenAgentChooser {
            metadata: vec![crate::domain::AgentChooserGitMetadata::for_agent(
                crate::domain::AgentId("agent-1".to_string()),
            )],
        },
        AppEvent::PrAgentChooserNavigateUp,
        AppEvent::PrAgentChooserNavigateDown,
        AppEvent::PrAgentChooserConfirm,
        AppEvent::PrAgentChooserCancel,
        AppEvent::PrSendToAgentCompleted,
        AppEvent::PrOpenInBrowser,
    );

    for original in &unit_samples {
        assert_pr_event_round_trip(original);
    }

    // ShowNotice carries a kind — verify the kind survives the round-trip.
    let kind = ReadOnlyHintKind::ReadOnlyReplyOnComment;
    let message: AppMessage = AppEvent::PrShowNotice(kind).into();
    let round_trip: AppEvent = message.into();
    assert!(
        matches!(round_trip, AppEvent::PrShowNotice(k) if k == kind),
        "PrShowNotice kind must survive round-trip, got {round_trip:?}"
    );
}
