//! Property editor overlay (issue #175).
//!
//! Mirrors the merge-chooser overlay (`merge_chooser.rs`). Renders a
//! selectable list of property options (Labels, Assignees, Milestone, Type,
//! State) or a single-line title text-box (Title kind). Up/Down navigates,
//! Space toggles (multi-select) or selects (single-select), Enter confirms,
//! Esc cancels.
//!
//! @requirement REQ-ISS-010

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the property editor overlay.
#[derive(Default, Props)]
pub struct PropertyEditorProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Header text, e.g. "Edit Labels - Issue #123".
    pub header: String,
    /// `(label, selected)` pairs for the option list.
    pub options: Vec<(String, bool)>,
    /// 0-based cursor index into `options`.
    pub selected_index: usize,
    /// Whether multiple options can be selected simultaneously
    /// (Labels/Assignees). Single-select (Milestone/Type/State) uses `>`
    /// cursor markers instead of `(x)`/`( )`.
    pub multi_select: bool,
    /// For Title kind: the editable text.
    pub title_text: String,
    /// Byte cursor within `title_text`.
    pub title_cursor: usize,
    /// Render the title text-box instead of the option list.
    pub is_title: bool,
    /// Error message shown in the footer when a mutation fails.
    pub error: Option<String>,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight (issue #178).
    pub selection: Option<TextSelection>,
}

const SEPARATOR: &str = "─────────────────────────────────────────";

/// Property editor overlay — lists options with selection, or a title
/// text-box; Enter confirms, Esc cancels.
///
/// @requirement REQ-ISS-010
#[component]
pub fn PropertyEditor(props: &PropertyEditorProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::PropertyEditor;
    let selection = props.selection;
    let mut line_idx: usize = 0;

    let mut lines: Vec<AnyElement<'static>> = Vec::new();

    // Header + separator
    lines.push(selectable_line(
        &props.header,
        {
            line_idx += 1;
            0
        },
        selection,
        pane,
        rc.bright,
        sel,
    ));
    lines.push(selectable_line(
        SEPARATOR,
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

    if props.is_title {
        lines.push(title_row_element(
            &props.title_text,
            &props.title_cursor.to_string(),
            rc.fg,
        ));
    } else {
        for (i, (label, selected)) in props.options.iter().enumerate() {
            let is_cursor = i == props.selected_index;
            let label_text = if props.multi_select {
                let marker = if *selected { "(x)" } else { "( )" };
                format!("{marker} {label}")
            } else {
                let marker = if is_cursor { ">" } else { " " };
                format!("{marker} {label}")
            };
            let color = if is_cursor { rc.bright } else { rc.fg };
            lines.push(selectable_line(
                &label_text,
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

    // Separator + footer hint
    lines.push(selectable_line(
        SEPARATOR,
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

    let hint = if let Some(ref err) = props.error {
        err.clone()
    } else if props.is_title {
        "type title  Ctrl+Enter apply  Esc cancel".to_string()
    } else if props.multi_select {
        "Up/Down move  Space toggle  Enter apply  Esc cancel".to_string()
    } else {
        "Up/Down move  Enter apply  Esc cancel".to_string()
    };
    let hint_color = if props.error.is_some() {
        rc.bright
    } else {
        rc.dim
    };
    lines.push(selectable_line(
        &hint, line_idx, selection, pane, hint_color, sel,
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

/// Render the title text-box row: a simple single-line display of the
/// editable title text with a visible caret marker.
fn title_row_element(text: &str, _cursor: &str, fg: Color) -> AnyElement<'static> {
    element! {
        Box(height: 1u32) {
            Text(content: text.to_string(), color: fg, wrap: TextWrap::NoWrap)
        }
    }
    .into()
}
