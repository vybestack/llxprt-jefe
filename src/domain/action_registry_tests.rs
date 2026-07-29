//! Unit tests for the CW-03 S0 closed action and binding values.

use super::action_registry::{
    ACTION_DESCRIPTION_BYTE_LIMIT, ACTION_LABEL_CELL_LIMIT, Action, ActionError, ActionId,
    ActionMetadata, Binding, BindingError, HandlerKey, Provenance,
};
use super::input_context::ContextId;
use super::keymap::Chord;

/// Parse a `ContextId`, panicking with context on grammar failure.
fn parsed_context(text: &str) -> ContextId {
    let Ok(context) = ContextId::parse(text) else {
        panic!("context {text:?} should parse");
    };
    context
}

/// Parse an `ActionId`, panicking with context on grammar failure.
fn parsed_action(text: &str) -> ActionId {
    let Ok(id) = ActionId::parse(text) else {
        panic!("action {text:?} should parse");
    };
    id
}

fn context() -> ContextId {
    parsed_context("dashboard")
}

fn action_id() -> ActionId {
    parsed_action("core.help")
}

fn metadata(label: String, description: String) -> ActionMetadata {
    ActionMetadata {
        id: action_id(),
        label,
        description,
        category: "core".to_owned(),
        contexts: vec![context()],
    }
}

#[test]
fn action_id_accepts_dotted_lowercase_and_rejects_invalid_grammar() {
    for valid in ["core.help", "core.jump-agent.1", "github.open-issues", "a"] {
        assert!(ActionId::parse(valid).is_ok(), "{valid:?}");
    }
    for invalid in [
        "",
        "Core.Help",
        "0abc",
        "core..help",
        "core help",
        "core/help",
    ] {
        assert!(ActionId::parse(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn action_id_length_bounds_are_inclusive() {
    assert!(ActionId::parse(&"a".repeat(128)).is_ok());
    assert!(ActionId::parse(&"a".repeat(129)).is_err());
}

#[test]
fn action_constructor_validates_complete_metadata() {
    let action = {
        let result = Action::new(
            metadata("Help".to_owned(), "Open contextual help.".to_owned()),
            HandlerKey::OpenHelp,
            false,
        );
        let Ok(action) = result else {
            panic!("valid action should construct, got {result:?}");
        };
        action
    };
    assert_eq!(action.handler, HandlerKey::OpenHelp);

    let empty = Action::new(
        metadata(String::new(), "description".to_owned()),
        HandlerKey::OpenHelp,
        false,
    );
    assert_eq!(empty, Err(ActionError::EmptyLabel));
}

#[test]
fn action_metadata_bounds_use_label_cells_and_description_bytes() {
    let wide = "界".repeat(ACTION_LABEL_CELL_LIMIT / 2 + 1);
    let result = Action::new(
        metadata(wide, "description".to_owned()),
        HandlerKey::OpenHelp,
        false,
    );
    assert!(matches!(result, Err(ActionError::LabelTooWide { .. })));

    let result = Action::new(
        metadata(
            "Help".to_owned(),
            "x".repeat(ACTION_DESCRIPTION_BYTE_LIMIT + 1),
        ),
        HandlerKey::OpenHelp,
        false,
    );
    assert!(matches!(
        result,
        Err(ActionError::DescriptionTooLong { .. })
    ));
}

#[test]
fn binding_constructor_accepts_eight_and_rejects_nine_chords() {
    let eight = (1..=8)
        .map(|number| {
            Chord::parse(&format!("F{number}"))
                .unwrap_or_else(|err| panic!("F{number} should parse: {err}"))
        })
        .collect();
    assert!(Binding::new(context(), action_id(), eight, Provenance::Compiled).is_ok());

    let nine = (1..=9)
        .map(|number| {
            Chord::parse(&format!("F{number}"))
                .unwrap_or_else(|err| panic!("F{number} should parse: {err}"))
        })
        .collect();
    assert!(matches!(
        Binding::new(context(), action_id(), nine, Provenance::Compiled),
        Err(BindingError::TooManyChords { .. })
    ));
}

#[test]
fn binding_constructor_rejects_empty_and_duplicate_chords() {
    assert_eq!(
        Binding::new(context(), action_id(), Vec::new(), Provenance::Compiled),
        Err(BindingError::EmptyChords)
    );
    let chord = parsed_chord("F1");
    assert!(matches!(
        Binding::new(
            context(),
            action_id(),
            vec![chord, chord],
            Provenance::Compiled,
        ),
        Err(BindingError::DuplicateChord(_))
    ));
}

/// Parse a canonical chord text, panicking with context on grammar failure.
fn parsed_chord(text: &str) -> Chord {
    let Ok(chord) = Chord::parse(text) else {
        panic!("chord {text:?} should parse");
    };
    chord
}
