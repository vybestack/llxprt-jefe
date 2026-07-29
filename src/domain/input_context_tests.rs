//! Unit tests for the CW-03 S0 context-identifier and ordered context stack.

use super::input_context::{
    ContextId, ContextIdErrorReason, ContextStack, ContextStackError, resolve_context_stack,
};

/// Resolve a context stack, panicking with context on failure.
fn resolved(
    modal: Option<&str>,
    focused_editor_or_chooser: Option<&str>,
    focused_panel: Option<&str>,
    screen: Option<&str>,
    global: Option<&str>,
) -> ContextStack {
    let result = resolve_context_stack(
        modal,
        focused_editor_or_chooser,
        focused_panel,
        screen,
        global,
    );
    let Ok(stack) = result else {
        panic!("context stack should resolve, got {result:?}");
    };
    stack
}

// ── ContextId ──────────────────────────────────────────────────────────────

#[test]
fn context_id_accepts_lowercase_dotted() {
    for valid in [
        "global",
        "terminal",
        "dashboard",
        "issues.list",
        "issues.detail",
        "pr.list",
        "pr.detail",
        "modal.confirm",
        "help",
        "split",
    ] {
        let parsed = ContextId::parse(valid);
        assert!(parsed.is_ok(), "{valid:?} must parse, got {parsed:?}");
    }
}

#[test]
fn context_id_rejects_invalid_grammar() {
    for invalid in [
        "",
        "Global",
        "0global",
        "issues..list",
        "issues list",
        "issues/list",
    ] {
        let parsed = ContextId::parse(invalid);
        assert!(parsed.is_err(), "{invalid:?} must be rejected");
    }
}

#[test]
fn context_id_error_categorized() {
    let Err(err) = ContextId::parse("0bad") else {
        panic!("invalid context should error");
    };
    assert!(matches!(err.reason, ContextIdErrorReason::Grammar));
}

// ── Ordered context stack ──────────────────────────────────────────────────

#[test]
fn context_stack_searches_modal_first_then_editor_chooser_panel_screen_global() {
    // Canonical resolution order: modal, focused editor/chooser, focused panel,
    // screen, global (CW03-02). S0 exposes the pure ordered stack builder.
    let stack = resolved(
        /* modal */ Some("modal.confirm"),
        /* focused_editor_or_chooser */ None,
        /* focused_panel */ None,
        /* screen */ Some("dashboard"),
        /* global */ Some("global"),
    );
    let ids: Vec<&str> = stack.iter().map(ContextId::as_str).collect();
    assert_eq!(ids, vec!["modal.confirm", "dashboard", "global"]);
}

#[test]
fn context_stack_includes_all_six_levels_when_present() {
    let stack = resolved(
        Some("modal.confirm"),
        Some("issues.inline"),
        Some("issues.list"),
        Some("dashboard"),
        Some("global"),
    );
    let ids: Vec<&str> = stack.iter().map(ContextId::as_str).collect();
    assert_eq!(
        ids,
        vec![
            "modal.confirm",
            "issues.inline",
            "issues.list",
            "dashboard",
            "global",
        ]
    );
}

#[test]
fn context_stack_omits_absent_levels() {
    let stack = resolved(None, None, None, None, Some("global"));
    let ids: Vec<&str> = stack.iter().map(ContextId::as_str).collect();
    assert_eq!(ids, vec!["global"]);
}

#[test]
fn context_stack_empty_when_no_levels() {
    let stack = resolved(None, None, None, None, None);
    assert!(stack.is_empty());
    assert!(!stack.is_terminal_capture());
}

#[test]
fn context_stack_rejects_an_invalid_level_instead_of_broadening_resolution() {
    let result = resolve_context_stack(Some("Bad"), None, None, None, Some("global"));
    assert!(result.is_err());
}

#[test]
fn terminal_stack_is_a_distinct_validated_sixth_resolution_level() {
    let result = ContextStack::from_ordered(["terminal", "global"], true);
    let Ok(stack) = result else {
        panic!("terminal stack should validate, got {result:?}");
    };
    assert!(stack.is_terminal_capture());
    assert_eq!(
        stack.iter().map(ContextId::as_str).collect::<Vec<_>>(),
        vec!["terminal", "global"]
    );
}

#[test]
fn context_stack_rejects_duplicate_levels_and_terminal_without_context() {
    let duplicate = ContextStack::from_ordered(["dashboard", "dashboard"], false);
    assert!(matches!(
        duplicate,
        Err(ContextStackError::DuplicateContext(_))
    ));

    let terminal = ContextStack::from_ordered(std::iter::empty::<&str>(), true);
    assert_eq!(terminal, Err(ContextStackError::MissingTerminalContext));
}
