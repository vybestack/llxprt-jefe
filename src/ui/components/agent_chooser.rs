//! Send-to-agent chooser overlay.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-011

use iocraft::prelude::*;

use crate::domain::AgentId;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the agent chooser overlay.
#[derive(Default, Props)]
pub struct AgentChooserProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// (agent_id, display_name) pairs for available agents.
    pub agents: Vec<(AgentId, String)>,
    /// Currently highlighted agent index.
    pub selected_index: usize,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Agent chooser overlay — lists existing agents with selection; Enter confirms, Esc cancels.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-011
#[component]
pub fn AgentChooser(props: &AgentChooserProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Double,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
            padding_top: 0u32,
            padding_bottom: 0u32,
        ) {
            // Header
            Box(height: 1u32) {
                Text(
                    content: "Send to Agent",
                    weight: Weight::Bold,
                    color: rc.bright,
                )
            }
            Box(height: 1u32) {
                Text(content: super::SEPARATOR_LINE, color: rc.dim)
            }

            // Agent list or empty state
            #(if props.agents.is_empty() {
                vec![element! {
                    Box(height: 2u32, padding_left: 1u32) {
                        Text(
                            content: "No agents available. Create an agent in Agents Mode.",
                            color: rc.dim,
                        )
                    }
                }]
            } else {
                props.agents.iter().enumerate().map(|(i, (_id, name))| {
                    let selected = i == props.selected_index;
                    let marker = if selected { "(x)" } else { "( )" };
                    let label = format!("{marker} {name}");
                    if selected {
                        element! {
                            Box(height: 1u32, background_color: rc.sel_bg) {
                                Text(content: label, color: rc.sel_fg, weight: Weight::Bold)
                            }
                        }
                    } else {
                        element! {
                            Box(height: 1u32) {
                                Text(content: label, color: rc.fg)
                            }
                        }
                    }
                }).collect()
            })

            // Action hints
            Box(height: 1u32) {
                Text(content: super::SEPARATOR_LINE, color: rc.dim)
            }
            Box(height: 1u32) {
                Text(content: "Enter send  ", color: rc.dim)
                Text(content: "Esc cancel", color: rc.dim)
            }
        }
    }
}
