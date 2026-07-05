//! Merge-method chooser overlay (issue #92).
//!
//! Mirrors the agent chooser overlay (`agent_chooser.rs`). Lists the three
//! GitHub merge methods, highlights the selection, and shows a confirmation
//! step before the merge mutation is dispatched.
//!
//! @requirement REQ-PR-009

use iocraft::prelude::*;

use crate::domain::{MERGE_METHODS, MergeMethod};
use crate::theme::{ResolvedColors, ThemeColors};

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
                    content: format!("Merge Pull Request #{}", props.pr_number),
                    weight: Weight::Bold,
                    color: rc.bright,
                )
            }
            Box(height: 1u32) {
                Text(content: "─────────────────────────────────────────", color: rc.dim)
            }

            // Method list
            #(
                MERGE_METHODS.iter().enumerate().map(|(i, method)| {
                    let selected = i == props.selected_index;
                    let enabled = is_method_enabled(*method, props.allowed_methods.as_deref());
                    let label = if enabled {
                        let marker = if selected { "(x)" } else { "( )" };
                        format!("{marker} {}", method.label())
                    } else {
                        format!("    {} (not enabled)", method.label())
                    };
                    if selected && enabled {
                        element! {
                            Box(height: 1u32, background_color: rc.sel_bg) {
                                Text(content: label, color: rc.sel_fg, weight: Weight::Bold)
                            }
                        }
                    } else if enabled {
                        element! {
                            Box(height: 1u32) {
                                Text(content: label, color: rc.fg)
                            }
                        }
                    } else {
                        element! {
                            Box(height: 1u32) {
                                Text(content: label, color: rc.dim)
                            }
                        }
                    }
                }).collect::<Vec<_>>()
            )

            // Separator
            Box(height: 1u32) {
                Text(content: "─────────────────────────────────────────", color: rc.dim)
            }

            // Confirmation or navigation hint
            #(if props.awaiting_confirmation {
                vec![element! {
                    Box(height: 1u32) {
                        Text(
                            content: "Press Enter to confirm merge, Esc to cancel",
                            color: rc.bright,
                            weight: Weight::Bold,
                        )
                    }
                }]
            } else {
                vec![element! {
                    Box(height: 1u32) {
                        Text(content: "Up/Down select  ", color: rc.dim)
                        Text(content: "Enter confirm  ", color: rc.dim)
                        Text(content: "Esc cancel", color: rc.dim)
                    }
                }]
            })
        }
    }
}
