//! New agent form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-004
//! @pseudocode component-001 lines 29-33

use iocraft::prelude::*;

use crate::domain::PlatformCapabilities;
use crate::selection::SelectablePane;
use crate::state::{AgentFormCursor, AgentFormFocus, AppState, ModalState};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;
use crate::ui::util::text_with_caret;

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
    let sel = SelectionColors::from_resolved(&rc);

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
    let selection = props.state.as_ref().and_then(|s| s.selection);
    let pane = SelectablePane::AgentForm;
    let mut line_idx: usize = 0;

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

    // Content line 0: title, line 1: blank.
    let mut all_lines: Vec<AnyElement<'static>> = Vec::new();
    all_lines.push(selectable_line(
        &format!(" {title}"),
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));
    all_lines.push(selectable_line(
        "",
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));

    // Content lines 2-8: text fields.
    for (((label, value), field_focus), field_cursor) in labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
    {
        let is_focused = focus == *field_focus;
        let rendered_value = if is_focused && *field_focus != AgentFormFocus::Shortcut {
            text_with_caret(value, *field_cursor)
        } else {
            (*value).to_owned()
        };
        let display = format!("  {label:<16} [{rendered_value}]");
        let color = if is_focused { rc.bright } else { rc.fg };
        all_lines.push(selectable_line(
            &display,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            color,
            sel,
        ));
    }

    // Content line 9: Pass --continue checkbox.
    let continue_focused = focus == AgentFormFocus::PassContinue;
    let continue_mark = if fields.pass_continue { "x" } else { " " };
    let continue_color = if continue_focused { rc.bright } else { rc.fg };
    let continue_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Pass --continue", continue_mark
    );
    all_lines.push(selectable_line(
        &continue_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        continue_color,
        sel,
    ));

    // Content line 10: Sandbox checkbox.
    let sandbox_focused = focus == AgentFormFocus::Sandbox;
    let sandbox_mark = if fields.sandbox_enabled { "x" } else { " " };
    let sandbox_color = if sandbox_focused { rc.bright } else { rc.fg };
    let sandbox_line = format!("  {:<16} [{}]  (space toggles)", "Sandbox", sandbox_mark);
    all_lines.push(selectable_line(
        &sandbox_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        sandbox_color,
        sel,
    ));

    // Content line 11: Sandbox engine pseudo-dropdown.
    let engine_focused = focus == AgentFormFocus::SandboxEngine;
    let engine_color = if engine_focused { rc.bright } else { rc.fg };
    let caps = PlatformCapabilities::current();
    let supported_engine_labels: Vec<&str> = caps
        .supported_engines()
        .iter()
        .map(|engine| engine.label())
        .collect();
    let engine_hint = if fields.sandbox_enabled {
        format!("space cycles: {}", supported_engine_labels.join(" / "))
    } else {
        String::from("disabled")
    };
    let engine_line = format!(
        "  {:<16} [{}]  ({engine_hint})",
        "Sandbox Engine", fields.sandbox_engine
    );
    all_lines.push(selectable_line(
        &engine_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        engine_color,
        sel,
    ));

    // Content line 12: Sandbox flags field.
    let flags_focused = focus == AgentFormFocus::SandboxFlags;
    let flags_color = if flags_focused { rc.bright } else { rc.fg };
    let flags_value = if flags_focused {
        text_with_caret(&fields.sandbox_flags, cursor.sandbox_flags)
    } else {
        fields.sandbox_flags.clone()
    };
    let flags_display = format!("  {:<16} [{}]", "Sandbox Flags", flags_value);
    all_lines.push(selectable_line(
        &flags_display,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        flags_color,
        sel,
    ));

    // Content line 13: blank, line 14: hints.
    all_lines.push(selectable_line(
        "",
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        rc.fg,
        sel,
    ));
    all_lines.push(selectable_line(
        "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles/cycles checkboxes  Enter submit  Esc cancel",
        line_idx,
        selection,
        pane,
        rc.dim,
        sel,
    ));

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
                #(all_lines)
            }
        }
    }
}
