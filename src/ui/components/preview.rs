//! Preview component - agent details and todo list.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::domain::Agent;
use crate::git_info::GitRepoInfo;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the preview component.
#[derive(Default, Props)]
pub struct PreviewProps {
    /// Selected agent (if any).
    pub agent: Option<Agent>,
    /// Git display info (origin shortform + branch) for the selected agent.
    pub git_info: Option<GitRepoInfo>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Preview pane showing details of the selected agent.
#[component]
pub fn Preview(props: &PreviewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            // Title
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Preview", weight: Weight::Bold, color: rc.fg)
            }

            // Content
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                #(if let Some(agent) = &props.agent {
                    let repo_line = props.git_info.as_ref()
                        .and_then(|g| g.origin_shortform.as_deref())
                        .unwrap_or("(unknown)");
                    let branch_line = props.git_info.as_ref()
                        .and_then(|g| g.branch.as_deref())
                        .unwrap_or("(unknown)");
                    element! {
                        Box(flex_direction: FlexDirection::Column) {
                            Text(content: format!("Name: {}", agent.name), color: rc.fg)
                            Text(content: format!("Status: {:?}", agent.status), color: rc.fg)
                            Text(content: format!("Repo: {repo_line}"), color: rc.fg)
                            Text(content: format!("Branch: {branch_line}"), color: rc.fg)
                            Text(content: format!("Dir: {}", agent.work_dir.display()), color: rc.fg)
                            Box(height: 1u32) {}
                            Text(content: "Todo:", weight: Weight::Bold, color: rc.fg)
                            Text(content: "  (no tasks)", color: rc.dim)
                        }
                    }
                } else {
                    element! {
                        Box {
                            Text(content: "No agent selected", color: rc.dim)
                        }
                    }
                })
            }
        }
    }
}
