//! Shared keyboard behavior for filter controls.
//!
//! Domains describe the active editor kind, then translate the resulting
//! command into their own events. State mutation and query semantics stay in
//! the domain reducers instead of leaking into a universal filter framework.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FilterEditorKind {
    Cycle,
    Text,
    Choice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FilterControlCommand {
    Apply,
    Cancel,
    Next,
    Previous,
    ClearCurrent,
    ClearAll,
    CycleNext,
    CyclePrevious,
    Append(char),
    Backspace,
}

pub(super) fn resolve_filter_control_key(
    editor: FilterEditorKind,
    key_event: &KeyEvent,
) -> Option<FilterControlCommand> {
    if key_event.kind != KeyEventKind::Press {
        return None;
    }
    match key_event.code {
        KeyCode::Enter => Some(FilterControlCommand::Apply),
        KeyCode::Esc => Some(FilterControlCommand::Cancel),
        KeyCode::Tab => Some(FilterControlCommand::Next),
        KeyCode::BackTab => Some(FilterControlCommand::Previous),
        KeyCode::Delete => Some(FilterControlCommand::ClearCurrent),
        KeyCode::Char('l') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(FilterControlCommand::ClearAll)
        }
        KeyCode::Left if editor != FilterEditorKind::Text => {
            Some(FilterControlCommand::CyclePrevious)
        }
        KeyCode::Right | KeyCode::Up | KeyCode::Down if editor != FilterEditorKind::Text => {
            Some(FilterControlCommand::CycleNext)
        }
        KeyCode::Char(' ') if editor == FilterEditorKind::Cycle => {
            Some(FilterControlCommand::CycleNext)
        }
        KeyCode::Char(c)
            if editor != FilterEditorKind::Cycle
                && !key_event
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(FilterControlCommand::Append(c))
        }
        KeyCode::Backspace if editor != FilterEditorKind::Cycle => {
            Some(FilterControlCommand::Backspace)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    #[test]
    fn shared_lifecycle_and_navigation_keys_are_domain_neutral() {
        let editor = FilterEditorKind::Text;
        assert_eq!(
            resolve_filter_control_key(editor, &key(KeyCode::Enter)),
            Some(FilterControlCommand::Apply)
        );
        assert_eq!(
            resolve_filter_control_key(editor, &key(KeyCode::Esc)),
            Some(FilterControlCommand::Cancel)
        );
        assert_eq!(
            resolve_filter_control_key(editor, &key(KeyCode::Tab)),
            Some(FilterControlCommand::Next)
        );
        assert_eq!(
            resolve_filter_control_key(editor, &key(KeyCode::BackTab)),
            Some(FilterControlCommand::Previous)
        );
        assert_eq!(
            resolve_filter_control_key(editor, &key(KeyCode::Delete)),
            Some(FilterControlCommand::ClearCurrent)
        );
    }

    #[test]
    fn ignores_non_press_events() {
        let mut release = key(KeyCode::Enter);
        release.kind = KeyEventKind::Release;
        assert_eq!(
            resolve_filter_control_key(FilterEditorKind::Text, &release),
            None
        );
    }

    #[test]
    fn editor_kind_selects_cycle_or_text_behavior() {
        assert_eq!(
            resolve_filter_control_key(FilterEditorKind::Cycle, &key(KeyCode::Char(' '))),
            Some(FilterControlCommand::CycleNext)
        );
        assert_eq!(
            resolve_filter_control_key(FilterEditorKind::Choice, &key(KeyCode::Left)),
            Some(FilterControlCommand::CyclePrevious)
        );
        assert_eq!(
            resolve_filter_control_key(FilterEditorKind::Text, &key(KeyCode::Char('x'))),
            Some(FilterControlCommand::Append('x'))
        );
        assert_eq!(
            resolve_filter_control_key(FilterEditorKind::Text, &key(KeyCode::Backspace)),
            Some(FilterControlCommand::Backspace)
        );
    }
}
