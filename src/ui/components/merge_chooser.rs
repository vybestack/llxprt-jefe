//! Merge-method chooser overlay (issue #92).
//!
//! Mirrors the agent chooser overlay (`agent_chooser.rs`). Lists the three
//! GitHub merge methods, highlights the selection, and shows a confirmation
//! step before the merge mutation is dispatched.
//!
//! @requirement REQ-PR-009

use iocraft::prelude::*;

use crate::domain::{MERGE_METHODS, MergeMethod};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the merge chooser overlay.
#[derive(Default, Props)]
pub struct MergeChooserProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// PR number for the header.
    pub pr_number: u64,
    /// 0-based index into `MERGE_METHODS` for the highlighted method.
    pub selected_index: usize,
    /// Allowed methods from repo settings (`None` = not yet loaded, all shown).
    pub allowed_methods: Option<Vec<MergeMethod>>,
    /// Whether the confirmation step is active.
    pub awaiting_confirmation: bool,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight (issue #178).
    pub selection: Option<TextSelection>,
}

fn is_method_enabled(method: MergeMethod, allowed: Option<&[MergeMethod]>) -> bool {
    match allowed {
        None => true,
        Some(methods) => methods.contains(&method),
    }
}

/// Merge chooser overlay — lists merge methods with selection; Enter confirms,
/// Esc cancels.
///
/// @requirement REQ-PR-009
#[component]
pub fn MergeChooser(props: &MergeChooserProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::MergeChooser;
    let selection = props.selection;
    let mut line_idx: usize = 0;

    let mut lines: Vec<AnyElement<'static>> = Vec::new();

    // Header + separator
    lines.push(selectable_line(
        &format!("Merge Pull Request #{}", props.pr_number),
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

    // Method list
    for (i, method) in MERGE_METHODS.iter().enumerate() {
        let selected = i == props.selected_index;
        let enabled = is_method_enabled(*method, props.allowed_methods.as_deref());
        let label = if enabled {
            let marker = if selected { "(x)" } else { "( )" };
            format!("{marker} {}", method.label())
        } else {
            format!("    {} (not enabled)", method.label())
        };
        let color = if !enabled {
            rc.dim
        } else if selected {
            rc.bright
        } else {
            rc.fg
        };
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
    let hint = if props.awaiting_confirmation {
        "Press Enter to confirm merge, Esc to cancel"
    } else {
        "Up/Down select  Enter confirm  Esc cancel"
    };
    let hint_color = if props.awaiting_confirmation {
        rc.bright
    } else {
        rc.dim
    };
    lines.push(selectable_line(
        hint, line_idx, selection, pane, hint_color, sel,
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
