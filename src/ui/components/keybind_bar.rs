//! Keybind bar component - bottom bar with keyboard shortcuts.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::state::ScreenMode;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the keybind bar component.
#[derive(Default, Props)]
pub struct KeybindBarProps {
    /// Current screen mode.
    pub screen_mode: ScreenMode,
    /// Whether terminal is focused.
    pub terminal_focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Context-sensitive keybind hint text for a screen mode (display-only; pure).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn keybind_hints_for(screen_mode: ScreenMode, terminal_focused: bool) -> &'static str {
    if terminal_focused {
        return "F12 unfocus";
    }
    match screen_mode {
        ScreenMode::Dashboard => {
            "^/v navigate | </> pane | t/f12 terminal focus | v active-only (repos+agents) | \u{2325}1-9 jump agent | n new-agent | N new-repo | ctrl-d delete | ctrl-k kill | ctrl-r restart | l relaunch-dead | Space reorder | s split | ? help | ctrl-q/qqq quit"
        }
        ScreenMode::Split => "^/v select | g grab | m move | Esc back | ? help | ctrl-q/qqq quit",
        ScreenMode::DashboardIssues => {
            "^/v navigate | Enter open detail | n new issue | f filter | / search | Tab cycle focus | i issue list | r reply | S send-to-agent | e edit | c comment | a exit issues | Esc back/exit"
        }
        // @plan PLAN-20260624-PR-MODE.P12
        // @requirement REQ-PR-001
        ScreenMode::DashboardPullRequests => {
            "^/v navigate | Enter open detail | f filter | / search | Tab cycle focus | p PR list | r reply | R resolve | S send-to-agent | e edit | c comment | o open in browser | m merge | a exit | Esc back/exit"
        }
    }
}

/// Keybind bar showing context-sensitive keyboard shortcuts.
#[component]
pub fn KeybindBar(props: &KeybindBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    let hints = keybind_hints_for(props.screen_mode, props.terminal_focused);

    element! {
        Box(
            width: 100pct,
            height: 1u32,
            background_color: rc.fg,
            padding_left: 1u32,
        ) {
            Text(content: hints, color: rc.bg)
        }
    }
}
