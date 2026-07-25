//! Split-mode entry/exit routing via the real `resolve_mode_key` resolver.

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind};

use jefe::state::{AppEvent, ScreenMode};

use super::normal::{KeyHandling, resolve_mode_key};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

#[test]
fn dashboard_s_emits_enter_split_mode_via_resolver() {
    let lower = resolve_mode_key(&key(KeyCode::Char('s')), ScreenMode::Dashboard);
    let upper = resolve_mode_key(&key(KeyCode::Char('S')), ScreenMode::Dashboard);
    assert!(
        matches!(lower, KeyHandling::Handled(Some(AppEvent::EnterSplitMode))),
        "Dashboard 's' must emit EnterSplitMode"
    );
    assert!(
        matches!(upper, KeyHandling::Handled(Some(AppEvent::EnterSplitMode))),
        "Dashboard 'S' must emit EnterSplitMode"
    );
}

#[test]
fn split_esc_emits_exit_split_mode_via_resolver() {
    let handling = resolve_mode_key(&key(KeyCode::Esc), ScreenMode::Split);
    assert!(
        matches!(
            handling,
            KeyHandling::Handled(Some(AppEvent::ExitSplitMode))
        ),
        "Split Esc must emit ExitSplitMode"
    );
}
