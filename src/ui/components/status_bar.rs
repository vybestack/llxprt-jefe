//! Status bar component - top bar with counts and theme name.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection};
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
    /// Whether to show "(Kennel mode)" branding for a code_puppy agent.
    pub kennel_mode: bool,
    /// Optional warning text shown in the center status area.
    pub warning_message: Option<String>,
    /// Number of outstanding entries on the Errors screen (issue #435).
    /// Rendered as a bare count, never as error text: the bar is a clue that
    /// something failed, and the Errors screen holds the copyable detail.
    pub error_count: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any. When it targets this pane the whole
    /// single-line bar is painted in inverse-video.
    pub selection: Option<TextSelection>,
}

/// Build the center segment of the status bar.
///
/// Issue #435: this used to render `ERR: <first 50 chars>…`, which was too short
/// to diagnose anything, hid the running summary while displayed, and gave the
/// operator nowhere to go. It now reports only how many errors are outstanding,
/// leaving the full text to the Errors screen. Transient warnings keep their
/// slot — they are short and self-contained — but no longer suppress the clue.
#[must_use]
pub fn status_bar_stats(
    warning_message: Option<&str>,
    repo_count: usize,
    running_count: usize,
    agent_count: usize,
    error_count: usize,
) -> String {
    let base = match warning_message {
        Some(warning) => format!("WARN: {warning}"),
        None => format!("{repo_count} repos | {running_count}/{agent_count} running"),
    };
    if error_count == 0 {
        return base;
    }
    let noun = if error_count == 1 { "error" } else { "errors" };
    format!("{base} | {error_count} {noun}")
}

/// Status bar showing app title and statistics.
#[component]
pub fn StatusBar(props: &StatusBarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));

    // When a drag selection covers the single-line status bar, paint the whole
    // line in inverse-video so the user sees live feedback.
    let highlighted = props
        .selection
        .as_ref()
        .is_some_and(|s| s.pane() == SelectablePane::StatusBar);
    let bar_bg = if highlighted { rc.sel_bg } else { rc.border };
    let text_color = if highlighted { rc.sel_fg } else { rc.bg };

    let title_suffix = if props.kennel_mode {
        " (Kennel mode)"
    } else {
        ""
    };
    let stats = status_bar_stats(
        props.warning_message.as_deref(),
        props.repo_count,
        props.running_count,
        props.agent_count,
        props.error_count,
    );

    element! {
        Box(
            flex_direction: FlexDirection::Row,
            width: 100pct,
            height: 1u32,
            background_color: bar_bg,
            justify_content: JustifyContent::SpaceBetween,
            padding_left: 1u32,
            padding_right: 1u32,
        ) {
            // Left: app title
            Text(
                content: format!("LLxprt Jefe{title_suffix} - {}", props.version),
                weight: Weight::Bold,
                color: text_color,
            )

            // Center: stats
            Text(content: stats, color: text_color)

            // Right: theme name
            Text(content: props.theme_name.clone(), color: text_color)
        }
    }
}
