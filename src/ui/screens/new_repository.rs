//! New repository form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 34-37

use iocraft::prelude::*;

use crate::selection::SelectablePane;
use crate::state::{AppState, ModalState, RepositoryFormCursor, RepositoryFormFocus};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Props for the new repository form.
#[derive(Default, Props)]
pub struct NewRepositoryFormProps {
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

/// Form for creating/editing a repository.
#[component]
pub fn NewRepositoryForm(props: &NewRepositoryFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let sel = SelectionColors::from_resolved(&rc);

    // Extract form state from modal
    let (title, fields, focus, cursor) = props.state.as_ref().map_or_else(
        || {
            (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
                RepositoryFormCursor::default(),
            )
        },
        |state| match &state.modal {
            ModalState::NewRepository {
                fields,
                focus,
                cursor,
                ..
            } => ("New Repository", fields.clone(), *focus, cursor.clone()),
            ModalState::EditRepository {
                fields,
                focus,
                cursor,
                ..
            } => ("Edit Repository", fields.clone(), *focus, cursor.clone()),
            _ => (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
                RepositoryFormCursor::default(),
            ),
        },
    );
    let selection = props.state.as_ref().and_then(|s| s.selection);
    let pane = SelectablePane::RepositoryForm;
    let mut line_idx: usize = 0;

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

    // Content lines 2-5: text fields.
    let labels = ["Name", "Base Dir", "Default Profile", "GitHub Repo"];
    let values = [
        &fields.name,
        &fields.base_dir,
        &fields.default_profile,
        &fields.github_repo,
    ];
    let focuses = [
        RepositoryFormFocus::Name,
        RepositoryFormFocus::BaseDir,
        RepositoryFormFocus::DefaultProfile,
        RepositoryFormFocus::GitHubRepo,
    ];
    let cursors = [
        cursor.name,
        cursor.base_dir,
        cursor.default_profile,
        cursor.github_repo,
    ];
    for (((label, value), field_focus), field_cursor) in labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
    {
        let is_focused = focus == *field_focus;
        let rendered_value = if is_focused {
            render_text_with_caret(value, *field_cursor)
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

    // Content line 6: Remote Repository checkbox.
    let remote_focused = focus == RepositoryFormFocus::RemoteEnabled;
    let remote_mark = if fields.remote_enabled { "x" } else { " " };
    let remote_color = if remote_focused { rc.bright } else { rc.fg };
    let remote_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Remote Repository", remote_mark
    );
    all_lines.push(selectable_line(
        &remote_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        remote_color,
        sel,
    ));

    // Content lines 7-9: remote fields.
    let remote_labels = ["Login User", "Host / IP", "Run As User"];
    let remote_values = [&fields.login_user, &fields.host, &fields.run_as_user];
    let remote_focuses = [
        RepositoryFormFocus::LoginUser,
        RepositoryFormFocus::Host,
        RepositoryFormFocus::RunAsUser,
    ];
    let remote_cursors = [cursor.login_user, cursor.host, cursor.run_as_user];
    for (((label, value), field_focus), field_cursor) in remote_labels
        .iter()
        .zip(remote_values.iter())
        .zip(remote_focuses.iter())
        .zip(remote_cursors.iter())
    {
        let is_focused = focus == *field_focus;
        let rendered_value = if is_focused {
            render_text_with_caret(value, *field_cursor)
        } else {
            (*value).to_owned()
        };
        let color = if fields.remote_enabled {
            if is_focused { rc.bright } else { rc.fg }
        } else {
            rc.dim
        };
        let display = format!("  {label:<16} [{rendered_value}]");
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

    // Content line 10: Setup Env Default checkbox.
    let setup_focused = focus == RepositoryFormFocus::SetupEnvDefault;
    let setup_mark = if fields.setup_env_default { "x" } else { " " };
    let setup_color = if fields.remote_enabled {
        if setup_focused { rc.bright } else { rc.fg }
    } else {
        rc.dim
    };
    let setup_line = format!(
        "  {:<16} [{}]  (space toggles)",
        "Setup Env Default", setup_mark
    );
    all_lines.push(selectable_line(
        &setup_line,
        {
            let i = line_idx;
            line_idx += 1;
            i
        },
        selection,
        pane,
        setup_color,
        sel,
    ));

    // Content line 11: blank, line 12: hints.
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
        "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles remote options  Enter submit  Esc cancel",
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
