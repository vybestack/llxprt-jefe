//! Property editor overlay (issue #175).
//!
//! Renders the shared pure overlay projection used by selection content and
//! mouse geometry. The component owns no viewport, caret, footer, or bounds
//! calculations.

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::property_editor_view::{
    PropertyEditorProjectionInput, PropertyEditorProjectionLineKind,
    build_property_editor_projection,
};
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
    /// Cursor index into `options`.
    pub selected_index: usize,
    /// Whether multiple options can be selected simultaneously.
    pub multi_select: bool,
    /// Editable title text.
    pub title_text: String,
    /// Byte cursor within `title_text`.
    pub title_cursor: usize,
    /// Render title editing instead of options.
    pub is_title: bool,
    /// Current validation or mutation error.
    pub error: Option<String>,
    /// Live terminal width used by the shared projection.
    pub terminal_cols: u16,
    /// Live terminal height used by the shared projection.
    pub terminal_rows: u16,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight.
    pub selection: Option<TextSelection>,
}

/// Property editor overlay rendered directly from its pure projection.
#[component]
pub fn PropertyEditor(props: &PropertyEditorProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! { Box(width: 0u32, height: 0u32) {} };
    }
    let projection = build_property_editor_projection(PropertyEditorProjectionInput {
        header: &props.header,
        options: &props.options,
        selected_index: props.selected_index,
        multi_select: props.multi_select,
        title_text: &props.title_text,
        title_cursor: props.title_cursor,
        is_title: props.is_title,
        error: props.error.as_deref(),
        terminal_cols: props.terminal_cols,
        terminal_rows: props.terminal_rows,
    });
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::PropertyEditor;
    let lines: Vec<AnyElement<'static>> = projection
        .lines
        .iter()
        .enumerate()
        .map(|(line_index, line)| {
            let color = match line.kind {
                PropertyEditorProjectionLineKind::Header
                | PropertyEditorProjectionLineKind::Option { cursor: true, .. }
                | PropertyEditorProjectionLineKind::Footer { error: true } => rc.bright,
                PropertyEditorProjectionLineKind::Separator
                | PropertyEditorProjectionLineKind::Footer { error: false } => rc.dim,
                PropertyEditorProjectionLineKind::Option { cursor: false, .. }
                | PropertyEditorProjectionLineKind::Title => rc.fg,
            };
            selectable_line(&line.text, line_index, props.selection, pane, color, sel)
        })
        .collect();

    element! {
        Box(
            width: u32::from(projection.bounds.outer_width),
            height: u32::from(projection.bounds.outer_height),
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Double,
            border_color: rc.bright,
            background_color: rc.bg,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            #(lines)
        }
    }
}
