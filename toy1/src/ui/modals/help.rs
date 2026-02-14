//! Help modal showing keyboard shortcuts.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the help modal.
#[derive(Default, Props)]
pub struct HelpModalProps {
    /// Whether the modal is visible.
    pub visible: bool,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Keyboard shortcut reference modal.
#[component]
pub fn HelpModal(props: &HelpModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    if !props.visible {
        return element! { Box() };
    }

    let shortcuts: Vec<(String, Color)> = [
        "^/v      Navigate up/down",
        "</>      Switch pane",
        "Enter    Select / expand",
        "Esc      Back / close",
        "n        New agent",
        "d        Delete agent",
        "/        Search / command palette",
        "t        Open terminal",
        "Ctrl+]   Toggle embedded terminal focus",
        "F12/F6   Toggle embedded terminal focus",
        "Mouse events are forwarded to PTY when terminal is focused",
        "Cmd+V    Paste clipboard into embedded terminal",
        "s        Send prompt to agent",
        "p        Pause agent",
        "k        Kill agent",
        "l        View logs",
        "T        Cycle theme",
        "?        This help",
        "q        Quit",
    ]
    .iter()
    .map(|s| (format!("  {s}"), rc.fg))
    .collect();

    element! {
        Box(
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            flex_direction: FlexDirection::Column,
            padding: 1i32,
            width: 50u32,
        ) {
            Box(height: 1u32) {
                Text(content: " Keyboard Shortcuts".to_owned(), color: rc.fg, weight: Weight::Bold)
            }
            Box(height: 1u32) {
                Text(content: "".to_owned(), color: rc.dim)
            }
            #(shortcuts.into_iter().map(|(line, color): (String, Color)| {
                element! {
                    Box(height: 1u32) {
                        Text(content: line, color: color)
                    }
                }
            }))
            Box(height: 1u32, padding_top: 1i32) {
                Text(content: "  Press Esc to close".to_owned(), color: rc.dim)
            }
        }
    }
}
