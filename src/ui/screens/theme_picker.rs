//! Theme picker screen — full overlay listing available themes.
//!
//! Pure projection lives in `state::theme_picker_view`; this is the thin
//! iocraft rendering layer that consumes the `(rows, selected_index)` pair.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-009

use iocraft::prelude::*;

use crate::state::AppState;
use crate::state::theme_picker_view::theme_picker_view;
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
    let rows = props
        .state
        .as_ref()
        .and_then(theme_picker_view)
        .unwrap_or_default();

    // Size the panel to fit available themes, capped at terminal height.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let panel_width = term_cols.saturating_sub(20).clamp(40, 60);
    let content_rows = u16::try_from(rows.len() + 6).unwrap_or(u16::MAX);
    let max_height = term_rows.saturating_sub(4);
    // clamp(min, max) panics if min > max, so use min(max_height, max(content_rows, 10)).
    let min_height = 10u16.min(max_height);
    let panel_height = content_rows.clamp(min_height, max_height);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            background_color: rc.bg,
        ) {
            Box(
                flex_direction: FlexDirection::Column,
                width: u32::from(panel_width),
                height: u32::from(panel_height),
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
                // Active theme (currently applied) gets a leading dot.
                Box(
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0_f32,
                    background_color: rc.bg,
                ) {
                    #(rows.iter().map(|row| {
                        let is_selected = row.selected;
                        let marker = if is_selected { "▶ " } else { "  " };
                        let active_marker = if row.active { " ●" } else { ""  };
                        let label = format!("{marker}{}{active_marker}", row.name);
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
