//! New repository form screen.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-003
//! @pseudocode component-001 lines 34-37

use iocraft::prelude::*;

use crate::state::{AppState, ModalState, RepositoryFormFocus};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the new repository form.
#[derive(Default, Props)]
pub struct NewRepositoryFormProps {
    /// Application state (cloned).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Form for creating/editing a repository.
#[component]
pub fn NewRepositoryForm(props: &NewRepositoryFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    // Extract form state from modal
    let (title, fields, focus) = props.state.as_ref().map_or_else(
        || {
            (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
            )
        },
        |state| match &state.modal {
            ModalState::NewRepository { fields, focus } => {
                ("New Repository", fields.clone(), *focus)
            }
            ModalState::EditRepository { fields, focus, .. } => {
                ("Edit Repository", fields.clone(), *focus)
            }
            _ => (
                "New Repository",
                crate::state::RepositoryFormFields::default(),
                RepositoryFormFocus::default(),
            ),
        },
    );

    // Build field lines with cursor indicator for focused field
    let labels = ["Name", "Base Dir", "Default Profile"];
    let values = [&fields.name, &fields.base_dir, &fields.default_profile];
    let focuses = [
        RepositoryFormFocus::Name,
        RepositoryFormFocus::BaseDir,
        RepositoryFormFocus::DefaultProfile,
    ];

    let field_lines: Vec<AnyElement<'static>> = labels
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
                    Text(content: "  Tab/Down next  Shift+Tab/Up prev  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
