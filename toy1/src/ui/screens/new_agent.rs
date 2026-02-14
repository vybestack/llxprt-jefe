//! New agent form screen.

use iocraft::prelude::*;

use crate::app::AppState;
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
        .map_or("unknown".to_owned(), |r| r.name.clone());

    let form_lines: Vec<(String, Color)> = vec![
        ("".to_owned(), rc.fg),
        (format!("  Repository:  [{}]", repo_name), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Agent purpose:  [Fix issue #2010]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Prompt:".to_owned(), rc.fg),
        ("    Fix issue #2010. The TLS certificate renewal handler".to_owned(), rc.dim),
        ("    fails to properly restart the service after renewal.".to_owned(), rc.dim),
        ("    Update the handler to gracefully restart nginx.".to_owned(), rc.dim),
        ("".to_owned(), rc.fg),
        ("  Work dir:  [~/worktrees/llxprt-code-2010]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Profile:  [default]      Model:  [claude-opus-4-6]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Mode:  (●) --yolo  ( ) interactive".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
    ];

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
                    Text(content: " New Agent".to_owned(), color: rc.fg, weight: Weight::Bold)
                }

                #(form_lines.into_iter().map(|(line, color): (String, Color)| {
                    element! {
                        Box(height: 1u32) {
                            Text(content: line, color: color)
                        }
                    }
                }))

                Box(height: 1u32) {
                    Text(content: "  [Esc] Cancel   [Enter] Launch (toy - not functional)".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
