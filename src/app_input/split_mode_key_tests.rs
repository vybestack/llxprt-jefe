//! Split-mode entry and exit through the S3 registry and typed executor.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use jefe::domain::action_registry::Resolution;
use jefe::list_viewport::PageItemCount;
use jefe::state::{AppEvent, AppState, ScreenId};

use super::action_handlers::{HandlerExecution, execution_for};

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
    let state = AppState::default();
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
fn split_esc_emits_exit_split_mode_via_registry() {
    let state = AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::Repositories),
        ..AppState::default()
    };
    assert!(matches!(
        resolved_execution(&state, &key(KeyCode::Esc)),
        HandlerExecution::Event(AppEvent::ExitSplitMode)
    ));
}

fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = key(code);
    event.modifiers = modifiers;
    event
}
