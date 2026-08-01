//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-TECH-001

mod action_projection;
/// Shared finite-width Actions detail body projection.
pub mod actions_detail_projection;
pub mod actions_detail_view;
pub mod actions_view;
pub mod agent_candidate;
pub mod agent_candidate_fingerprint;
pub mod agent_candidate_path;
pub mod agent_detection;
pub mod agent_registry;
pub mod agent_status_view;
/// Provider-free binding explanation using the composed action registry.
pub mod binding_explain;
#[cfg(test)]
mod binding_explain_tests;
/// Detects whether the console font supports rounded-corner box-drawing
/// glyphs and falls back to single-line borders when it does not (issue #497).
pub mod border_capability;
pub mod cli;
/// OSC 52 clipboard writer with tmux / GNU screen passthrough.
pub mod clipboard;
/// Static descriptors for configuration owners built into this executable.
pub mod config_owners;
/// Resolved dashboard Git display data shared by rendering and selection copy.
pub mod dashboard_git_info;
/// Read-only local readiness diagnostics for `jefe doctor` (issue #264).
pub mod doctor;
pub mod domain;
pub mod input;
pub mod issue_detail_content;
pub(crate) mod keys_view;
#[cfg(test)]
#[path = "keys_view_tests.rs"]
mod keys_view_tests;
pub mod layout;
/// Pure geometry, windowing, navigation, and row-width primitives for selectable lists.
pub mod list_viewport;
/// Explicit local Git and GitHub CLI executable resolution.
pub mod local_command;
pub mod logging;
/// Single-pass HTML-to-text stripping for untrusted markdown (issue #155).
#[doc(hidden)]
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
pub mod pr_diff_content;
#[cfg(test)]
mod pr_diff_content_tests;
/// Provider-free configuration recovery command boundary.
pub mod recovery;
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
/// I/O-free screen descriptors and the sole executable layout resolver.
pub mod workbench;

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

/// JSP (Jefe Stream Protocol) external wire boundary (issue #476).
pub mod jsp;
