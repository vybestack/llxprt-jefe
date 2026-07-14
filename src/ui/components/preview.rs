//! Preview component - agent details and todo list.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::domain::Agent;
use crate::git_info::GitRepoInfo;
use crate::list_viewport::fit_text_to_width;
use crate::selection::{SelectablePane, TextSelection, row_highlight_range};
use crate::theme::{ResolvedColors, ThemeColors};

const TODO_HEADER_ROW: usize = 6;
const NO_TASKS_ROW: usize = 7;

/// Props for the preview component.
#[derive(Default, Props)]
pub struct PreviewProps {
    /// Selected agent (if any).
    pub agent: Option<Agent>,
    /// Git display info (origin shortform + branch) for the selected agent.
    pub git_info: Option<GitRepoInfo>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Width of each physical content row after border and padding.
    pub content_width: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active drag selection for Preview physical rows.
    pub selection: Option<TextSelection>,
}

/// Project the exact finite-width physical text rows painted by the Preview pane.
#[must_use]
pub fn preview_content_lines(
    agent: Option<&Agent>,
    git_info: Option<&GitRepoInfo>,
    content_width: usize,
) -> Vec<String> {
    let lines = if let Some(agent) = agent {
        let repository = git_info
            .and_then(|info| info.origin_shortform.as_deref())
            .unwrap_or("(unknown)");
        let branch = git_info
            .and_then(|info| info.branch.as_deref())
            .unwrap_or("(unknown)");
        vec![
            format!("Name: {}", agent.name),
            format!("Status: {:?}", agent.status),
            format!("Repo: {repository}"),
            format!("Branch: {branch}"),
            format!("Dir: {}", agent.work_dir.display()),
            String::new(),
            "Todo:".to_string(),
            "  (no tasks)".to_string(),
        ]
    } else {
        vec!["No agent selected".to_string()]
    };
    lines
        .into_iter()
        .map(|line| fit_text_to_width(&line, content_width))
        .collect()
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
    let has_agent = props.agent.is_some();
    let content_lines = preview_content_lines(
        props.agent.as_ref(),
        props.git_info.as_ref(),
        props.content_width,
    );
    let content_width = u32::try_from(props.content_width).unwrap_or(u32::MAX);
    let content_children = content_lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let highlighted = props
                .selection
                .as_ref()
                .filter(|selection| selection.pane() == SelectablePane::Preview)
                .and_then(|selection| row_highlight_range(selection, index))
                .is_some();
            let color = if highlighted {
                rc.sel_fg
            } else if !has_agent || index == NO_TASKS_ROW {
                rc.dim
            } else {
                rc.fg
            };
            let background = if highlighted { rc.sel_bg } else { rc.bg };
            let weight = if has_agent && index == TODO_HEADER_ROW {
                Weight::Bold
            } else {
                Weight::Normal
            };
            element! {
                Box(height: 1u32, width: content_width, background_color: background) {
                    Text(content: line, color: color, weight: weight, wrap: TextWrap::NoWrap)
                }
            }
            .into_any()
        })
        .collect::<Vec<_>>();

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
                #(content_children)
            }
        }
    }
}
