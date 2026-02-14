//! New repository form screen.

use iocraft::prelude::*;

use crate::app::AppState;
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

    let form_lines: Vec<(String, Color)> = vec![
        ("".to_owned(), rc.fg),
        ("  Repository name:  [my-new-repository]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Base directory:  [~/projects/my-new-repository]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Git repo URL:  [git@github.com:user/my-new-repository.git]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Default profile:  [default]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Default model:  [claude-opus-4-6]".to_owned(), rc.fg),
        ("".to_owned(), rc.fg),
        ("  Worktree strategy:  (o) git worktree  ( ) clone  ( ) manual".to_owned(), rc.fg),
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
                    Text(content: " New Repository".to_owned(), color: rc.fg, weight: Weight::Bold)
                }

                #(form_lines.into_iter().map(|(line, color): (String, Color)| {
                    element! {
                        Box(height: 1u32) {
                            Text(content: line, color: color)
                        }
                    }
                }))

                Box(height: 1u32) {
                    Text(content: "  [Esc] Cancel   [Enter] Create (toy - not functional)".to_owned(), color: rc.dim)
                }
            }
        }
    }
}
