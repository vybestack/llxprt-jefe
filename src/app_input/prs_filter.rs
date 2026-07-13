//! Filter-controls key routing for PR Mode.
//!
//! Implements the eight-field filter model: 0 state, 1 draft, 2 review-decision,
//! 3 checks-status, 4 author, 5 assignee, 6 reviewer, 7 labels. Cycle fields
//! (0-3) advance on Space; text fields (4-7) accept char/backspace input.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-008
//! @pseudocode component-003 lines 134-146

use iocraft::prelude::*;

use jefe::state::{AppEvent, AppState};

use super::filter_controls::{FilterControlCommand, FilterEditorKind, resolve_filter_control_key};

/// The eight filter fields indexed by `filter_ui.field_index`.
/// State field (index 0) is the default for Space cycling.
const DRAFT_FIELD: usize = 1;
const REVIEW_FIELD: usize = 2;
const CHECKS_FIELD: usize = 3;
const AUTHOR_FIELD: usize = 4;
const ASSIGNEE_FIELD: usize = 5;
const REVIEWER_FIELD: usize = 6;
const LABELS_FIELD: usize = 7;

/// Resolve a key event while PR filter controls are open.
///
/// Tab/BackTab navigate fields; Enter applies; Esc closes; Ctrl-c clears; Space
/// cycles the active cycle field (state/draft/review/checks); text chars and
/// Backspace edit the active text field (author/assignee/reviewer/labels).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-003 lines 134-146
pub(super) fn handle_pr_filter_controls_key(
    state: &AppState,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    let field_idx = state.prs_state.filter_ui.field_index;
    let editor = if is_text_field(field_idx) {
        FilterEditorKind::Text
    } else {
        FilterEditorKind::Cycle
    };
    match resolve_filter_control_key(editor, key_event)? {
        FilterControlCommand::Apply => Some(AppEvent::PrApplyFilter),
        FilterControlCommand::Cancel => Some(AppEvent::PrCloseFilterControls),
        FilterControlCommand::Next => Some(AppEvent::PrFilterNavigateNext),
        FilterControlCommand::Previous => Some(AppEvent::PrFilterNavigatePrev),
        FilterControlCommand::ClearAll => Some(AppEvent::PrClearFilter),
        FilterControlCommand::ClearCurrent if is_text_field(field_idx) => {
            Some(text_clear_event(state, field_idx))
        }
        FilterControlCommand::CycleNext | FilterControlCommand::CyclePrevious => {
            Some(space_event_for_field(field_idx))
        }
        FilterControlCommand::Append(c) => Some(text_char_event(state, field_idx, c)),
        FilterControlCommand::Backspace => Some(text_backspace_event(state, field_idx)),
        FilterControlCommand::ClearCurrent => None,
    }
}

/// Whether the given field index is a text-input field.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
fn is_text_field(field_idx: usize) -> bool {
    matches!(
        field_idx,
        AUTHOR_FIELD | ASSIGNEE_FIELD | REVIEWER_FIELD | LABELS_FIELD
    )
}

/// Map a Space press on a cycle field to the matching cycle event.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
fn space_event_for_field(field_idx: usize) -> AppEvent {
    match field_idx {
        DRAFT_FIELD => AppEvent::PrCycleDraftFilter,
        REVIEW_FIELD => AppEvent::PrCycleReviewFilter,
        CHECKS_FIELD => AppEvent::PrCycleChecksFilter,
        // STATE_FIELD and any unexpected index default to state cycling.
        _ => AppEvent::PrCycleFilterState,
    }
}

/// Append a char to the active text field and emit a `PrUpdateDraftFilter`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
fn text_char_event(state: &AppState, field_idx: usize, c: char) -> AppEvent {
    let (field, value) = text_field_value(state, field_idx);
    let mut value = value;
    value.push(c);
    AppEvent::PrUpdateDraftFilter { field, value }
}

/// Pop the last char from the active text field and emit a `PrUpdateDraftFilter`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
fn text_backspace_event(state: &AppState, field_idx: usize) -> AppEvent {
    let (field, mut value) = text_field_value(state, field_idx);
    value.pop();
    AppEvent::PrUpdateDraftFilter { field, value }
}

fn text_clear_event(state: &AppState, field_idx: usize) -> AppEvent {
    let (field, _) = text_field_value(state, field_idx);
    AppEvent::PrUpdateDraftFilter {
        field,
        value: String::new(),
    }
}

/// Read the (field_name, current_value) for the active text field.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-008
/// @pseudocode component-001 lines 249-251
fn text_field_value(state: &AppState, field_idx: usize) -> (String, String) {
    match field_idx {
        AUTHOR_FIELD => (
            "author".to_string(),
            state.prs_state.draft_filter.author.clone(),
        ),
        ASSIGNEE_FIELD => (
            "assignee".to_string(),
            state.prs_state.draft_filter.assignee.clone(),
        ),
        REVIEWER_FIELD => (
            "reviewer".to_string(),
            state.prs_state.draft_filter.reviewer.clone(),
        ),
        LABELS_FIELD => (
            "labels".to_string(),
            state.prs_state.filter_ui.draft_labels_text.clone(),
        ),
        _ => (String::new(), String::new()),
    }
}
