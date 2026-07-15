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

    // Compute visibility mask from the current agent kind so Code Puppy hides
    // LLxprt-only controls.
    let visibility =
        crate::state::agent_form_visibility(crate::state::kind_from_form_value(&fields.agent_kind));

    // Build field lines with cursor indicator for focused field.
    let shortcut_display = fields
        .shortcut_slot
        .map_or_else(|| "none".to_owned(), |slot| slot.to_string());

    // Fields rendered BEFORE the Agent Runtime selector. The focus order is
    // WorkDir → Profile → AgentKind → Mode, so Agent Runtime is inserted
    // between Profile and Mode (see render below).
    let pre_kind_text_fields: [(&str, &str, AgentFormFocus, usize); 5] = [
        (
            "Shortcut (1-9)",
            &shortcut_display,
            AgentFormFocus::Shortcut,
            0,
        ),
        ("Name", &fields.name, AgentFormFocus::Name, cursor.name),
        (
            "Description",
            &fields.description,
            AgentFormFocus::Description,
            cursor.description,
        ),
        (
            "Work Dir",
            &fields.work_dir,
            AgentFormFocus::WorkDir,
            cursor.work_dir,
        ),
        (
            "Profile",
            &fields.profile,
            AgentFormFocus::Profile,
            cursor.profile,
        ),
    ];

    let post_kind_text_fields: std::boxed::Box<[_]> = vec![
        (
            "Model",
            &fields.code_puppy_model,
            AgentFormFocus::CodePuppyModel,
            cursor.code_puppy_model,
        ),
        (
            "Mode Flags",
            &fields.mode,
            AgentFormFocus::Mode,
            cursor.mode,
        ),
        (
            "Version",
            &fields.llxprt_version,
            AgentFormFocus::LlxprtVersion,
            cursor.llxprt_version,
        ),
        (
            "LLXPRT_DEBUG",
            &fields.llxprt_debug,
            AgentFormFocus::LlxprtDebug,
            cursor.llxprt_debug,
        ),
    ]
    .into_boxed_slice();

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

    // Content lines 2+: visible text fields before Agent Runtime (skips
    // LLxprt-only fields for Code Puppy so render order matches focus order).
    for (label, value, field_focus, field_cursor) in pre_kind_text_fields
        .iter()
        .copied()
        .filter(|(_, _, ff, _)| crate::state::is_field_visible(*ff, visibility))
    {
        let is_focused = focus == field_focus;
        let rendered_value = if is_focused && field_focus != AgentFormFocus::Shortcut {
            text_with_caret(value, field_cursor)
        } else {
            value.to_owned()
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

    // Agent Runtime selector — rendered immediately after Profile so the
    // visual order matches the focus/navigation order
    // (WorkDir → Profile → AgentKind → Mode). Uses the shared effective-agent-
    // kinds projection so the hint matches what Space actually cycles.
    let kind_focused = focus == AgentFormFocus::AgentKind;
    let kind_color = if kind_focused { rc.bright } else { rc.fg };
    let effective_kinds = effective_kinds_for_form(props.state.as_ref());
    let kind_hint = crate::state::effective_kinds_hint(&effective_kinds);
    let kind_line = format!(
        "  {:<16} [{}]  ({kind_hint})",
        "Agent Runtime", fields.agent_kind
    );
    all_lines.push(selectable_line(
        &kind_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        kind_color,
        sel,
    ));

    // Post-kind text fields (Mode Flags, LLXPRT_DEBUG) — rendered after Agent
    // Runtime so the visual order matches focus order.
    for (label, value, field_focus, field_cursor) in post_kind_text_fields
        .iter()
        .copied()
        .filter(|(_, _, ff, _)| crate::state::is_field_visible(*ff, visibility))
    {
        let is_focused = focus == field_focus;
        let rendered_value = if is_focused {
            text_with_caret(value, field_cursor)
        } else {
            value.to_owned()
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

    // Explicit Code Puppy YOLO boolean; unlike LLxprt mode flags this always
    // launches as exactly `--yolo true|false`.
    if !visibility.shows_llxprt_fields() {
        let yolo_focused = focus == AgentFormFocus::CodePuppyYolo;
        let yolo_mark = if fields.code_puppy_yolo { "x" } else { " " };
        let yolo_line = format!("  {:<16} [{}]  (space toggles)", "YOLO", yolo_mark);
        all_lines.push(selectable_line(
            &yolo_line,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if yolo_focused { rc.bright } else { rc.fg },
            sel,
        ));
    }

    // Code Puppy continuation is explicit and independent of LLxprt --continue.
    if !visibility.shows_llxprt_fields() {
        let focused = focus == AgentFormFocus::CodePuppyQuickResume;
        let mark = if fields.code_puppy_quick_resume.enabled() {
            "x"
        } else {
            " "
        };
        let line = format!("  {:<16} [{}]  (space toggles)", "Quick resume", mark);
        all_lines.push(selectable_line(
            &line,
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if focused { rc.bright } else { rc.fg },
            sel,
        ));
    }

    // Content line: Pass --continue checkbox (LLxprt-only).
    if visibility.shows_llxprt_fields() {
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
    }

    // Content line: Sandbox checkbox (LLxprt-only).
    if visibility.shows_llxprt_fields() {
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
    }

    // Content line: Sandbox engine pseudo-dropdown (LLxprt-only).
    if visibility.shows_llxprt_fields() {
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
    }

    // Content line: Sandbox flags field (LLxprt-only).
    if visibility.shows_llxprt_fields() {
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
    }

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

/// Resolve the effective agent kinds for the currently open agent form.
///
/// Uses the shared [`crate::state::effective_agent_kinds`] projection so the
/// form hint matches exactly what Space cycles. Remote-enabled repositories
/// offer both kinds regardless of the local installed snapshot; local
/// repositories offer only installed kinds.
fn effective_kinds_for_form(state: Option<&AppState>) -> Vec<crate::domain::AgentKind> {
    let Some(state) = state else {
        return Vec::new();
    };
    let is_remote = match &state.modal {
        ModalState::NewAgent { repository_id, .. } => state
            .repository_by_id(repository_id)
            .is_some_and(|r| r.remote.enabled),
        ModalState::EditAgent { id, .. } => state
            .repository_for_agent(id)
            .is_some_and(|r| r.remote.enabled),
        _ => false,
    };
    crate::state::effective_agent_kinds(&state.installed_agent_kinds, is_remote)
}
