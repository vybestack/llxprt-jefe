//! New agent form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-004
//! @pseudocode component-001 lines 29-33

use iocraft::prelude::*;

use crate::state::{AgentFormCursor, AgentFormFocus, AppState, ModalState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the new agent form.
#[derive(Default, Props)]
pub struct NewAgentFormProps {
    /// Application state (cloned).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

fn render_text_with_caret(value: &str, cursor: usize) -> String {
    let char_len = value.chars().count();
    let clamped = cursor.min(char_len);

    let byte_idx = if clamped == 0 {
        0
    } else {
        value
            .char_indices()
            .nth(clamped)
            .map_or_else(|| value.len(), |(idx, _)| idx)
    };

    format!("{}▏{}", &value[..byte_idx], &value[byte_idx..])
}

/// Form for creating/editing an agent.
#[component]
pub fn NewAgentForm(props: &NewAgentFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    // Extract form state from modal
    let (title, fields, focus, cursor) = props.state.as_ref().map_or_else(
        || {
            (
                "New Agent",
                crate::state::AgentFormFields::default(),
                AgentFormFocus::default(),
                AgentFormCursor::default(),
            )
        },
        |state| match &state.modal {
            ModalState::NewAgent {
                fields,
                focus,
                cursor,
                ..
            } => ("New Agent", fields.clone(), *focus, cursor.clone()),
            ModalState::EditAgent {
                fields,
                focus,
                cursor,
                ..
            } => ("Edit Agent", fields.clone(), *focus, cursor.clone()),
            _ => (
                "New Agent",
                crate::state::AgentFormFields::default(),
                AgentFormFocus::default(),
                AgentFormCursor::default(),
            ),
        },
    );

    // Build field lines with cursor indicator for focused field.
    let shortcut_display = fields
        .shortcut_slot
        .map_or_else(|| "none".to_owned(), |slot| slot.to_string());

    let labels = [
        "Shortcut (1-9)",
        "Name",
        "Description",
        "Work Dir",
        "Profile",
        "Mode Flags",
        "LLXPRT_DEBUG",
    ];
    let values = [
        &shortcut_display,
        &fields.name,
        &fields.description,
        &fields.work_dir,
        &fields.profile,
        &fields.mode,
        &fields.llxprt_debug,
    ];
    let focuses = [
        AgentFormFocus::Shortcut,
        AgentFormFocus::Name,
        AgentFormFocus::Description,
        AgentFormFocus::WorkDir,
        AgentFormFocus::Profile,
        AgentFormFocus::Mode,
        AgentFormFocus::LlxprtDebug,
    ];
    let cursors = [
        0,
        cursor.name,
        cursor.description,
        cursor.work_dir,
        cursor.profile,
        cursor.mode,
        cursor.llxprt_debug,
    ];

    let mut field_lines: Vec<AnyElement<'static>> = labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
        .map(|(((label, value), field_focus), field_cursor)| {
            let is_focused = focus == *field_focus;
            let rendered_value = if is_focused && *field_focus != AgentFormFocus::Shortcut {
                render_text_with_caret(value, *field_cursor)
            } else {
                (*value).to_owned()
            };
            let display = format!("  {label:<16} [{rendered_value}]");
            let color = if is_focused { rc.bright } else { rc.fg };
            element! {
                Box(height: 1u32) {
                    Text(content: display, color: color)
                }
            }
            .into()
        })
        .collect();

    // Pass/continue checkbox.
    let continue_focused = focus == AgentFormFocus::PassContinue;
    let continue_mark = if fields.pass_continue { "x" } else { " " };
    let continue_color = if continue_focused { rc.bright } else { rc.fg };
    let continue_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Pass --continue", continue_mark,
    );
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(content: continue_line, color: continue_color)
            }
        }
        .into(),
    );

    // Sandbox checkbox.
    let sandbox_focused = focus == AgentFormFocus::Sandbox;
    let sandbox_mark = if fields.sandbox_enabled { "x" } else { " " };
    let sandbox_color = if sandbox_focused { rc.bright } else { rc.fg };
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(
                    content: format!("  {:<16} [{}]  (space toggles)", "Sandbox", sandbox_mark),
                    color: sandbox_color
                )
            }
        }
        .into(),
    );

    // Sandbox engine pseudo-dropdown.
    let engine_focused = focus == AgentFormFocus::SandboxEngine;
    let engine_color = if engine_focused { rc.bright } else { rc.fg };
    let engine_hint = if fields.sandbox_enabled {
        "space cycles"
    } else {
        "disabled"
    };
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(
                    content: format!("  {:<16} [{}]  ({engine_hint})", "Sandbox Engine", fields.sandbox_engine),
                    color: engine_color
                )
            }
        }
        .into(),
    );

    // Sandbox flags field.
    let flags_focused = focus == AgentFormFocus::SandboxFlags;
    let flags_color = if flags_focused { rc.bright } else { rc.fg };
    let flags_value = if flags_focused {
        render_text_with_caret(&fields.sandbox_flags, cursor.sandbox_flags)
    } else {
        fields.sandbox_flags.clone()
    };
    let flags_display = format!("  {:<16} [{}]", "Sandbox Flags", flags_value);
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(content: flags_display, color: flags_color)
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
                    Text(content: "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles/cycles checkboxes  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
