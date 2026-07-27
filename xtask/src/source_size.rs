//! Placeholder for slice 3 (source-size policy, A6).

use std::path::Path;

use crate::process::CommandFailed;

/// Run the source-size policy against the repository root.
///
/// # Errors
/// Returns `CommandFailed` until slice 3 implements the real policy.
#[allow(clippy::missing_errors_doc)]
pub fn run_repo_check(_root: &Path) -> Result<(), CommandFailed> {
    Err(CommandFailed {
        program: "xtask".into(),
        args: vec!["check".into(), "source-size".into()],
        status: None,
        stdout: Vec::new(),
        stderr: b"source-size policy not yet implemented (slice 3)".to_vec(),
    })
}
