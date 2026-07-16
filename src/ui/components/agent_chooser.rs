//! Send-to-agent chooser overlay.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-011

use iocraft::prelude::*;

use crate::domain::{AgentChooserEntry, agent_chooser_label};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the agent chooser overlay.
#[derive(Default, Props)]
pub struct AgentChooserProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Typed entries for available agents.
    pub agents: Vec<AgentChooserEntry>,
    /// Whether the transient-agent slot is available (issue #213).
    pub transient_available: bool,
    /// Currently highlighted agent index.
    pub selected_index: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight (issue #178).
    pub selection: Option<TextSelection>,
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
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::AgentChooser;
    let selection = props.selection;
    let mut line_idx: usize = 0;

    let mut lines: Vec<AnyElement<'static>> = Vec::new();

    // Header + separator
    lines.push(selectable_line(
        "Send to Agent",
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.bright,
        sel,
    ));
    lines.push(selectable_line(
        super::SEPARATOR_LINE,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.dim,
        sel,
    ));

    // Agent list or empty state
    if props.agents.is_empty() && !props.transient_available {
        lines.push(selectable_line(
            "No agents available. Create an agent in Agents Mode.",
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.dim,
            sel,
        ));
        lines.push(selectable_line(
            "",
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.fg,
            sel,
        ));
    } else {
        for (i, entry) in props.agents.iter().enumerate() {
            let selected = i == props.selected_index;
            let marker = if selected { "(x)" } else { "( )" };
            let label = agent_chooser_label(entry);
            let display = format!("{marker} {label}");
            let color = if selected { rc.bright } else { rc.fg };
            lines.push(selectable_line(
                &display,
                {
                    let li = line_idx;
                    line_idx += 1;
                    li
                },
                selection,
                pane,
                color,
                sel,
            ));
        }
        if props.transient_available {
            let transient_idx = props.agents.len();
            let selected = transient_idx == props.selected_index;
            let marker = if selected { "(x)" } else { "( )" };
            let label = format!("{marker} Transient Agent");
            let color = if selected { rc.bright } else { rc.fg };
            lines.push(selectable_line(
                &label,
                {
                    let li = line_idx;
                    line_idx += 1;
                    li
                },
                selection,
                pane,
                color,
                sel,
            ));
        }
    }

    // Separator + hints
    lines.push(selectable_line(
        super::SEPARATOR_LINE,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.dim,
        sel,
    ));
    lines.push(selectable_line(
        "Enter send  Esc cancel",
        line_idx,
        selection,
        pane,
        rc.dim,
        sel,
    ));

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
            #(lines)
        }
    }
}
