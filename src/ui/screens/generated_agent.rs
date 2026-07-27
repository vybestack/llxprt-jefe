//! Thin renderer for a definition-generated New Agent form.

use iocraft::prelude::*;

use crate::selection::generated_form_content;
use crate::state::{AppState, ModalState};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the generated agent form.
#[derive(Default, Props)]
pub struct GeneratedAgentFormProps {
    /// Application state snapshot.
    pub state: Option<AppState>,
    /// Resolved theme source.
    pub colors: Option<ThemeColors>,
    /// Terminal rows available to the modal.
    pub available_rows: u16,
}

fn render_lines<'a>(
    lines: impl Iterator<Item = &'a String>,
    selection: Option<crate::selection::TextSelection>,
    foreground: Color,
    selection_colors: SelectionColors,
    offset: usize,
) -> Vec<AnyElement<'static>> {
    lines
        .enumerate()
        .map(|(index, line)| {
            selectable_line(
                line,
                offset + index,
                selection,
                crate::selection::SelectablePane::AgentForm,
                foreground,
                selection_colors,
            )
        })
        .collect()
}

/// Render the generic form projection without product-specific branches.
#[component]
pub fn GeneratedAgentForm(props: &GeneratedAgentFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let selection_colors = SelectionColors::from_resolved(&rc);
    let selection = props.state.as_ref().and_then(|state| state.selection);
    let projected = props
        .state
        .as_ref()
        .and_then(|state| match &state.modal {
            ModalState::GeneratedAgent { form, .. } => {
                Some(generated_form_content::content_lines(form, 100))
            }
            _ => None,
        })
        .unwrap_or_default();
    let action_start = projected
        .iter()
        .position(|line| line.contains("[Create "))
        .unwrap_or(projected.len());
    let action_count = projected.len().saturating_sub(action_start);
    let body_capacity = usize::from(props.available_rows)
        .saturating_sub(action_count + 2)
        .max(1);
    let body_lines = render_lines(
        projected.iter().take(action_start).take(body_capacity),
        selection,
        rc.fg,
        selection_colors,
        0,
    );
    let action_lines = render_lines(
        projected.iter().skip(action_start),
        selection,
        rc.fg,
        selection_colors,
        action_start,
    );

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
        ) {
            Box(
                border_style: BorderStyle::Round,
                border_color: rc.border_focused,
                background_color: rc.bg,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                padding: 1i32,
            ) {
                Box(flex_direction: FlexDirection::Column, flex_grow: 1.0_f32) {
                    #(body_lines)
                }
                #(action_lines)
            }
        }
    }
}
