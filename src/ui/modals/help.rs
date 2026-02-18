//! Help modal - keyboard shortcut reference.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-008

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the help modal.
#[derive(Default, Props)]
pub struct HelpModalProps {
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Help modal showing all keyboard shortcuts.
#[component]
pub fn HelpModal(props: &HelpModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 60u32,
            height: 20u32,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            // Title
            Box(height: 2u32, background_color: rc.bg) {
                Text(
                    content: "Help - Keyboard Shortcuts",
                    weight: Weight::Bold,
                    color: rc.fg,
                )
            }

            // Shortcuts
            Box(flex_direction: FlexDirection::Column, flex_grow: 1.0, background_color: rc.bg) {
                Text(content: "Navigation:", color: rc.fg)
                Text(content: "  Up/Down     Select item", color: rc.fg)
                Text(content: "  Left/Right  Switch pane", color: rc.fg)
                Text(content: "  F12         Toggle terminal focus", color: rc.fg)
                Text(content: "", color: rc.fg)
                Text(content: "Actions:", color: rc.fg)
                Text(content: "  n           New agent", color: rc.fg)
                Text(content: "  N           New repository", color: rc.fg)
                Text(content: "  Ctrl-d      Delete selected", color: rc.fg)
                Text(content: "  Ctrl-k      Kill agent", color: rc.fg)
                Text(content: "  l           Relaunch agent", color: rc.fg)
                Text(content: "  s           Split mode", color: rc.fg)
                Text(content: "", color: rc.fg)
                Text(content: "Other:", color: rc.fg)
                Text(content: "  1/2/3       Switch theme", color: rc.fg)
                Text(content: "  ?/h/F1      This help", color: rc.fg)
                Text(content: "  q/Esc       Quit/Close", color: rc.fg)
            }

            // Footer
            Box(height: 1u32, background_color: rc.bg) {
                Text(content: "Press any key to close", color: rc.dim)
            }
        }
    }
}
