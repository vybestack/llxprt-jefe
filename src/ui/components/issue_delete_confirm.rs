//! Issue delete confirm overlay (issue #182).
//!
//! Mirrors `merge_chooser.rs`. A simple two-step confirm overlay: first `D`
//! opens it (not armed), `Enter` arms it, a second `Enter` confirms (dispatches
//! the delete), `Esc` cancels.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the issue delete confirm overlay.
#[derive(Default, Props)]
pub struct IssueDeleteConfirmProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Issue number for the header.
    pub issue_number: u64,
    /// Whether the confirmation step is active.
    pub awaiting_confirmation: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// The header line for the delete overlay, parameterized by issue number.
/// Pure (no iocraft) so it can be unit-tested directly.
#[must_use]
pub fn delete_confirm_header(issue_number: u64) -> String {
    format!("Delete Issue #{issue_number}")
}

/// The hint line for the delete overlay depending on the armed state.
/// Pure (no iocraft) so it can be unit-tested directly.
#[must_use]
pub fn delete_confirm_hint(awaiting_confirmation: bool) -> &'static str {
    if awaiting_confirmation {
        "Press Enter to confirm delete, Esc to cancel"
    } else {
        "Enter confirm  Esc cancel"
    }
}

/// Issue delete confirm overlay — shows a warning and a two-step confirm hint.
///
/// @requirement issue #182
#[component]
pub fn IssueDeleteConfirmOverlay(
    props: &IssueDeleteConfirmProps,
) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let hint = delete_confirm_hint(props.awaiting_confirmation);
    let hint_weight = if props.awaiting_confirmation {
        Weight::Bold
    } else {
        Weight::Normal
    };

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
            Box(height: 1u32) {
                Text(
                    content: delete_confirm_header(props.issue_number),
                    weight: Weight::Bold,
                    color: rc.bright,
                )
            }
            Box(height: 1u32) {
                Text(content: "This action cannot be undone.", color: rc.bright)
            }
            Box(height: 1u32) {
                Text(content: "─────────────────────────────────────────", color: rc.dim)
            }
            Box(height: 1u32) {
                Text(content: hint, color: rc.bright, weight: hint_weight)
            }
        }
    }
}
