//! Split-mode entry and exit through the S3 registry and typed executor.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use jefe::domain::action_registry::Resolution;
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, AppState};

use super::action_handlers::{BoundaryAction, HandlerExecution, execution_for};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn resolved_execution(state: &AppState, event: &KeyEvent) -> HandlerExecution {
    let resolved = crate::app_shell_key_routing::resolve_compiled_registry_key(state, event);
    let Resolution::Dispatch { handler, .. } = resolved.resolution else {
        panic!("S3 key should dispatch, got {:?}", resolved.resolution);
    };
    execution_for(handler, resolved.chord, state, PageItemCount::new(1))
}

#[test]
fn dashboard_s_emits_enter_split_mode_via_registry() {
    let state = crate::test_app_state();
    for event in [
        key(KeyCode::Char('s')),
        modified(KeyCode::Char('S'), KeyModifiers::SHIFT),
    ] {
        assert!(matches!(
            resolved_execution(&state, &event),
            HandlerExecution::Event(AppEvent::EnterSplitMode)
        ));
    }
}

#[test]
fn split_esc_enters_the_shared_back_reducer() {
    let mut state = crate::test_app_state();
    state.restore_navigation_root(jefe::workbench::REPOSITORIES_IDENTITY);
    assert!(matches!(
        resolved_execution(&state, &key(KeyCode::Esc)),
        HandlerExecution::Event(AppEvent::Back)
    ));
}

/// The split paging keys must dispatch through the shared control path
/// (boundary → control action against the declared cards control); the legacy
/// direct AppEvent dispatch was deleted with the split screen (issue #706).
#[test]
fn split_page_keys_dispatch_through_the_shared_control_path() {
    let mut state = crate::test_app_state();
    state.restore_navigation_root(jefe::workbench::REPOSITORIES_IDENTITY);
    for event in [key(KeyCode::PageUp), key(KeyCode::PageDown)] {
        let execution = resolved_execution(&state, &event);
        assert!(
            matches!(execution, HandlerExecution::Boundary(_)),
            "the split paging keys must dispatch a boundary control action, got {execution:?}"
        );
    }
    assert!(
        matches!(
            resolved_execution(&state, &key(KeyCode::PageUp)),
            HandlerExecution::Boundary(BoundaryAction::WorkbenchPagePrevious)
        ),
        "PageUp must dispatch the previous-page control action"
    );
    assert!(
        matches!(
            resolved_execution(&state, &key(KeyCode::PageDown)),
            HandlerExecution::Boundary(BoundaryAction::WorkbenchPageNext)
        ),
        "PageDown must dispatch the next-page control action"
    );
}

fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = key(code);
    event.modifiers = modifiers;
    event
}
