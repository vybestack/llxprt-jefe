//! New repository form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 34-37

use iocraft::prelude::*;

use crate::state::{AppState, ModalState, RepositoryFormCursor, RepositoryFormFocus};
use crate::theme::{ResolvedColors, ThemeColors};

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

    // Build field lines with cursor indicator for focused field
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

    let mut field_lines: Vec<AnyElement<'static>> = labels
        .iter()
        .zip(values.iter())
        .zip(focuses.iter())
        .zip(cursors.iter())
        .map(|(((label, value), field_focus), field_cursor)| {
            let is_focused = focus == *field_focus;
            let rendered_value = if is_focused {
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

    let remote_focused = focus == RepositoryFormFocus::RemoteEnabled;
    let remote_mark = if fields.remote_enabled { "x" } else { " " };
    let remote_color = if remote_focused { rc.bright } else { rc.fg };
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(
                    content: format!("  {:<16} [{}]  (space toggles)", "Remote Repository", remote_mark),
                    color: remote_color
                )
            }
        }
        .into(),
    );

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
        field_lines.push(
            element! {
                Box(height: 1u32) {
                    Text(content: format!("  {label:<16} [{rendered_value}]"), color: color)
                }
            }
            .into(),
        );
    }

    let setup_focused = focus == RepositoryFormFocus::SetupEnvDefault;
    let setup_mark = if fields.setup_env_default { "x" } else { " " };
    let setup_color = if fields.remote_enabled {
        if setup_focused { rc.bright } else { rc.fg }
    } else {
        rc.dim
    };
    field_lines.push(
        element! {
            Box(height: 1u32) {
                Text(
                    content: format!("  {:<16} [{}]  (space toggles)", "Setup Env Default", setup_mark),
                    color: setup_color
                )
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
                flex_grow: 1.0_f32,
                padding: 1i32,
            ) {
                Box(height: 1u32) {
                    Text(content: format!(" {}", title), color: rc.fg, weight: Weight::Bold)
                }
                Box(height: 1u32) {
                    Text(content: String::new(), color: rc.fg)
                }

                #(field_lines)

                Box(height: 1u32) {
                    Text(content: String::new(), color: rc.fg)
                }
                Box(height: 1u32) {
                    Text(content: "  Tab/Down next  Shift+Tab/Up prev  Left/Right move cursor  Space toggles remote options  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
