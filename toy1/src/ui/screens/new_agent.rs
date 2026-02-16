//! New agent form screen.

use iocraft::prelude::*;

use crate::app::{AppState, Screen};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the new agent form screen.
#[derive(Default, Props)]
pub struct NewAgentFormProps {
    /// Application state (cloned snapshot).
    pub state: Option<AppState>,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// New agent form screen.
#[component]
pub fn NewAgentForm(props: &NewAgentFormProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let state = props.state.as_ref();

    let repo_name = state
        .and_then(AppState::current_repo)
        .map_or("(none)".to_owned(), |r| r.name.clone());

    let title = if state.map_or(false, |s| s.screen == Screen::EditAgent) {
        format!(" Edit Agent  (repo: {})", repo_name)
    } else {
        format!(" New Agent  (repo: {})", repo_name)
    };

    let fields = state.map(|s| &s.new_agent_fields);
    let focus = state.map_or(0, |s| s.new_agent_focus);

    let labels = ["Name", "Description", "Work dir", "Profile", "Mode"];
    let pass_continue = state.map_or(true, |s| s.new_agent_pass_continue);

    let mut field_lines: Vec<AnyElement<'static>> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let value = fields
                .and_then(|f| f.get(i))
                .map_or(String::new(), |v| v.clone());
            let is_focused = i == focus;
            let display = if is_focused {
                format!("  {:<16} [{}_]", label, value)
            } else {
                format!("  {:<16} [{}]", label, value)
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

    let continue_focused = focus == 5;
    let continue_mark = if pass_continue { "x" } else { " " };
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
                    Text(content: "  Tab/Down next  Shift+Tab/Up prev  Space toggle checkbox  Enter submit  Esc cancel".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
