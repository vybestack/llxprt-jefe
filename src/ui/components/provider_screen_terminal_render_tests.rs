//! Painted shell-title hint contracts for the retained provider-screen renderer.

use iocraft::prelude::*;

use crate::domain::AgentId;
use crate::state::AppState;

const COLUMNS: u16 = 120;
const ROWS: u16 = 40;

fn painted_attached_shell(mut state: AppState) -> String {
    state.open_shell_overlay(AgentId("agent-render-hint".to_owned()));
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, COLUMNS, ROWS);
    assert!(
        state.resolved_layout.is_some(),
        "the attached-shell screen must resolve at {COLUMNS}x{ROWS}"
    );

    let mut element = element! {
        Box(width: u32::from(COLUMNS), height: u32::from(ROWS)) {
            super::ProviderScreen(
                state: Some(state),
                colors: crate::theme::ThemeColors::default(),
                theme_name: "default".to_owned(),
            )
        }
    };
    element.render(Some(usize::from(COLUMNS))).to_string()
}

fn attached_shell_title_row(painted: &str) -> &str {
    painted
        .lines()
        .find(|row| row.contains("Agent Shell ("))
        .unwrap_or_else(|| panic!("the attached shell title row was not painted:\n{painted}"))
}

#[test]
fn focused_terminal_manager_shell_paints_list_and_close_hint() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let _ = state.show_terminal_manager();

    let painted = painted_attached_shell(state);
    let title_row = attached_shell_title_row(&painted);

    assert!(
        title_row.contains("F12 list | F10 close shell"),
        "the focused Terminal Manager shell must paint its manager actions:\n{painted}"
    );
    assert!(
        !title_row.contains("F12 hide shell"),
        "the focused Terminal Manager shell must not paint the ordinary hide action:\n{painted}"
    );
}

#[test]
fn focused_shell_on_another_screen_keeps_the_hide_hint() {
    let state = AppState::new(crate::test_support::published_workbench());

    let painted = painted_attached_shell(state);
    let title_row = attached_shell_title_row(&painted);

    assert!(
        title_row.contains("F12 hide shell"),
        "a focused attached shell outside Terminal Manager must keep the hide action:\n{painted}"
    );
    assert!(
        !title_row.contains("F10 close shell"),
        "another screen must not inherit Terminal Manager actions:\n{painted}"
    );
}
