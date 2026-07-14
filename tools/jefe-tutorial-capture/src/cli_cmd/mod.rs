//! CLI command modules for the tutorial-capture binary.
//!
//! Each submodule implements one subcommand or supporting helper group,
//! extracted to keep file sizes under the project limit.

pub mod capture_flow;
pub mod cleanup_cmd;
pub mod cli;
pub mod commands;
pub mod plan_github;
pub mod svg_helpers;
pub mod tmux_helpers;
