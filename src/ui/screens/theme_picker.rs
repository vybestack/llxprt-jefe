//! Theme picker screen — full overlay listing available themes.
//!
//! Pure projection lives in `state::theme_picker_view`; this is the thin
//! iocraft rendering layer that consumes the `(rows, selected_index)` pair.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-009

use iocraft::prelude::*;

use crate::state::AppState;
use crate::state::theme_picker_view::{ThemePickerRow, theme_picker_view};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the theme picker screen.
#[derive(Default, Props)]
pub struct ThemePickerScreenProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Full-screen theme picker overlay.
///
/// Renders a bordered panel listing themes with the selected row highlighted
/// (inverse video). Closes on Esc; selection on Enter is handled by the
/// input layer which emits `AppEvent::ThemePickerConfirm`.
#[component]
pub fn ThemePickerScreen(props: &ThemePickerScreenProps) -> impl Into<AnyElement<'static>> {
    let colors = props.colors.clone().unwrap_or_default();
    let rc = ResolvedColors::from_theme(Some(&colors));

    // Derive rows from the pure projection. Fall back to empty when no modal.
    let (rows, _selected) = props
        .state
        .as_ref()
        .and_then(theme_picker_view)
        .unwrap_or_else(|| (Vec::<ThemePickerRow>::new(), 0));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            Box(
                flex_direction: FlexDirection::Column,
                width: 50u32,
                height: 20u32,
                border_style: BorderStyle::Round,
                border_color: rc.border_focused,
                background_color: rc.bg,
                padding: 1u32,
            ) {
                // Title
                Box(height: 2u32, background_color: rc.bg) {
                    Text(
                        content: "Select Theme",
                        weight: Weight::Bold,
                        color: rc.fg,
                    )
                }

                // Theme list — selected row gets inverse-video highlight + marker.
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    background_color: rc.bg,
                ) {
                    #(rows.iter().map(|row| {
                        let is_selected = row.selected;
                        let marker = if is_selected { "▶ " } else { "  " };
                        let label = format!("{marker}{}", row.name);
                        element! {
                            Box(
                                width: 100pct,
                                background_color: if is_selected { rc.sel_bg } else { rc.bg },
                            ) {
                                Text(
                                    content: label,
                                    color: if is_selected { rc.sel_fg } else { rc.fg },
                                    weight: if is_selected { Weight::Bold } else { Weight::Normal },
                                )
                            }
                        }
                    }))
                }

                // Footer
                Box(height: 1u32, background_color: rc.bg) {
                    Text(
                        content: "↑/↓ navigate | Enter apply | Esc cancel",
                        color: rc.dim,
                    )
                }
            }
        }
    }
}
