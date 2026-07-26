//! New Issue dialog form modal (issue #407).
//!
//! Full-screen form modal (`ModalState::NewIssue`) mirroring the
//! `NewAgentForm`/`NewRepositoryForm` pattern. Renders the template picker,
//! title, body, and property pickers (labels/milestone/type/assignees).
//! Editing is driven by dedicated `NewIssue*` events (not the generic
//! `FormChar` events), routed by `app_input::new_issue_dialog`.

use iocraft::prelude::*;

use crate::selection::SelectablePane;
use crate::state::{AppState, ModalState, NewIssueDialogFocus, NewIssueTemplate};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;
use crate::ui::util::text_with_caret;

/// Props for the New Issue dialog form.
#[derive(Default, Props)]
pub struct NewIssueFormProps {
    /// Application state (cloned).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// New Issue dialog form modal.
#[component]
pub fn NewIssueForm(props: &NewIssueFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let sel = SelectionColors::from_resolved(&rc);

    let (template, type_name, title_text, title_cursor, body_text, body_cursor, focus, error) =
        props.state.as_ref().map_or_else(
            || {
                (
                    NewIssueTemplate::Blank,
                    None,
                    String::new(),
                    0,
                    String::new(),
                    0,
                    NewIssueDialogFocus::Title,
                    None,
                )
            },
            |state| match &state.modal {
                ModalState::NewIssue { state: d, .. } => (
                    d.template,
                    d.type_name.clone(),
                    d.title_text.clone(),
                    d.title_cursor,
                    d.body_text.clone(),
                    d.body_cursor,
                    d.focus,
                    d.error.clone(),
                ),
                _ => (
                    NewIssueTemplate::Blank,
                    None,
                    String::new(),
                    0,
                    String::new(),
                    0,
                    NewIssueDialogFocus::Title,
                    None,
                ),
            },
        );

    let selection = props.state.as_ref().and_then(|s| s.selection);
    let pane = SelectablePane::NewIssueForm;
    let mut line_idx: usize = 0;

    let mut all_lines: Vec<AnyElement<'static>> = Vec::new();

    all_lines.push(selectable_line(
        " New Issue",
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

    // Template picker line.
    let template_focused = focus == NewIssueDialogFocus::Template;
    let template_line = format!(
        "  {:<16} [{}]  (space cycles: Blank/Bug/Feature/Task)",
        "Template",
        template.label()
    );
    all_lines.push(selectable_line(
        &template_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if template_focused { rc.bright } else { rc.fg },
        sel,
    ));

    // Type picker line.
    let type_focused = focus == NewIssueDialogFocus::Type;
    let type_display = type_name.clone().unwrap_or_else(|| "—".to_string());
    let type_line = format!("  {:<16} [{}]  (space cycles)", "Type", type_display);
    all_lines.push(selectable_line(
        &type_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if type_focused { rc.bright } else { rc.fg },
        sel,
    ));

    // Title field.
    let title_focused = focus == NewIssueDialogFocus::Title;
    let title_value = if title_focused {
        text_with_caret(&title_text, title_cursor)
    } else {
        title_text.clone()
    };
    let title_line = format!("  {:<16} [{title_value}]", "Title");
    all_lines.push(selectable_line(
        &title_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if title_focused { rc.bright } else { rc.fg },
        sel,
    ));

    // Body field — render the first line with a caret when focused. The full
    // multi-line body is rendered as static lines below (the caret indicator
    // goes on the focused line only, mirroring the inline composer).
    let body_focused = focus == NewIssueDialogFocus::Body;
    let body_label = format!("  {:<16} [...]", "Body");
    all_lines.push(selectable_line(
        &body_label,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        if body_focused { rc.bright } else { rc.fg },
        sel,
    ));
    // Render body lines (up to a reasonable cap for the dialog). Use
    // split('\n') rather than lines() so a trailing newline produces a
    // visible empty line the caret can land on.
    let all_body_lines: Vec<&str> = body_text.split('\n').collect();
    let cap = 12usize;
    // Compute the caret line once before the loop (issue #407). The line
    // index is the number of '\n' chars before the cursor — robust for empty
    // lines and trailing newlines where `str::lines()` undercounts.
    let caret_line: Option<usize> =
        body_focused.then(|| char_offset(body_text.as_str(), body_cursor));
    for (i, line) in all_body_lines.iter().take(cap).enumerate() {
        let is_caret_line = caret_line.is_some_and(|cl| cl == i);
        let display = if is_caret_line {
            format!("{line}▌")
        } else {
            (*line).to_string()
        };
        all_lines.push(selectable_line(
            &format!("  {display}"),
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            if is_caret_line { rc.bright } else { rc.dim },
            sel,
        ));
    }
    if all_body_lines.len() > cap {
        all_lines.push(selectable_line(
            "  ... (truncated)",
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.dim,
            sel,
        ));
    }

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

    // Error line (if any).
    if let Some(err) = &error {
        all_lines.push(selectable_line(
            &format!("  Error: {err}"),
            {
                let i = line_idx;
                line_idx += 1;
                i
            },
            selection,
            pane,
            rc.bright,
            sel,
        ));
    }

    all_lines.push(selectable_line(
        "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space cycles Template/Type  Enter submit (Alt+Enter from body)  Esc cancel",
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

/// Return the 0-based line index of a char-offset cursor within `text`.
///
/// Counts the number of newline characters before the cursor position. This
/// is robust for empty lines and trailing newlines, where
/// `text[..byte_idx].lines().count()` undercounts (a final trailing newline
/// is ignored by `lines()`, and `count().saturating_sub(1)` is also off-by-one
/// for the first line). Uses char-boundary-safe traversal so non-ASCII body
/// text does not panic.
fn char_offset(text: &str, cursor: usize) -> usize {
    text.chars().take(cursor).filter(|c| *c == '\n').count()
}
