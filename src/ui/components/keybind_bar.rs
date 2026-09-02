//! Keybind bar component - bottom bar with keyboard shortcuts.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

#[cfg(test)]
use crate::action_projection::{FooterProjectionInput, project_footer_effective};
#[cfg(test)]
use crate::domain::action_registry::{ActionRegistrySnapshot, AvailabilityGeneration};
#[cfg(test)]
use crate::domain::default_action_inventory::display::FooterMode;
#[cfg(test)]
use crate::state::{ActionsFocus, ScreenId};
use crate::theme::{ResolvedColors, ThemeColors};
#[cfg(test)]
use crate::workbench::ScreenIdentity;

/// Props for the keybind bar component.
#[derive(Default, Props)]
pub struct KeybindBarProps {
    /// Footer text projected at the mandatory committed-workbench boundary.
    pub hints: String,
    /// Process-identity label (pid + commit) shown in the lower-right corner
    /// (issue #223).
    pub identity_label: String,
    /// Theme colors.
    pub colors: ThemeColors,
}

#[cfg(test)]
/// Context-sensitive footer projection from the immutable registry snapshot.
#[must_use]
pub fn keybind_hints_for(
    snapshot: &ActionRegistrySnapshot,
    screen: impl Into<ScreenIdentity>,
    terminal_focused: bool,
    actions_focus: Option<ActionsFocus>,
) -> String {
    keybind_hints_for_effective(snapshot, None, screen, terminal_focused, actions_focus)
}

#[cfg(test)]
/// Context-sensitive footer projection with generation-bound runtime availability.
#[must_use]
pub fn keybind_hints_for_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
    screen: impl Into<ScreenIdentity>,
    terminal_focused: bool,
    actions_focus: Option<ActionsFocus>,
) -> String {
    project_footer_effective(
        snapshot,
        runtime,
        FooterProjectionInput {
            screen: screen.into(),
            terminal_focused,
            shell_overlay_active: false,
            shell_resume_available: false,
            actions_focus,
            mode_override: None,
        },
    )
}

/// Keybind bar showing context-sensitive keyboard shortcuts.
#[component]
pub fn KeybindBar(props: &KeybindBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let hints = props.hints.clone();

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
    fn split_hints_include_shell_shortcuts_without_changing_focused_terminal_hint() {
        let dashboard = project_footer_effective(
            &crate::action_projection::test_snapshot(),
            None,
            FooterProjectionInput {
                screen: crate::workbench::DASHBOARD_IDENTITY,
                terminal_focused: false,
                shell_overlay_active: false,
                shell_resume_available: false,
                actions_focus: None,
                mode_override: Some(FooterMode::Dashboard),
            },
        );
        assert!(dashboard.contains("F10 shell"));
        assert!(dashboard.contains("F8 external term"));
        assert_eq!(
            project_footer_effective(
                &crate::action_projection::test_snapshot(),
                None,
                FooterProjectionInput {
                    screen: crate::workbench::DASHBOARD_IDENTITY,
                    terminal_focused: true,
                    shell_overlay_active: false,
                    shell_resume_available: false,
                    actions_focus: None,
                    mode_override: Some(FooterMode::Dashboard),
                },
            ),
            "F12 unfocus"
        );
    }

    #[test]
    fn pull_request_hints_advertise_the_lifecycle_actions() {
        let hints = keybind_hints_for(
            &crate::action_projection::test_snapshot(),
            ScreenId::PullRequests,
            false,
            None,
        );

        // Issue #183: the PR footer gains the same lifecycle vocabulary the
        // Issues footer already has, so triage does not require guessing.
        for required in ["new PR", "close / reopen", "delete", "merge"] {
            assert!(hints.contains(required), "missing {required:?} in {hints}");
        }
        assert!(hints.is_ascii(), "the footer stays emoji-free: {hints}");
    }

    #[test]
    fn actions_hints_are_focus_specific_and_fit_footer_width() {
        let repos = keybind_hints_for(
            &crate::action_projection::test_snapshot(),
            ScreenId::Actions,
            false,
            Some(ActionsFocus::RepoList),
        );
        let list = keybind_hints_for(
            &crate::action_projection::test_snapshot(),
            ScreenId::Actions,
            false,
            Some(ActionsFocus::RunList),
        );
        let detail = keybind_hints_for(
            &crate::action_projection::test_snapshot(),
            ScreenId::Actions,
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
        assert!(repos.contains("^/v repositories"));
        assert!(repos.contains("runs / pane"));
        assert!(detail.contains("expand"));
        assert!(detail.contains("collapse / back"));
        assert!(repos.chars().count() <= 150);
        assert!(list.chars().count() <= 150);
        assert!(detail.chars().count() <= 150);
    }

    #[test]
    fn active_shell_footer_documents_f12_hide_and_f10_close_shortcuts() {
        let mut element = element! {
            Box(width: 80u32, height: 1u32) {
                KeybindBar(
                    hints: project_footer_effective(
                        crate::test_support::published_workbench().actions(),
                        None,
                        FooterProjectionInput {
                            screen: crate::workbench::DASHBOARD_IDENTITY,
                            terminal_focused: true,
                            shell_overlay_active: true,
                            shell_resume_available: false,
                            actions_focus: None,
                            mode_override: Some(FooterMode::Dashboard),
                        },
                    ),
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

        assert!(rendered.contains("F12 hide shell"));
        assert!(rendered.contains("F10 close shell"));
        assert!(!rendered.contains("F11 close shell"));
    }

    #[test]
    fn actions_run_list_footer_renders_refresh_at_fixed_width() {
        let mut element = element! {
            Box(width: 151u32, height: 1u32) {
                KeybindBar(
                    hints: keybind_hints_for(
                        crate::test_support::published_workbench().actions(),
                        ScreenId::Actions,
                        false,
                        Some(ActionsFocus::RunList),
                    ),
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
                    hints: keybind_hints_for(
                        crate::test_support::published_workbench().actions(),
                        ScreenId::Actions,
                        false,
                        Some(ActionsFocus::Detail),
                    ),
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
        // Use a width wide enough for the hints + identity in any active screen.
        let mut element = element! {
            Box(width: 360u32, height: 1u32) {
                KeybindBar(
                    hints: keybind_hints_for(
                        crate::test_support::published_workbench().actions(),
                        crate::workbench::REPOSITORIES_IDENTITY,
                        false,
                        None,
                    ),
                    identity_label: identity.clone(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(360));
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
