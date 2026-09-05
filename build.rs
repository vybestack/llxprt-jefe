//! Build script: captures the short git commit hash at compile time.
//!
//! Sets `JEFE_GIT_COMMIT` so the running binary can display its build identity
//! (issue #223). Falls back to "unknown" when git is unavailable or the build
//! directory is not inside a git working tree (e.g. a tarball extraction).

#[path = "build_support/git_watch.rs"]
mod git_watch;

use std::path::Path;
use std::process::Command;

fn main() {
    // Watching `.git/HEAD` alone misses a fast-forward: the file is a
    // symbolic ref whose bytes never change when the branch tip moves
    // (issue #753). `git_watch` resolves the watch list through git plumbing
    // instead: an attached HEAD also watches the loose branch-ref file (even
    // while packed, because the absent-to-present transition is the next ref
    // update), and a detached HEAD needs only the HEAD file, which every
    // movement rewrites.
    let package_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|error| {
        panic!("cargo must provide CARGO_MANIFEST_DIR to the build script: {error}")
    });
    let root = Path::new(&package_root);
    for watched in git_watch::resolve(root).rerun_paths(root) {
        println!("cargo:rerun-if-changed={watched}");
    }

    let commit = git_short_commit().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=JEFE_GIT_COMMIT={commit}");

    // A plugin keys its provider binaries by exact build host triple (issue
    // #389). The triple is a build-time fact that `std` does not expose at
    // runtime: `env::consts` gives only architecture and OS, which cannot tell
    // `-gnu` from `-musl`, or `-msvc` from `-gnu` on Windows. Cargo does give
    // it to build scripts, so it is baked in here.
    // Cargo always sets TARGET for a build script. Falling back to a
    // placeholder would bake in a value that `HostTriple::parse` rejects,
    // breaking the type's invariant everywhere downstream, so a missing TARGET
    // fails the build loudly instead.
    let target = std::env::var("TARGET")
        .unwrap_or_else(|error| panic!("cargo must provide TARGET to the build script: {error}"));
    println!("cargo:rustc-env=JEFE_HOST_TRIPLE={target}");
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
