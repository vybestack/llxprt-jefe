//! Shared raw text mutation classification for filter controls.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FilterTextMutation {
    Append(char),
    Backspace,
}

#[must_use]
pub(super) fn resolve_filter_text_mutation(
    accepts_text: bool,
    key_event: &KeyEvent,
) -> Option<FilterTextMutation> {
    if !accepts_text || key_event.kind != KeyEventKind::Press {
        return None;
    }
    match key_event.code {
        KeyCode::Char(character)
            if super::raw_key_mutations::text_modifiers(key_event.modifiers) =>
        {
            Some(FilterTextMutation::Append(character))
        }
        KeyCode::Backspace => Some(FilterTextMutation::Backspace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iocraft::prelude::{KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(KeyEventKind::Press, code)
    }

    #[test]
    fn only_text_mutations_are_classified() {
        assert_eq!(
            resolve_filter_text_mutation(true, &key(KeyCode::Char('x'))),
            Some(FilterTextMutation::Append('x'))
        );
        assert_eq!(
            resolve_filter_text_mutation(true, &key(KeyCode::Backspace)),
            Some(FilterTextMutation::Backspace)
        );
        for code in [KeyCode::Enter, KeyCode::Esc, KeyCode::Tab, KeyCode::Delete] {
            assert_eq!(resolve_filter_text_mutation(true, &key(code)), None);
        }
    }

    #[test]
    fn modified_actions_and_non_text_fields_are_not_raw() {
        let mut control = key(KeyCode::Char('l'));
        control.modifiers = KeyModifiers::CONTROL;
        assert_eq!(resolve_filter_text_mutation(true, &control), None);
        assert_eq!(
            resolve_filter_text_mutation(false, &key(KeyCode::Char('x'))),
            None
        );
    }
}
