//! Top status bar showing app name, repository count, and running agent count.

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the top status bar.
#[derive(Default, Props)]
pub struct StatusBarProps {
    /// Total repository count.
    pub repo_count: usize,
    /// Number of currently running agents.
    pub running_count: usize,
    /// Total agent count.
    pub agent_count: usize,
    /// Active theme name.
    pub theme_name: String,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Top-of-screen status bar.
#[component]
pub fn StatusBar(props: &StatusBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());

    let left = format!(" Jefe  {} repos", props.repo_count);
    let right = format!(
        "{}  running  {}  total  [{}] ",
        props.running_count, props.agent_count, props.theme_name
    );

    element! {
        Box(
            flex_direction: FlexDirection::Row,
            background_color: rc.bg,
            width: 100pct,
            height: 1u32,
        ) {
            Box(flex_grow: 1.0) {
                Text(content: left, color: rc.fg, weight: Weight::Bold)
            }
            Box() {
                Text(content: right, color: rc.dim)
            }
        }
    }
}
