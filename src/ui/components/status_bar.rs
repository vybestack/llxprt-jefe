//! Status bar component - top bar with counts and theme name.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the status bar component.
#[derive(Default, Props)]
pub struct StatusBarProps {
    /// Number of repositories.
    pub repo_count: usize,
    /// Number of running agents.
    pub running_count: usize,
    /// Total number of agents.
    pub agent_count: usize,
    /// Active theme name.
    pub theme_name: String,
    /// App version string.
    pub version: String,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Status bar showing app title and statistics.
#[component]
pub fn StatusBar(props: &StatusBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    let stats = format!(
        "{} repos | {}/{} running",
        props.repo_count, props.running_count, props.agent_count
    );

    element! {
        Box(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            height: 1u32,
            background_color: rc.border,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            // Left: app title
            Text(
                content: format!("LLxprt Jefe - {}", props.version),
                weight: Weight::Bold,
                color: rc.bg,
            )

            // Center: stats
            Text(content: stats, color: rc.bg)

            // Right: theme name
            Text(content: props.theme_name.clone(), color: rc.bg)
        }
    }
}
