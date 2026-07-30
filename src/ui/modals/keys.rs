//! Thin iocraft renderer for the pure Keys editor projection.

use iocraft::prelude::*;

use crate::keys_view::project_keys_view;
use crate::state::KeysEditorState;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the Keys editor modal.
#[derive(Default, Props)]
pub struct KeysModalProps {
    pub editor: Option<KeysEditorState>,
    pub colors: ThemeColors,
    pub available_cols: u16,
    pub available_rows: u16,
}

/// Render one bounded Keys editor projection.
#[component]
pub fn KeysModal(props: &KeysModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let view = props.editor.as_ref().map_or_else(
        || crate::keys_view::KeysView {
            title: "Keys - Keyboard Bindings".to_owned(),
            lines: Vec::new(),
            footer: "Esc Back | Ctrl-Q Quit".to_owned(),
        },
        |editor| project_keys_view(editor, props.available_cols, props.available_rows),
    );
    let width = u32::from(props.available_cols.saturating_sub(4).clamp(1, 100));
    let height = u32::from(props.available_rows.max(1));
    let lines: Vec<AnyElement<'static>> = view
        .lines
        .into_iter()
        .map(|line| element! { Text(content: line, color: rc.fg) }.into_any())
        .collect();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: width,
            height: height,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            Text(content: view.title, weight: Weight::Bold, color: rc.fg)
            #(lines)
            Box(flex_grow: 1.0_f32)
            Text(content: view.footer, color: rc.dim)
        }
    }
}
