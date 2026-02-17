//! Preview component - agent details and todo list.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::domain::Agent;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the preview component.
#[derive(Default, Props)]
pub struct PreviewProps {
    /// Selected agent (if any).
    pub agent: Option<Agent>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Preview pane showing details of the selected agent.
#[component]
pub fn Preview(props: &PreviewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_color = if props.focused {
        rc.border_focused
    } else {
        rc.border
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: BorderStyle::Round,
            border_color: border_color,
            background_color: rc.bg,
        ) {
            // Title
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Preview", weight: Weight::Bold, color: rc.fg)
            }

            // Content
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                #(if let Some(agent) = &props.agent {
                    element! {
                        Box(flex_direction: FlexDirection::Column) {
                            Text(content: format!("Name: {}", agent.name), color: rc.fg)
                            Text(content: format!("Status: {:?}", agent.status), color: rc.fg)
                            Text(content: format!("Dir: {}", agent.work_dir.display()), color: rc.fg)
                            Box(height: 1u32) {}
                            Text(content: "Todo:", weight: Weight::Bold, color: rc.fg)
                            Text(content: "  (no tasks)", color: rc.dim)
                        }
                    }
                } else {
                    element! {
                        Box {
                            Text(content: "No agent selected", color: rc.dim)
                        }
                    }
                })
            }
        }
    }
}
