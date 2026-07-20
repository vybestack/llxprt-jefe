//! Keybind bar component - bottom bar with keyboard shortcuts.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::state::{ActionsFocus, ScreenMode};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the keybind bar component.
#[derive(Default, Props)]
pub struct KeybindBarProps {
    /// Current screen mode.
    pub screen_mode: ScreenMode,
    /// Whether terminal is focused.
    pub terminal_focused: bool,
    /// Whether the embedded agent shell owns the workspace.
    pub shell_overlay_active: bool,
    /// Active Actions pane when Actions mode is rendered.
    pub actions_focus: Option<ActionsFocus>,
    /// Process-identity label (pid + commit) shown in the lower-right corner
    /// (issue #223).
    pub identity_label: String,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Context-sensitive keybind hint text for a screen mode (display-only; pure).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn keybind_hints_for(
    screen_mode: ScreenMode,
    terminal_focused: bool,
    actions_focus: Option<ActionsFocus>,
) -> &'static str {
    if terminal_focused {
        return "F12 unfocus";
    }
    match screen_mode {
        ScreenMode::Dashboard => {
            "^/v navigate | </> pane | t/f12 terminal focus | F10 shell | F8 external term | v active-only (repos+agents) | \u{2325}1-9 jump agent | n new-agent | N new-repo | ctrl-d delete | ctrl-k kill | ctrl-r restart | l relaunch-dead | Space reorder | s split | F9 theme | ? help | ctrl-q/qqq quit"
        }
        ScreenMode::Split => "^/v select | g grab | m move | Esc back | ? help | ctrl-q/qqq quit",
        ScreenMode::DashboardIssues => {
            "^/v items | </> panes | Enter detail | n new issue | f filter | / search | Tab detail focus (j/k) | i list | r reply | S send-to-agent | e edit | c comment | C close D delete | L labels A assignees M milestone T title Y type W state | a exit | Esc back/exit"
        }
        // @plan PLAN-20260624-PR-MODE.P12
        // @requirement REQ-PR-001
        ScreenMode::DashboardPullRequests => {
            "^/v items | </> panes | Enter detail | f filter | / search | Tab detail focus (j/k) | p list | r reply | R resolve | S send-to-agent | c comment | o open | m merge | L labels A assignees M milestone T title W state | a exit | Esc back/exit"
        }
        ScreenMode::DashboardActions => match actions_focus {
            Some(ActionsFocus::RepoList) => {
                "^/v repos | > runs | Tab pane | f filter | / search | d dispatch | r refresh | Esc exit"
            }
            Some(ActionsFocus::RunList) | None => {
                "^/v runs | Enter detail | Tab pane | f filter | / search | d dispatch | r refresh | Esc exit"
            }
            Some(ActionsFocus::Detail) => {
                "^/v jobs | Enter/Right expand | Left collapse | Esc collapse/back | PgUp/PgDn scroll | Tab pane | ? help"
            }
        },
        ScreenMode::DashboardErrors => {
            "^/v errors | Enter detail | Tab pane | PgUp/PgDn scroll | Ctrl-C clear | Esc exit"
        }
    }
}

