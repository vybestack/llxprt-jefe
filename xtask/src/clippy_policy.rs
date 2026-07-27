//! Placeholder for slice 2 (clippy-allow policy, A4 + A5).
//!
//! The real scanner is implemented in slice 2; this stub keeps the workspace
//! compiling while the skeleton lands.

use std::path::Path;

use crate::process::CommandFailed;

/// Run the clippy-allow policy against the repository root.
///
/// # Errors
/// Returns `CommandFailed` until slice 2 implements the real policy.
#[allow(clippy::missing_errors_doc)]
pub fn run_repo_check(_root: &Path) -> Result<(), CommandFailed> {
    Err(CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), "clippy-allows".into()],
        status: None,
        stdout: Vec::new(),
        stderr: b"clippy-allow policy not yet implemented (slice 2)".to_vec(),
    })
}
