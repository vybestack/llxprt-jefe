//! Confirm modal - confirmation dialogs for destructive actions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-004

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the confirm modal.
#[derive(Default, Props)]
pub struct ConfirmModalProps {
    /// Title of the confirmation dialog.
    pub title: String,
    /// Message to display.
    pub message: String,
    /// Whether to show delete work dir option.
    pub show_delete_work_dir: bool,
    /// Current state of delete work dir toggle.
    pub delete_work_dir: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Confirm modal for destructive actions (delete, kill).
#[component]
pub fn ConfirmModal(props: &ConfirmModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 50u32,
            height: 10u32,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            // Title
            Box(height: 2u32, background_color: rc.bg) {
                Text(
                    content: props.title.clone(),
                    weight: Weight::Bold,
                    color: rc.fg,
                )
            }

            // Message
            Box(flex_grow: 1.0, background_color: rc.bg) {
                Text(content: props.message.clone(), color: rc.fg)
            }

            // Delete work dir option (conditional)
            #(if props.show_delete_work_dir {
                let checkbox = if props.delete_work_dir { "[x]" } else { "[ ]" };
                element! {
                    Box(height: 1u32, background_color: rc.bg) {
                        Text(content: format!("{checkbox} Delete work directory"), color: rc.fg)
                    }
                }
            } else {
                element! {
                    Box {}
                }
            })

            // Buttons
            Box(
                flex_direction: FlexDirection::Row,
                height: 2u32,
                justify_content: JustifyContent::Center,
                background_color: rc.bg,
            ) {
                Text(content: "[ Cancel ]  ", color: rc.dim)
                Text(content: "[ Confirm ]", color: rc.bright)
            }
        }
    }
}
