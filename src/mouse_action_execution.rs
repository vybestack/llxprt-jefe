//! S7 mouse activation boundary using the keyboard-owned resolution executor.

use crate::app_shell::{CtxArc, HookState};
use jefe::state::AppState;

/// Transient left-button candidate; this is app-shell state, never durable state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseClickState {
    down: Option<(u16, u16)>,
}

impl MouseClickState {
    pub fn clear(&mut self) {
        self.down = None;
    }

    pub(super) fn observe(&mut self, event: &iocraft::FullscreenMouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.down = Some((event.column, event.row));
            }
            MouseEventKind::Up(MouseButton::Left) => {}
            _ => self.clear(),
        }
    }

    fn take(&mut self) -> Option<(u16, u16)> {
        self.down.take()
    }
}

/// Resolve and execute a left-button release if it is an approved action click.
pub(super) fn try_up_click(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<crate::pty_encoding::PasteEnterSuppression>,
    click_state: &mut HookState<MouseClickState>,
    mouse_event: &iocraft::FullscreenMouseEvent,
) -> bool {
    use crossterm::event::{MouseButton, MouseEventKind};
    if !matches!(mouse_event.kind, MouseEventKind::Up(MouseButton::Left)) {
        return false;
    }
    let down = click_state.write().take();
    let (state, cols, rows) = {
        let state = app_state.read();
        let (cols, rows) = super::terminal_size();
        (state.clone(), cols, rows)
    };
    let click = super::mouse_action_routing::MouseClickInput {
        down,
        up: (mouse_event.column, mouse_event.row),
        terminal: (cols, rows),
    };
    let Some(route) = super::mouse_action_routing::resolve_action_click(&state, click) else {
        return false;
    };
    crate::action_capture_emit::record_mouse(
        mouse_event.column,
        mouse_event.row,
        route.hit,
        route.action.as_str(),
        &route.resolution,
    );
    let key_event = key_event_for(route.chord);
    crate::app_shell_key_routing::execute_mouse_resolution(
        ctx,
        app_state,
        should_quit,
        suppress_next_enter,
        crate::app_shell_key_routing::MouseResolutionInput {
            chord: route.chord,
            resolution: route.resolution,
            key_event: &key_event,
        },
    )
}

fn key_event_for(chord: jefe::domain::keymap::Chord) -> iocraft::prelude::KeyEvent {
    use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use jefe::domain::keymap::{Key, Modifier};
    let code = match chord.key {
        Key::Char(character) => KeyCode::Char(character),
        Key::Enter => KeyCode::Enter,
        Key::Esc => KeyCode::Esc,
        Key::Tab => KeyCode::Tab,
        Key::BackTab => KeyCode::BackTab,
        Key::Backspace => KeyCode::Backspace,
        Key::Delete => KeyCode::Delete,
        Key::Insert => KeyCode::Insert,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Function(number) => KeyCode::F(number),
    };
    let mut modifiers = KeyModifiers::empty();
    for modifier in chord.modifiers.iter() {
        modifiers |= match modifier {
            Modifier::Ctrl => KeyModifiers::CONTROL,
            Modifier::Alt => KeyModifiers::ALT,
            Modifier::Shift => KeyModifiers::SHIFT,
            Modifier::Super => KeyModifiers::SUPER,
        };
    }
    let mut event = KeyEvent::new(KeyEventKind::Press, code);
    event.modifiers = modifiers;
    event
}