/// Keybind bar showing context-sensitive keyboard shortcuts.
#[component]
pub fn KeybindBar(props: &KeybindBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    let hints = if props.shell_overlay_active {
        "F10 close shell"
    } else {
        keybind_hints_for(
            props.screen_mode,
            props.terminal_focused,
            props.actions_focus,
        )
    };

    element! {
        Box(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            height: 1u32,
            background_color: rc.fg,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            // Left: keybind hints
            Text(content: hints, color: rc.bg)

            // Right: process identity (pid + commit) — issue #223
            Text(content: props.identity_label.clone(), color: rc.bg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_hints_include_shell_shortcuts_without_changing_focused_terminal_hint() {
        let dashboard = keybind_hints_for(ScreenMode::Dashboard, false, None);
        assert!(dashboard.contains("F10 shell"));
        assert!(dashboard.contains("F8 external term"));
        assert_eq!(
            keybind_hints_for(ScreenMode::Dashboard, true, None),
            "F12 unfocus"
        );
    }

    #[test]
    fn actions_hints_are_focus_specific_and_fit_footer_width() {
        let repos = keybind_hints_for(
            ScreenMode::DashboardActions,
            false,
            Some(ActionsFocus::RepoList),
        );
        let list = keybind_hints_for(
            ScreenMode::DashboardActions,
            false,
            Some(ActionsFocus::RunList),
        );
        let detail = keybind_hints_for(
            ScreenMode::DashboardActions,
            false,
            Some(ActionsFocus::Detail),
        );

        for required in [
            "Enter detail",
            "f filter",
            "/ search",
            "d dispatch",
            "r refresh",
        ] {
            assert!(list.contains(required));
        }
        assert!(repos.contains("^/v repos"));
        assert!(repos.contains("> runs"));
        assert!(detail.contains("Enter/Right expand"));
        assert!(detail.contains("Esc collapse/back"));
        assert!(repos.chars().count() <= 150);
        assert!(list.chars().count() <= 150);
        assert!(detail.chars().count() <= 150);
    }

    #[test]
    fn active_shell_footer_documents_f10_close_shortcut() {
        let mut element = element! {
            Box(width: 80u32, height: 1u32) {
                KeybindBar(
                    screen_mode: ScreenMode::Dashboard,
                    terminal_focused: true,
                    shell_overlay_active: true,
                    actions_focus: None,
                    identity_label: "pid:1 abc".to_string(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(80));
        let mut output = Vec::new();
        canvas
            .write_ansi(&mut output)
            .unwrap_or_else(|error| panic!("render keybind bar: {error}"));
        let rendered = String::from_utf8_lossy(&output);

        assert!(rendered.contains("F10 close shell"));
        assert!(!rendered.contains("F11 close shell"));
    }

    #[test]
    fn actions_run_list_footer_renders_refresh_at_fixed_width() {
        let mut element = element! {
            Box(width: 151u32, height: 1u32) {
                KeybindBar(
                    screen_mode: ScreenMode::DashboardActions,
                    terminal_focused: false,
                    actions_focus: Some(ActionsFocus::RunList),
                    identity_label: "pid:1 abc".to_string(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(151));
        let mut output = Vec::new();
        canvas
            .write_ansi(&mut output)
            .unwrap_or_else(|error| panic!("render keybind bar: {error}"));
        let rendered = String::from_utf8_lossy(&output);

        assert!(rendered.contains("f filter"));
        assert!(rendered.contains("/ search"));
        assert!(rendered.contains("d dispatch"));
        assert!(rendered.contains("r refresh"));
    }

    #[test]
    fn actions_detail_footer_renders_scroll_and_help_at_fixed_width() {
        let mut element = element! {
            Box(width: 151u32, height: 1u32) {
                KeybindBar(
                    screen_mode: ScreenMode::DashboardActions,
                    terminal_focused: false,
                    actions_focus: Some(ActionsFocus::Detail),
                    identity_label: "pid:1 abc".to_string(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(151));
        let mut output = Vec::new();
        canvas
            .write_ansi(&mut output)
            .unwrap_or_else(|error| panic!("render keybind bar: {error}"));
        let rendered = String::from_utf8_lossy(&output);

        assert!(rendered.contains("PgUp/PgDn scroll"));
        assert!(rendered.contains("? help"));
    }

    #[test]
    fn keybind_bar_renders_identity_label_in_lower_right() {
        let identity = "pid:99999 deadbeef".to_string();
        // Use a width wide enough for the hints + identity in any screen mode.
        let mut element = element! {
            Box(width: 300u32, height: 1u32) {
                KeybindBar(
                    screen_mode: ScreenMode::Dashboard,
                    terminal_focused: false,
                    actions_focus: None,
                    identity_label: identity.clone(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(300));
        let mut output = Vec::new();
        canvas
            .write_ansi(&mut output)
            .unwrap_or_else(|error| panic!("render keybind bar: {error}"));
        let rendered = String::from_utf8_lossy(&output);

        assert!(
            rendered.contains(&identity),
            "keybind bar must render the identity label: {rendered}"
        );
        assert!(
            rendered.contains("pid:"),
            "keybind bar must show the pid marker: {rendered}"
        );
    }
}
