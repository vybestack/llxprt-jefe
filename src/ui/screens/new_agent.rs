//! New agent form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-004
//! @pseudocode component-001 lines 29-33

use iocraft::prelude::*;

use crate::state::{AgentFormFocus, AppState, ModalState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the new agent form.
#[derive(Default, Props)]
pub struct NewAgentFormProps {
    /// Application state (cloned).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Form for creating/editing an agent.
#[component]
pub fn NewAgentForm(props: &NewAgentFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    // Extract form state from modal
    let (title, fields, focus) = props.state.as_ref().map_or_else(
        || {
            (
                "New Agent",
                crate::state::AgentFormFields::default(),
                AgentFormFocus::default(),
            )
        },
        |state| match &state.modal {
            ModalState::NewAgent { fields, focus, .. } => ("New Agent", fields.clone(), *focus),
            ModalState::EditAgent { fields, focus, .. } => ("Edit Agent", fields.clone(), *focus),
            _ => (
                "New Agent",
                crate::state::AgentFormFields::default(),
                AgentFormFocus::default(),
            ),
        },
    );

    // Build field lines with cursor indicator for focused field
    let labels = ["Name", "Description", "Work Dir", "Profile", "Mode Flags"];
    let values = [
        &fields.name,
        &fields.description,
        &fields.work_dir,
        &fields.profile,
        &fields.mode,
    ];
    let focuses = [
        AgentFormFocus::Name,
        AgentFormFocus::Description,
        AgentFormFocus::WorkDir,
        AgentFormFocus::Profile,
        AgentFormFocus::Mode,
    ];

    let mut field_lines: Vec<AnyElement<'static>> = labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .map(|((label, value), field_focus)| {
            let is_focused = focus == *field_focus;
            let display = if is_focused {
                format!("  {label:<16} [{value}_]")
            } else {
                format!("  {label:<16} [{value}]")
            };
            let color = if is_focused { rc.bright } else { rc.fg };
            element! {
                Box(height: 1u32) {
                    Text(content: display, color: color)
                }
            }
            .into()
        })
        .collect();

    // Pass/continue checkbox
    let continue_focused = focus == AgentFormFocus::PassContinue;
    let continue_mark = if fields.pass_continue { "x" } else { " " };
    let continue_color = if continue_focused { rc.bright } else { rc.fg };
    let continue_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Pass --continue",
        continue_mark,
    );
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(content: continue_line, color: continue_color)
            }
        }
        .into(),
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
                flex_grow: 1.0,
                padding: 1i32,
            ) {
                Box(height: 1u32) {
                    Text(content: format!(" {}", title), color: rc.fg, weight: Weight::Bold)
                }
                Box(height: 1u32) {
                    Text(content: "".to_owned(), color: rc.fg)
                }

                #(field_lines)

                Box(height: 1u32) {
                    Text(content: "".to_owned(), color: rc.fg)
                }
                Box(height: 1u32) {
                    Text(content: "  Tab/Down next  Shift+Tab/Up prev  Space toggle checkbox  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
