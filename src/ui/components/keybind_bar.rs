//! Keybind bar component - bottom bar with keyboard shortcuts.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::action_projection::{FooterProjectionInput, project_footer_effective};
use crate::domain::action_registry::{ActionRegistrySnapshot, AvailabilityGeneration};
use crate::domain::default_action_inventory::display::FooterMode;
use crate::published_workbench::PublishedWorkbench;
use crate::state::{ActionsFocus, ScreenId};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the keybind bar component.
#[derive(Default, Props)]
pub struct KeybindBarProps {
    /// Current active screen.
    pub screen: ScreenId,
    /// Whether terminal is focused.
    pub terminal_focused: bool,
    /// Whether the embedded agent shell overlay is visible (issue #222/#361).
    pub shell_overlay_active: bool,
    /// Whether the selected agent owns a hidden shell that F10 can resume
    /// (issue #361 PR A).
    pub shell_resume_available: bool,
    /// Active Actions pane when Actions mode is rendered.
    /// The committed declaration authority for this render.
    pub published_workbench: Option<std::sync::Arc<PublishedWorkbench>>,
    /// Latest validated runtime-only availability generation, when one exists.
    pub action_availability: Option<AvailabilityGeneration>,
    pub actions_focus: Option<ActionsFocus>,
    pub mode_override: Option<FooterMode>,
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
    screen: ScreenId,
    terminal_focused: bool,
    actions_focus: Option<ActionsFocus>,
) -> String {
    keybind_hints_for_effective(snapshot, None, screen, terminal_focused, actions_focus)
}

/// Context-sensitive footer projection with generation-bound runtime availability.
#[must_use]
pub fn keybind_hints_for_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
    screen: ScreenId,
    terminal_focused: bool,
    actions_focus: Option<ActionsFocus>,
) -> String {
    project_footer_effective(
        snapshot,
        runtime,
        FooterProjectionInput {
            screen,
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

    let Some(workbench) = props.published_workbench.as_ref() else {
        panic!("KeybindBar requires the committed workbench");
    };
    let hints = project_footer_effective(
        workbench.actions(),
        props.action_availability.as_ref(),
        FooterProjectionInput {
            screen: props.screen,
            terminal_focused: props.terminal_focused,
            shell_overlay_active: props.shell_overlay_active,
            shell_resume_available: props.shell_resume_available,
            actions_focus: props.actions_focus,
            mode_override: props.mode_override,
        },
    );

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
        let dashboard = keybind_hints_for(
            &crate::action_projection::test_snapshot(),
            ScreenId::Dashboard,
            false,
            None,
        );
        assert!(dashboard.contains("F10 shell"));
        assert!(dashboard.contains("F8 external term"));
        assert_eq!(
            keybind_hints_for(
                &crate::action_projection::test_snapshot(),
                ScreenId::Dashboard,
                true,
                None
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
                    published_workbench: Some(crate::test_support::published_workbench()),
                    screen: ScreenId::Dashboard,
                    terminal_focused: true,
                    shell_overlay_active: true,
                    shell_resume_available: false,
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

        assert!(rendered.contains("F12 hide shell"));
        assert!(rendered.contains("F10 close shell"));
        assert!(!rendered.contains("F11 close shell"));
    }

    #[test]
    fn dashboard_footer_offers_f10_resume_when_selected_agent_has_hidden_shell() {
        let mut element = element! {
            Box(width: 180u32, height: 1u32) {
                KeybindBar(
                    published_workbench: Some(crate::test_support::published_workbench()),
                    screen: ScreenId::Dashboard,
                    terminal_focused: false,
                    shell_overlay_active: false,
                    shell_resume_available: true,
                    actions_focus: None,
                    identity_label: "pid:1 abc".to_string(),
                    colors: ThemeColors::default(),
                )
            }
        };
        let canvas = element.render(Some(180));
        let mut output = Vec::new();
        canvas
            .write_ansi(&mut output)
            .unwrap_or_else(|error| panic!("render keybind bar: {error}"));
        let rendered = String::from_utf8_lossy(&output);

        assert!(rendered.contains("F10 resume shell"));
        assert!(rendered.contains("F7 shells"));
        assert!(rendered.contains("F8 external term"));
    }

    #[test]
    fn actions_run_list_footer_renders_refresh_at_fixed_width() {
        let mut element = element! {
            Box(width: 151u32, height: 1u32) {
                KeybindBar(
                    published_workbench: Some(crate::test_support::published_workbench()),
                    screen: ScreenId::Actions,
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
                    published_workbench: Some(crate::test_support::published_workbench()),
                    screen: ScreenId::Actions,
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
        // Use a width wide enough for the hints + identity in any active screen.
        let mut element = element! {
            Box(width: 360u32, height: 1u32) {
                KeybindBar(
                    published_workbench: Some(crate::test_support::published_workbench()),
                    screen: ScreenId::Dashboard,
                    terminal_focused: false,
                    actions_focus: None,
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
