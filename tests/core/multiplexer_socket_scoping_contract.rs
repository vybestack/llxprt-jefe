//! Tests must not be able to reach the developer's real multiplexer server.
//!
//! A test that runs `psmux new-session` without `-L` puts its session on the
//! *default* server -- the same one the developer's live agents are on. If it
//! then tears that server down, every one of those agents dies. There is no
//! error, nothing in the logs, and nothing obviously to blame: it presents as
//! agents dying spontaneously.
//!
//! This is not hypothetical. It happened (see #617), it cost hours of live
//! agent work, and the mistake was a single missing flag in a fixture that
//! drove `psmux` directly instead of going through `MultiplexerPlan`.
//!
//! The safe path already exists and is used by every psmux test in the tree:
//! build commands from a `MultiplexerPlan` carrying
//! `MultiplexerIsolation::Namespace`, which puts `-L <namespace>` on every
//! invocation. This contract makes that the only path, so the unsafe one --
//! which is one obvious line away -- cannot be written by accident.

use std::path::{Path, PathBuf};

/// Verbs that either create state on a server or destroy it. `new-session` is
/// included deliberately: landing on the default server is the actual defect,
/// and a test that creates there will usually clean up there too.
const MULTIPLEXER_VERBS: [&str; 4] = [
    "new-session",
    "kill-server",
    "kill-session",
    "attach-session",
];

/// Evidence that the file does not merely mention a verb but actually runs it.
const SPAWN_MARKERS: [&str; 3] = [".status()", ".output()", ".spawn()"];

/// Evidence that commands are scoped to a private server. `MultiplexerPlan` and
/// the harness driver both inject `-L`/`-S` themselves, so naming either counts.
const SCOPING_MARKERS: [&str; 6] = [
    "\"-L\"",
    "\"-S\"",
    "MultiplexerPlan",
    "plan.command",
    "PsmuxDriver",
    "base_args",
];

#[test]
fn tests_never_drive_the_multiplexer_without_scoping_it_to_a_private_server() {
    let mut offenders = Vec::new();

    for file in test_sources() {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if !text.lines().any(is_verb_line) {
            continue;
        }
        // A verb table or an assertion over argument strings is harmless; only
        // a file that actually launches something can reach a live server.
        if !SPAWN_MARKERS.iter().any(|marker| text.contains(marker)) {
            continue;
        }
        if SCOPING_MARKERS.iter().any(|marker| text.contains(marker)) {
            continue;
        }
        let line = text
            .lines()
            .position(is_verb_line)
            .map_or(0, |index| index + 1);
        offenders.push(format!("{} line {line}", display_path(&file)));
    }

    assert!(
        offenders.is_empty(),
        "these tests run multiplexer commands without scoping them to a private \
         server, so they can reach the developer's live agent sessions and kill \
         them:\n  {}\n\nBuild commands from a MultiplexerPlan carrying \
         MultiplexerIsolation::Namespace(unique_namespace()); it puts -L on every \
         invocation. See tests/psmux_attach.rs for the pattern, and #617 for what \
         happens without it.",
        offenders.join("\n  ")
    );
}

fn is_verb_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("///") {
        return false;
    }
    MULTIPLEXER_VERBS
        .iter()
        .any(|verb| line.contains(&format!("\"{verb}\"")))
}

/// Every Rust source that is compiled only for tests: the `tests/` tree, plus
/// in-crate modules whose names mark them as test-only.
fn test_sources() -> Vec<PathBuf> {
    let root = repo_root();
    let mut found = Vec::new();
    collect(&root.join("tests"), &mut found, &|_| true);
    collect(&root.join("src"), &mut found, &|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("_tests.rs"))
    });
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>, accept: &dyn Fn(&Path) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found, accept);
        } else if path.extension().is_some_and(|ext| ext == "rs") && accept(&path) {
            found.push(path);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}
