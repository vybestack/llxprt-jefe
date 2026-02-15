//! New repository form screen.

use iocraft::prelude::*;

use crate::app::{AppState, Screen};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the new repository form screen.
#[derive(Default, Props)]
pub struct NewRepositoryFormProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// New repository form screen.
#[component]
pub fn NewRepositoryForm(props: &NewRepositoryFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let state = props.state.as_ref();

    let title = if state.map_or(false, |s| s.screen == Screen::EditRepository) {
        " Edit Repository".to_owned()
    } else {
        " New Repository".to_owned()
    };

    let fields = state.map(|s| &s.new_repository_fields);
    let focus = state.map_or(0, |s| s.new_repository_focus);

    let labels = ["Name", "Base dir", "Profile"];

    let field_lines: Vec<AnyElement<'static>> = labels.iter().enumerate().map(|(i, label)| {
        let value = fields
            .and_then(|f| f.get(i))
            .map_or(String::new(), |v| v.clone());
        let is_focused = i == focus;
        let display = if is_focused {
            format!("  {:<12} [{}_]", label, value)
        } else {
            format!("  {:<12} [{}]", label, value)
        };
        let color = if is_focused { rc.bright } else { rc.fg };
        element! {
            Box(height: 1u32) {
                Text(content: display, color: color)
            }
        }.into()
    }).collect();

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
                    Text(content: title, color: rc.fg, weight: Weight::Bold)
                }
                Box(height: 1u32) {
                    Text(content: "".to_owned(), color: rc.fg)
                }

                #(field_lines)

                Box(height: 1u32) {
                    Text(content: "".to_owned(), color: rc.fg)
                }
                Box(height: 1u32) {
                    Text(content: "  Tab next field  Shift+Tab prev  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
