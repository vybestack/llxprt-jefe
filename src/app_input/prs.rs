//! Raw Pull Request editor, search, property, and filter mutation routing.

use iocraft::prelude::{KeyCode, KeyEvent};
#[cfg(test)]
use iocraft::prelude::{KeyEventKind, KeyModifiers};
#[cfg(test)]
use jefe::state::PrFocus;
use jefe::state::{AppEvent, AppState, InlineState, PrDetailSubfocus, ReadOnlyHintKind};

#[must_use]
pub(super) fn resolve_raw_key(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    if state.prs_state.inline_state != InlineState::None {
        return resolve_inline_text(key_event);
    }
    if let Some(editor) = state.prs_state.property_editor.as_ref() {
        return resolve_property_text(editor.kind, key_event);
    }
    if state.prs_state.search_input_focused {
        return resolve_search_text(state, key_event);
    }
    if state.prs_state.filter_ui.controls_open {
        return super::prs_filter::resolve_raw_key(state, key_event);
    }
    None
}

fn resolve_inline_text(key_event: &KeyEvent) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Enter if key_event.modifiers.is_empty() => Some(AppEvent::PrInlineNewline),
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(AppEvent::PrInlineChar(character))
        }
        KeyCode::Backspace => Some(AppEvent::PrInlineBackspace),
        KeyCode::Delete => Some(AppEvent::PrInlineDelete),
        KeyCode::Left => Some(AppEvent::PrInlineCursorLeft),
        KeyCode::Right => Some(AppEvent::PrInlineCursorRight),
        KeyCode::Up => Some(AppEvent::PrInlineCursorUp),
        KeyCode::Down => Some(AppEvent::PrInlineCursorDown),
        KeyCode::Home => Some(AppEvent::PrInlineCursorHome),
        KeyCode::End => Some(AppEvent::PrInlineCursorEnd),
        _ => None,
    }
}

fn resolve_property_text(
    kind: jefe::state::PrPropertyKind,
    key_event: &KeyEvent,
) -> Option<AppEvent> {
    match key_event.code {
        KeyCode::Char(' ') if kind != jefe::state::PrPropertyKind::Title => None,
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(AppEvent::PrPropertyEditorTitleChar(character))
        }
        KeyCode::Backspace => Some(AppEvent::PrPropertyEditorTitleBackspace),
        KeyCode::Delete => Some(AppEvent::PrPropertyEditorTitleDelete),
        KeyCode::Left => Some(AppEvent::PrPropertyEditorTitleCursorLeft),
        KeyCode::Right => Some(AppEvent::PrPropertyEditorTitleCursorRight),
        KeyCode::Home => Some(AppEvent::PrPropertyEditorTitleCursorHome),
        KeyCode::End => Some(AppEvent::PrPropertyEditorTitleCursorEnd),
        _ => None,
    }
}

fn resolve_search_text(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    let mut query = state.prs_state.search_query.clone();
    match key_event.code {
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            query.push(character);
        }
        KeyCode::Backspace => {
            query.pop();
        }
        _ => return None,
    }
    Some(AppEvent::PrSetSearchQuery { query })
}

pub(super) fn pr_to_actions_event(state: &AppState) -> AppEvent {
    let selected_pr = state
        .prs_state
        .selected_pr_index()
        .and_then(|index| state.prs_state.pull_requests().get(index));
    if let Some(pr) = selected_pr {
        let head_sha = state
            .prs_state
            .pr_detail
            .as_ref()
            .filter(|detail| detail.number == pr.number)
            .map_or_else(|| pr.head_sha.clone(), |detail| detail.head_sha.clone());
        return AppEvent::EnterActionsModeWithPrFilter {
            pr_number: pr.number,
            head_sha,
        };
    }
    state
        .prs_state
        .pr_detail
        .as_ref()
        .map_or(AppEvent::EnterActionsMode, |detail| {
            AppEvent::EnterActionsModeWithPrFilter {
                pr_number: detail.number,
                head_sha: detail.head_sha.clone(),
            }
        })
}

pub(super) fn selected_changes_thread(state: &AppState) -> Option<usize> {
    let changes = &state.prs_state.changes;
    let file = changes
        .selected_file
        .and_then(|index| changes.files.get(index))?;
    let base = if changes.view_mode == jefe::state::PrDiffViewMode::FullFile {
        let blob = changes
            .blobs
            .iter()
            .find(|entry| entry.blob_sha == file.blob_sha)?;
        jefe::pr_diff_content::build_full_document(file, &blob.blob)
    } else {
        jefe::pr_diff_content::build_delta_document(file)
    };
    let threads = state
        .prs_state
        .pr_detail
        .as_ref()?
        .reviews
        .iter()
        .flat_map(|review| review.review_threads.iter().cloned())
        .collect::<Vec<_>>();
    let document = jefe::pr_diff_content::build_threaded_document(file, base, &threads);
    changes
        .selected_row
        .and_then(|index| document.rows.get(index))
        .and_then(|row| row.thread_index)
}

pub(super) fn comment_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::Body | PrDetailSubfocus::Comment(_) | PrDetailSubfocus::NewComment => {
            AppEvent::PrOpenNewCommentComposer
        }
        PrDetailSubfocus::Review(_)
        | PrDetailSubfocus::ReviewThread(_)
        | PrDetailSubfocus::Check(_) => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyNoComment),
    }
}

pub(super) fn reply_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::Comment(comment_index) => AppEvent::PrOpenReplyComposer { comment_index },
        PrDetailSubfocus::ReviewThread(thread_index) => {
            AppEvent::PrOpenThreadReplyComposer { thread_index }
        }
        _ => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyReplyOnComment),
    }
}

pub(super) fn resolve_event_for_subfocus(subfocus: PrDetailSubfocus) -> AppEvent {
    match subfocus {
        PrDetailSubfocus::ReviewThread(thread_index) => {
            AppEvent::PrToggleThreadResolve { thread_index }
        }
        _ => AppEvent::PrShowNotice(ReadOnlyHintKind::ReadOnlyResolveOnThread),
    }
}

pub(super) fn pr_open_in_browser_or_notice(target_present: bool) -> AppEvent {
    if target_present {
        AppEvent::PrOpenInBrowser
    } else {
        AppEvent::PrShowNotice(ReadOnlyHintKind::NoSelectionToOpen)
    }
}

pub(super) fn pr_merge_event_for_detail(state: &AppState) -> AppEvent {
    if state.prs_state.pr_detail.is_none() {
        return AppEvent::PrShowNotice(ReadOnlyHintKind::NoPrToMerge);
    }
    if let Some(detail) = &state.prs_state.pr_detail
        && detail.state != jefe::domain::PrState::Open
    {
        return AppEvent::PrShowNotice(ReadOnlyHintKind::PrNotMergeable);
    }
    AppEvent::PrOpenMergeChooser
}

#[cfg(test)]
pub(super) fn resolve_prs_key_event(state: &AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    super::resolve_test_registry_event(state, key_event, 120, 40)
}

#[cfg(test)]
fn resolve_prs_key_event_for_rows(
    state: &AppState,
    key_event: &KeyEvent,
    terminal_rows: u16,
) -> Option<AppEvent> {
    super::resolve_test_registry_event(state, key_event, 120, terminal_rows)
}

#[cfg(test)]
#[path = "prs_key_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "prs_list_send_key_tests.rs"]
mod list_send_key_tests;

#[cfg(test)]
#[path = "prs_property_key_tests.rs"]
mod prs_property_key_tests;

#[cfg(test)]
#[path = "prs_diff_key_tests.rs"]
mod prs_diff_key_tests;
