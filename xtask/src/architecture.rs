//! Placeholder for slice 4 (architecture policy, A7).
//!
//! The real policy is implemented in slice 4; this stub keeps the workspace
//! compiling while the skeleton lands.

use std::path::Path;

use crate::process::CommandFailed;

/// Run the architecture policy against the repository root.
///
/// # Errors
/// Returns `CommandFailed` until slice 4 implements the real policy.
#[allow(clippy::missing_errors_doc)]
pub fn run_repo_check(_root: &Path) -> Result<(), CommandFailed> {
    Err(CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), "architecture".into()],
        status: None,
        stdout: Vec::new(),
        stderr: b"architecture policy not yet implemented (slice 4)".to_vec(),
    })
}
