//! Build script: captures the short git commit hash at compile time.
//!
//! Sets `JEFE_GIT_COMMIT` so the running binary can display its build identity
//! (issue #223). Falls back to "unknown" when git is unavailable or the build
//! directory is not inside a git working tree (e.g. a tarball extraction).

use std::process::Command;

fn main() {
    // Re-run when HEAD changes so the baked commit stays accurate.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let commit = git_short_commit().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=JEFE_GIT_COMMIT={commit}");
}

/// Run `git rev-parse --short HEAD` in the crate root, returning the trimmed
/// short hash. Returns `None` if git is missing or the directory is not a
/// working tree so the build never fails due to the identity lookup.
fn git_short_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
