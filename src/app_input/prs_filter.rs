//! Filter-controls key routing for PR Mode.
//!
//! Implements the ten-field filter+sort model: 0 state, 1 draft, 2
//! review-decision, 3 checks-status, 4 author, 5 assignee, 6 reviewer, 7
//! labels, 8 sort_by, 9 sort_order (issue #473). Cycle fields (0-3, 8-9)
//! advance on Space/arrow; text fields (4-7) accept char/backspace input.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-008
//! @pseudocode component-003 lines 134-146

use iocraft::prelude::*;

use jefe::domain::action_registry::HandlerKey;
use jefe::state::{AppEvent, AppState};

use super::filter_controls::{FilterTextMutation, resolve_filter_text_mutation};

/// The filter fields indexed by `filter_ui.field_index`.
const DRAFT_FIELD: usize = 1;
const REVIEW_FIELD: usize = 2;
const CHECKS_FIELD: usize = 3;
const AUTHOR_FIELD: usize = 4;
const ASSIGNEE_FIELD: usize = 5;
const REVIEWER_FIELD: usize = 6;
const LABELS_FIELD: usize = 7;
/// Sort-by field index (issue #473).
const SORT_BY_FIELD: usize = 8;
/// Sort-order field index (issue #473).
const SORT_ORDER_FIELD: usize = 9;

/// Resolve raw text editing while PR filter controls are open.
/// @requirement REQ-PR-008
pub(super) fn resolve_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let field_idx = state.prs_state.filter_ui.field_index;
    match resolve_filter_text_mutation(is_text_field(field_idx), key_event)? {
        FilterTextMutation::Append(character) => Some(text_char_event(state, field_idx, character)),
        FilterTextMutation::Backspace => Some(text_backspace_event(state, field_idx)),
    }
}

/// Translate a resolved registry filter control into the matching PR event.
///
/// The typed handler is already authoritative here, so this planner does not
/// inspect the originating key event or perform a second resolution.
pub(super) fn control_event(state: &AppState, handler: HandlerKey) -> Option<AppEvent> {
    let field_idx = state.prs_state.filter_ui.field_index;
    match handler {
        HandlerKey::FilterApply => Some(AppEvent::PrApplyFilter),
        HandlerKey::FilterCancel => Some(AppEvent::PrCloseFilterControls),
        HandlerKey::FilterNextField => Some(AppEvent::PrFilterNavigateNext),
        HandlerKey::FilterPreviousField => Some(AppEvent::PrFilterNavigatePrev),
        HandlerKey::FilterClearAll => Some(AppEvent::PrClearFilter),
        HandlerKey::FilterClearCurrent if is_text_field(field_idx) => {
            Some(text_clear_event(state, field_idx))
        }
        HandlerKey::FilterPreviousChoice | HandlerKey::FilterNextChoice => {
            Some(space_event_for_field(field_idx))
        }
        _ => None,
    }
}

/// Whether the given field index is a text-input field.
fn is_text_field(field_idx: usize) -> bool {
    matches!(
        field_idx,
        AUTHOR_FIELD | ASSIGNEE_FIELD | REVIEWER_FIELD | LABELS_FIELD
    )
}

/// Whether the given field index is a sort field (issue #473).
#[cfg(test)]
const fn is_sort_field(field_idx: usize) -> bool {
    field_idx == SORT_BY_FIELD || field_idx == SORT_ORDER_FIELD
}

/// Map a Space/arrow press on a cycle field to the matching cycle event.
fn space_event_for_field(field_idx: usize) -> AppEvent {
    match field_idx {
        DRAFT_FIELD => AppEvent::PrCycleDraftFilter,
        REVIEW_FIELD => AppEvent::PrCycleReviewFilter,
        CHECKS_FIELD => AppEvent::PrCycleChecksFilter,
        SORT_BY_FIELD => AppEvent::PrCycleSortByNext,
        SORT_ORDER_FIELD => AppEvent::PrToggleSortOrder,
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

#[cfg(test)]
mod sort_field_tests {
    use super::*;

    #[test]
    fn sort_by_field_index_is_sort_field() {
        assert!(is_sort_field(SORT_BY_FIELD));
    }

    #[test]
    fn sort_order_field_index_is_sort_field() {
        assert!(is_sort_field(SORT_ORDER_FIELD));
    }

    #[test]
    fn non_sort_fields_are_not_sort_fields() {
        assert!(!is_sort_field(0));
        assert!(!is_sort_field(AUTHOR_FIELD));
    }

    #[test]
    fn sort_by_field_routes_to_cycle_next() {
        assert!(matches!(
            space_event_for_field(SORT_BY_FIELD),
            AppEvent::PrCycleSortByNext
        ));
    }

    #[test]
    fn sort_order_field_routes_to_toggle_order() {
        assert!(matches!(
            space_event_for_field(SORT_ORDER_FIELD),
            AppEvent::PrToggleSortOrder
        ));
    }

    #[test]
    fn state_field_routes_to_state_cycle() {
        assert!(matches!(
            space_event_for_field(0),
            AppEvent::PrCycleFilterState
        ));
    }

    #[test]
    fn draft_field_routes_to_draft_cycle() {
        assert!(matches!(
            space_event_for_field(DRAFT_FIELD),
            AppEvent::PrCycleDraftFilter
        ));
    }
}
