//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

/// Shared finite-width Actions detail body projection.
pub mod actions_detail_projection;
pub mod actions_detail_view;
pub mod actions_view;
pub mod agent_detection;
pub mod cli;
/// OSC 52 clipboard writer with tmux / GNU screen passthrough.
pub mod clipboard;
/// Resolved dashboard Git display data shared by rendering and selection copy.
pub mod dashboard_git_info;
pub mod domain;
pub mod input;
pub mod issue_detail_content;
pub mod layout;
/// Pure geometry, windowing, navigation, and row-width primitives for selectable lists.
pub mod list_viewport;
/// Explicit local Git and GitHub CLI executable resolution.
pub mod local_command;
pub mod logging;
/// Single-pass HTML-to-text stripping for untrusted markdown (issue #155).
pub mod markdown_html_strip;
/// Plain-text markdown rendering for the detail panes (issue #155).
pub mod markdown_render;
pub mod messages;
/// Boundary-owned display data for mouse-selection content projection.
pub mod pane_content_projection;
pub mod persistence;
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
pub mod pr_detail_content;
pub mod runtime;
/// Pure, iocraft-free mouse-selection model (pane geometry + text extraction).
pub mod selection;
pub mod services;
/// Native-host OpenSSH planning and typed failure classification.
pub mod ssh;
pub mod startup;
pub mod state;
/// Pure multiline text-box viewport projection (iocraft-free).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
pub mod text_box_view;
/// Pure, iocraft-free word-wrap projection shared by the editor and displayer.
///
/// @requirement REQ-TEXT-WRAP
pub mod text_wrap;
pub mod theme;
pub mod ui;

/// Cached git repository info (origin shortform + branch) for agent display.
pub mod git_info;
/// @plan PLAN-20260329-ISSUES-MODE.P03
pub mod github;

/// Current application version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git commit hash baked in at build time (issue #223).
///
/// Falls back to `"unknown"` when the crate was built outside a git working
/// tree (e.g. a tarball) so display code never has to branch on availability.
pub const GIT_COMMIT: &str = match option_env!("JEFE_GIT_COMMIT") {
    Some(commit) => commit,
    None => "unknown",
};

/// Format the process-identity label shown in the lower-right corner so the
/// running jefe can always be identified (issue #223).
///
/// The format is `pid:{pid} {commit}` — compact and greppable. The function is
/// pure so render code and selection-copy projections share one source of
/// truth and it can be unit-tested without a process or git working tree.
#[must_use]
pub fn process_identity_label(pid: u32, commit: &str) -> String {
    format!("pid:{pid} {commit}")
}

pub mod harness;
