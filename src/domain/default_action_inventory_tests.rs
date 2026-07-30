//! Integrity and source-authority tests for the compiled S0 inventory.

use std::collections::{HashMap, HashSet};

use super::action_registry::{ACTION_DESCRIPTION_BYTE_LIMIT, ACTION_LABEL_CELL_LIMIT};
use super::default_action_inventory::{
    AUDITED_DISPATCH_SOURCES, CompiledInventory, compiled_inventory, golden_projection,
};
use unicode_width::UnicodeWidthStr;

/// Compile the frozen inventory, panicking with context on failure.
fn inventory() -> CompiledInventory {
    let result = compiled_inventory();
    let Ok(inventory) = result else {
        panic!("compiled literals are valid, got {result:?}");
    };
    inventory
}

/// Project golden rows, panicking with context on failure.
fn projection_rows() -> Vec<super::default_action_inventory::GoldenProjectionRow> {
    let result = golden_projection(&inventory());
    let Ok(rows) = result else {
        panic!("projection integrity, got {result:?}");
    };
    rows
}

#[test]
fn compiled_inventory_is_valid_and_metadata_complete() {
    let inventory = inventory();
    assert!(!inventory.actions.is_empty());
    assert_eq!(inventory.actions.len(), inventory.bindings.len());
    for action in &inventory.actions {
        assert!(!action.label.trim().is_empty(), "{}", action.id.as_str());
        assert!(UnicodeWidthStr::width(action.label.as_str()) <= ACTION_LABEL_CELL_LIMIT);
        assert!(!action.description.trim().is_empty());
        assert!(action.description.len() <= ACTION_DESCRIPTION_BYTE_LIMIT);
        assert!(!action.category.trim().is_empty());
        assert!(!action.contexts.is_empty());
    }
}

#[test]
fn every_binding_is_bounded_and_references_one_authoritative_action() {
    let inventory = inventory();
    let actions: HashMap<_, _> = inventory
        .actions
        .iter()
        .map(|action| (action.id.clone(), action))
        .collect();
    assert_eq!(actions.len(), inventory.actions.len());
    for binding in &inventory.bindings {
        assert!(
            binding.validate().is_ok(),
            "compiled binding should validate"
        );
        let Some(action) = actions.get(&binding.action) else {
            panic!(
                "binding references a declared action {}",
                binding.action.as_str()
            );
        };
        assert!(action.contexts.contains(&binding.context));
    }
}

#[test]
fn golden_projection_uses_action_handler_and_has_no_context_chord_collisions() {
    let inventory = inventory();
    let rows = projection_rows();
    let actions: HashMap<_, _> = inventory
        .actions
        .iter()
        .map(|action| (action.id.clone(), action.handler))
        .collect();
    let mut seen = HashSet::new();
    for row in rows {
        assert_eq!(Some(&row.handler), actions.get(&row.action));
        assert!(
            seen.insert((row.context.clone(), row.chord)),
            "duplicate binding in {} for {}",
            row.context.as_str(),
            row.chord
        );
    }
}

#[test]
fn accepted_source_decisions_do_not_invent_ticket_aliases() {
    let rows = projection_rows();
    let has = |context: &str, chord: &str, action: &str| {
        rows.iter().any(|row| {
            row.context.as_str() == context
                && row.chord.to_canonical_text() == chord
                && row.action.as_str() == action
        })
    };

    assert!(has("dashboard", "g", "github.open-actions"));
    assert!(has("dashboard", "Ctrl+D", "dashboard.delete-selection"));
    assert!(has("split", "Esc", "split.back"));
    assert!(
        !rows
            .iter()
            .any(|row| row.context.as_str() == "split" && row.chord.to_canonical_text() == "q")
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.context.as_str() == "global" && row.action.as_str().contains("help"))
    );
    assert!(!rows.iter().any(|row| row.context.as_str() == "global" && row.action.as_str().contains("terminal")));
}

#[test]
fn translated_terminal_control_chords_equal_compiled_defaults() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let inventory = inventory();
    let Some(binding) = inventory
        .bindings
        .iter()
        .find(|binding| binding.action.as_str() == "core.emergency-exit")
    else {
        panic!("compiled inventory must define core.emergency-exit");
    };
    let event = KeyEvent {
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let result = super::keymap::Chord::from_crossterm(&event);
    let Ok(translated) = result else {
        panic!("translate should succeed, got {result:?}");
    };
    assert_eq!(binding.chords, vec![translated]);
}

#[test]
fn source_audit_covers_every_current_keyboard_authority_family() {
    for required in [
        "src/input.rs",
        "src/app_shell_key_routing.rs",
        "src/app_input/normal.rs",
        "src/app_input/errors.rs",
        "src/app_input/terminal_manager.rs",
        "src/app_input/shell_overlay.rs",
        "src/app_input/modal_handlers.rs",
        "src/app_input/issues.rs",
        "src/app_input/prs.rs",
        "src/app_input/actions.rs",
        "src/pty_encoding.rs",
    ] {
        assert!(AUDITED_DISPATCH_SOURCES.contains(&required), "{required}");
    }
}

#[test]
fn every_context_local_unwind_is_protected() {
    let inventory = inventory();
    for handler in [
        super::action_registry::HandlerKey::ExitSplit,
        super::action_registry::HandlerKey::ErrorsBack,
        super::action_registry::HandlerKey::TerminalManagerBack,
        super::action_registry::HandlerKey::HelpClose,
        super::action_registry::HandlerKey::ConfirmCancel,
        super::action_registry::HandlerKey::AuthCancel,
        super::action_registry::HandlerKey::FormCancel,
        super::action_registry::HandlerKey::ThemeCancel,
        super::action_registry::HandlerKey::SearchCancel,
        super::action_registry::HandlerKey::FilterCancel,
        super::action_registry::HandlerKey::IssuesExit,
        super::action_registry::HandlerKey::IssuesBack,
        super::action_registry::HandlerKey::IssuesCancelInline,
        super::action_registry::HandlerKey::IssuesChooserCancel,
        super::action_registry::HandlerKey::PullRequestsExit,
        super::action_registry::HandlerKey::PullRequestsBack,
        super::action_registry::HandlerKey::PullRequestsCancelInline,
        super::action_registry::HandlerKey::PullRequestsChooserCancel,
        super::action_registry::HandlerKey::ActionsExit,
    ] {
        assert!(
            inventory
                .actions
                .iter()
                .any(|action| action.handler == handler && action.protected),
            "local unwind {handler:?} must be protected"
        );
    }
}

#[test]
fn context_stacks_are_closed_source_orders_with_nested_canonical_parents() {
    let inventory = inventory();
    let first_stack = |leaf: &str| {
        inventory
            .context_stacks
            .iter()
            .find(|stack| {
                stack
                    .iter()
                    .next()
                    .is_some_and(|context| context.as_str() == leaf)
            })
            .map(|stack| {
                stack
                    .iter()
                    .map(super::input_context::ContextId::as_str)
                    .collect::<Vec<_>>()
            })
    };

    assert_eq!(
        first_stack("modal.confirm"),
        Some(vec![
            "modal.confirm",
            "issues.inline",
            "issues.detail",
            "issues.list",
            "global",
        ])
    );
    assert_eq!(
        first_stack("dashboard.grab"),
        Some(vec![
            "dashboard.grab",
            "dashboard.reorder",
            "dashboard",
            "global",
        ])
    );
    assert_eq!(first_stack("shell-overlay"), Some(vec!["shell-overlay"]));
}
