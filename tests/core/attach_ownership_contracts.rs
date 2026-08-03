//! A failed attach must not conclude that an agent died (issue #306).
//!
//! Attaching is a viewer operation. It builds a terminal view of a session that
//! is already running, and it can fail for reasons the agent knows nothing
//! about. Concluding death from it removed the runtime mapping while the
//! session and worker kept running, which is how a live agent became untracked:
//! the row said Dead, nothing owned the tree, and nothing reaped it either.
//!
//! Liveness is the only thing entitled to conclude death, and it re-probes on
//! its own interval, so a session that really has gone is still resolved — by
//! evidence rather than by a failed attach.
//!
//! This is asserted in source because #306 has been open since 2026-07-14,
//! deferred out of one narrower PR after another. The call is a one-liner that
//! reads like tidy-up at the point of failure, which is exactly why it keeps
//! being written.

use std::path::{Path, PathBuf};

/// Modules whose failure paths are viewer-only and must never mark a session
/// dead.
const VIEWER_ONLY_MODULES: &[&str] = &["src/app_shell_attach.rs"];

#[test]
fn an_attach_failure_never_marks_a_session_dead() {
    for relative in VIEWER_ONLY_MODULES {
        let text = read_repo_text(relative);
        assert!(
            !text.contains("mark_session_dead"),
            "{relative} calls `mark_session_dead`. Attaching is a viewer \
             operation, so its failure is not evidence the session or worker \
             died; concluding death there orphans a live tree by removing the \
             mapping that owned it (issue #306)."
        );
    }
}

/// A relaunch that spawned successfully must not disown what it just started.
///
/// Checked as an adjacency rather than a whole-file ban, because `relaunch.rs`
/// legitimately marks a session dead elsewhere -- the psmux recovery path moves
/// a live record to its retained cache before recreating it. The defect is
/// specifically marking death *in the failure arm of an attach*, where a worker
/// was created and disowned in consecutive statements.
#[test]
fn a_relaunch_never_disowns_the_session_it_just_spawned() {
    let text = read_repo_text("src/app_input/relaunch.rs");
    for (index, window) in text.lines().collect::<Vec<_>>().windows(3).enumerate() {
        let opens_attach_failure =
            window[0].contains("Err(error) =") && window[0].contains("attach");
        if opens_attach_failure {
            assert!(
                !window[1..]
                    .iter()
                    .any(|line| line.contains("mark_session_dead")),
                "src/app_input/relaunch.rs line {} marks a session dead in the failure arm of an \
                 attach. The spawn above it succeeded, so a worker is running; disowning it there \
                 is how an agent is created and orphaned in consecutive statements (issue #306).",
                index + 1
            );
        }
    }
}

fn read_repo_text(relative: &str) -> String {
    let path = repo_path(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}
