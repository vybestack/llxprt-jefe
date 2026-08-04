//! Todo-window projection for the workbench card (issues #626, #625).
//!
//! Two questions are answered here and they are deliberately separate. What is
//! *true* about an item comes from the producer: its published task state
//! decides the checkbox and whether it is marked as being worked on. What is
//! *visible* is a viewport choice: which slice of a long list to show claims
//! nothing about the items in it.
//!
//! The window is fixed height and blank-padded, so every card occupies the same
//! number of lines whatever its list length.

use crate::domain::observation::{
    AgentObservation, Availability, FieldState, TodoItem, TodoList, TodoState,
};
use crate::list_viewport::fit_text_to_width;

use super::{TodoLine, TodoRender, TodoWindow};

/// Resolve todo rendering with field-state honesty.
pub(super) fn render_todos(
    observation: Option<&AgentObservation>,
    window: usize,
    interior: usize,
) -> TodoRender {
    let Some(observation) = observation else {
        return TodoRender::Unsupported;
    };
    match &observation.todos {
        FieldState::Unsupported => TodoRender::Unsupported,
        FieldState::Supported {
            availability: Availability::Unknown,
            ..
        } => TodoRender::Unknown,
        FieldState::Supported {
            availability: Availability::Known(list),
            ..
        }
        | FieldState::Supported {
            availability:
                Availability::Degraded {
                    last_value: list, ..
                },
            ..
        } => TodoRender::Known(window_todos(list, window, interior)),
    }
}

/// Build the windowed todo slice with counter independence and blank padding.
fn window_todos(list: &TodoList, window: usize, interior: usize) -> TodoWindow {
    let items = &list.items;
    let total = items.len();
    let done = items
        .iter()
        .filter(|t| t.state == TodoState::Completed)
        .count();
    let (start, current_global) = todo_window_start(items, window);
    let current_visible = current_global.and_then(|g| {
        if g >= start && g < start.saturating_add(window) {
            Some(g.saturating_sub(start))
        } else {
            None
        }
    });
    let mut visible = Vec::with_capacity(window);
    for slot in 0..window {
        let global = start.saturating_add(slot);
        if global < total {
            let item = &items[global];
            // Every item the producer calls in-progress is marked, not just
            // the anchor: an agent working several strands at once is working
            // on all of them, and each marker is something it said (#625).
            let is_current = item.state == TodoState::InProgress;
            let prefix = if is_current { "▸" } else { " " };
            let line = format!(
                "{prefix}[{}] {}",
                todo_state_marker(item.state),
                item.text.as_str()
            );
            visible.push(TodoLine {
                text: fit_text_to_width(&line, interior),
                is_current,
                is_blank: false,
            });
        } else {
            visible.push(TodoLine {
                text: String::new(),
                is_current: false,
                is_blank: true,
            });
        }
    }
    TodoWindow {
        visible,
        done,
        total,
        current: current_visible,
    }
}

/// The checkbox marker for a published task state.
const fn todo_state_marker(state: TodoState) -> &'static str {
    match state {
        TodoState::Completed => "x",
        TodoState::InProgress => ">",
        TodoState::Pending => " ",
        TodoState::Unrecognized => "?",
    }
}

/// Compute the window start index and the active item's global index.
///
/// The active item is the one the producer published as in progress. When
/// nothing is in progress there is no active item, because an unfinished entry
/// is not evidence that anybody is working on it (#625).
///
/// Scrolling is a separate question from truth. With no active item the window
/// still anchors on the first unfinished entry, which is only a choice of which
/// slice to show and marks nothing.
fn todo_window_start(items: &[TodoItem], window: usize) -> (usize, Option<usize>) {
    let len = items.len();
    let current = items.iter().position(|t| t.state == TodoState::InProgress);
    let anchor = current.or_else(|| items.iter().position(|t| t.state != TodoState::Completed));
    let start = match anchor {
        None => len.saturating_sub(window),
        Some(open) => open.saturating_sub(1).min(len.saturating_sub(window)),
    };
    (start, current)
}
