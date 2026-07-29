//! Home/End cursor movement for form text fields (issue #406).
//!
//! Extracted from `form_ops.rs` to stay within the per-file line budget.

use super::AppState;
use super::types::ModalState;

impl AppState {
    /// Move the focused form-field cursor to the start of the field
    /// (Home key, issue #406).
    pub(super) fn handle_form_move_cursor_start(&mut self) {
        match &mut self.modal {
            ModalState::NewRepository { focus, cursor, .. }
            | ModalState::EditRepository { focus, cursor, .. } => {
                crate::state::form_cursor::move_repository_field_cursor_start(cursor, *focus);
            }
            ModalState::NewAgent { focus, cursor, .. }
            | ModalState::EditAgent { focus, cursor, .. } => {
                crate::state::form_cursor::move_agent_field_cursor_start(cursor, *focus);
            }
            ModalState::WorkflowDispatch { focus, cursor, .. } => {
                crate::state::form_workflow_dispatch::move_cursor_field_start(cursor, *focus);
            }
            _ => {}
        }
    }

    /// Move the focused form-field cursor to the end of the field
    /// (End key, issue #406).
    pub(super) fn handle_form_move_cursor_end(&mut self) {
        if self.handle_generated_form_intent(
            super::generated_agent_form::GeneratedAgentFormIntent::CursorEnd,
        ) {
            return;
        }
        match &mut self.modal {
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => {
                crate::state::form_cursor::move_repository_field_cursor_end(fields, cursor, *focus);
            }
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                ..
            }
            | ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => crate::state::form_cursor::move_agent_field_cursor_end(fields, cursor, *focus),
            ModalState::WorkflowDispatch {
                fields,
                focus,
                cursor,
                ..
            } => {
                crate::state::form_workflow_dispatch::move_cursor_field_end(fields, cursor, *focus);
            }
            _ => {}
        }
    }
}
