//! Agent list projection for the generic [`SelectableList`] component.
//!
//! The agent list pane used to have its own iocraft `AgentList` component; the
//! rendering is now owned by [`crate::ui::components::SelectableList`]. This
//! module keeps the domain-specific projection ([`agent_list_props`]) that maps
//! each agent into a [`SelectableRow`] with a fixed-color status glyph span and
//! a themed name span.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002
//! @requirement REQ-FUNC-006

use crate::domain::{Agent, AgentStatus};
use crate::git_info::GitRepoInfo;
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;
use crate::ui::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
    SpanRole,
};

/// Selection state for the agent list (keyboard-selected index + optional
/// grabbed index for dashboard reordering).
///
/// Groups these related fields so [`agent_list_props`] stays under the
/// clippy `too_many_arguments` threshold, mirroring the `IssueListWindow` /
/// `PrListWindow` pattern.
#[derive(Debug, Clone, Copy, Default)]
pub struct AgentListSelection {
    /// Index of the keyboard-selected agent row.
    pub selected: usize,
    /// Index of the grabbed agent row (dashboard reorder), if any.
    pub grabbed: Option<usize>,
}

/// Status glyph rendered before the agent name (single character).
///
/// Matches the pre-refactor `AgentList` component's `status_icon` match arms.
fn status_icon(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Running => "*",
        AgentStatus::Completed => "+",
        AgentStatus::Dead => "x",
        AgentStatus::Errored => "!",
        AgentStatus::Waiting => "?",
        AgentStatus::Paused => "-",
        AgentStatus::Queued => "o",
    }
}

/// Semantic color role for the status glyph, immune to selection/highlight.
///
/// Matches the pre-refactor `AgentList` component's `status_color` match arms;
/// the generic [`SelectableList`] resolves the role against the theme.
fn status_role(status: AgentStatus) -> SpanRole {
    match status {
        AgentStatus::Running | AgentStatus::Completed => SpanRole::Bright,
        AgentStatus::Dead | AgentStatus::Errored => SpanRole::Red,
        AgentStatus::Waiting => SpanRole::Yellow,
        AgentStatus::Paused => SpanRole::Blue,
        AgentStatus::Queued => SpanRole::Dim,
    }
}

/// Build the prefix string for an agent row: `↕ ` when grabbed, `> ` when
/// selected, otherwise two spaces.
fn agent_prefix(is_selected: bool, grabbed: bool) -> &'static str {
    if grabbed {
        "\u{2195} "
    } else if is_selected {
        "> "
    } else {
        "  "
    }
}

/// Project one agent into a [`SelectableRow`].
///
/// Spans: prefix (themed), status glyph (fixed role), ` {shortcut}{name}`
/// (themed), and when git info is available, a dim suffix span
/// `  {origin} @ {branch}`.
fn to_selectable_row(
    agent: &Agent,
    is_selected: bool,
    grabbed: bool,
    git_info: Option<&GitRepoInfo>,
) -> SelectableRow {
    let shortcut_label = agent
        .shortcut_slot
        .map_or_else(String::new, |slot| format!("[{slot}] "));

    let mut spans = vec![
        SelectableSpan {
            text: agent_prefix(is_selected, grabbed).to_string(),
            color: SpanColor::Themed,
        },
        SelectableSpan {
            text: status_icon(agent.status).to_string(),
            color: SpanColor::Role(status_role(agent.status)),
        },
        SelectableSpan {
            text: format!(" {}{}", shortcut_label, agent.name),
            color: SpanColor::Themed,
        },
    ];

    // Append a dim git-info suffix when available (issue #170).
    if let Some(info) = git_info {
        let suffix = info.list_suffix();
        if !suffix.is_empty() {
            spans.push(SelectableSpan {
                text: format!("  {suffix}"),
                color: SpanColor::Role(SpanRole::Dim),
            });
        }
    }

    SelectableRow {
        spans,
        meta_line: None,
        is_selected,
    }
}

/// Build [`SelectableListProps`] for the agent list pane.
///
/// Projects each agent into a [`SelectableRow`] with a fixed-color status glyph
/// and a themed name span. When `git_infos` is provided and aligned with
/// `agents` by index, each row also shows the origin shortform and branch as a
/// dim suffix.
///
/// Uses the agent-style border/padding/selection policy so rendered output is
/// byte-identical to the pre-refactor `AgentList` component.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P09
/// @requirement REQ-FUNC-002
/// @requirement REQ-FUNC-006
#[must_use]
pub fn agent_list_props(
    agents: &[Agent],
    git_infos: &[GitRepoInfo],
    selection_state: AgentListSelection,
    focused: bool,
    colors: ThemeColors,
    selection: Option<TextSelection>,
) -> SelectableListProps {
    let rows = agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let is_selected = i == selection_state.selected;
            let is_grabbed = selection_state.grabbed.is_some_and(|idx| idx == i);
            let git_info = git_infos.get(i);
            to_selectable_row(agent, is_selected, is_grabbed, git_info)
        })
        .collect();
    SelectableListProps {
        title: "Agents".to_string(),
        rows,
        focused,
        empty_message: None,
        colors,
        selection,
        pane: SelectablePane::AgentList,
        border: ListBorder::RoundFocusedColor,
        content_padding: true,
        selection_style: SelectionStyle::BrightSelected,
    }
}
