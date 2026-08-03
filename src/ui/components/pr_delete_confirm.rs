//! Pull-request delete confirm overlay (issue #183).
//!
//! Mirrors `issue_delete_confirm`. GitHub cannot delete a pull request, so the
//! overlay states exactly what will happen instead: the pull request is closed
//! when it is still open, and its head branch is removed. Both facts are named
//! before the first confirmation, so the second one is never a surprise.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the pull-request delete confirm overlay.
#[derive(Default, Props)]
pub struct PrDeleteConfirmProps {
    /// Whether the overlay is visible.
    pub visible: bool,
    /// Pull request number for the header.
    pub pr_number: u64,
    /// The branch that will be removed.
    pub head_ref: String,
    /// Whether the pull request is still open and will be closed first.
    pub is_open: bool,
    /// Whether the confirmation step is active.
    pub awaiting_confirmation: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// The header line for the overlay. Pure, so it is unit-testable directly.
#[must_use]
pub fn pr_delete_confirm_header(pr_number: u64) -> String {
    format!("Delete Pull Request #{pr_number}")
}

/// What the confirmation will actually do, stated before it is armed.
#[must_use]
pub fn pr_delete_confirm_effect(pr_number: u64, head_ref: &str, is_open: bool) -> String {
    if is_open {
        format!("Closes PR #{pr_number} and deletes branch {head_ref}.")
    } else {
        format!("Deletes branch {head_ref}.")
    }
}

/// The hint line, which changes once the overlay is armed. Pure.
#[must_use]
pub fn pr_delete_confirm_hint(awaiting_confirmation: bool) -> &'static str {
    if awaiting_confirmation {
        "Press Enter to confirm delete, Esc to cancel"
    } else {
        "Enter confirm, Esc cancel"
    }
}

/// Pull-request delete confirm overlay.
#[component]
pub fn PrDeleteConfirmOverlay(props: &PrDeleteConfirmProps) -> impl Into<AnyElement<'static>> {
    if !props.visible {
        return element! {
            Box(width: 0u32, height: 0u32) {}
        };
    }

    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let hint = pr_delete_confirm_hint(props.awaiting_confirmation);
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
                    content: pr_delete_confirm_header(props.pr_number),
                    weight: Weight::Bold,
                    color: rc.bright,
                )
            }
            Box(height: 1u32) {
                Text(
                    content: pr_delete_confirm_effect(
                        props.pr_number,
                        &props.head_ref,
                        props.is_open,
                    ),
                    color: rc.bright,
                )
            }
            Box(height: 1u32) {
                Text(content: "This action cannot be undone.", color: rc.bright)
            }
            Box(height: 1u32) {
                Text(content: super::SEPARATOR_LINE, color: rc.dim)
            }
            Box(height: 1u32) {
                Text(content: hint, color: rc.bright, weight: hint_weight)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{pr_delete_confirm_effect, pr_delete_confirm_header, pr_delete_confirm_hint};

    #[test]
    fn the_header_names_the_pull_request() {
        assert_eq!(pr_delete_confirm_header(42), "Delete Pull Request #42");
    }

    #[test]
    fn an_open_pull_request_is_told_it_will_be_closed_too() {
        let effect = pr_delete_confirm_effect(42, "feature/login", true);
        assert!(effect.contains("Closes PR #42"), "got: {effect}");
        assert!(effect.contains("feature/login"), "got: {effect}");
    }

    #[test]
    fn a_finished_pull_request_only_loses_its_branch() {
        let effect = pr_delete_confirm_effect(42, "feature/login", false);
        assert!(!effect.contains("Closes"), "got: {effect}");
        assert!(effect.contains("feature/login"), "got: {effect}");
    }

    #[test]
    fn the_hint_changes_once_the_overlay_is_armed() {
        assert_ne!(
            pr_delete_confirm_hint(false),
            pr_delete_confirm_hint(true),
            "an armed overlay must read differently from an unarmed one"
        );
        assert!(pr_delete_confirm_hint(true).contains("confirm delete"));
    }

    #[test]
    fn the_overlay_text_is_emoji_free() {
        let lines = [
            pr_delete_confirm_header(42),
            pr_delete_confirm_effect(42, "feature", true),
            pr_delete_confirm_hint(true).to_string(),
        ];
        for line in lines {
            assert!(line.is_ascii(), "UI text stays emoji-free: {line}");
        }
    }
}
