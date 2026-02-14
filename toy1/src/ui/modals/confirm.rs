//! Confirmation dialog modal.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the confirm dialog.
#[derive(Default, Props)]
pub struct ConfirmModalProps {
    /// Whether the modal is visible.
    pub visible: bool,
    /// The title of the confirmation.
    pub title: String,
    /// The message to display.
    pub message: String,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Centered confirmation dialog.
#[component]
pub fn ConfirmModal(props: &ConfirmModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    if !props.visible {
        return element! { Box() };
    }

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
                Text(content: format!(" {}", props.title), color: rc.fg, weight: Weight::Bold)
            }
            Box(height: 1u32) {
                Text(content: "".to_owned(), color: rc.dim)
            }
            Box(height: 1u32) {
                Text(content: format!("  {}", props.message), color: rc.fg)
            }
            Box(height: 1u32, padding_top: 1i32) {
                Text(content: "  [Enter] Confirm   [Esc] Cancel".to_owned(), color: rc.dim)
            }
        }
    }
}
