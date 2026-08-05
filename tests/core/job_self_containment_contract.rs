//! The dashboard must never join a kill-on-close Job it owns (issue #664).
//!
//! Issue #664 recorded jefe's entire process tree vanishing with no panic, no
//! event-log entry, no crash dump, and no exit status observed by the parent.
//! `KILL_ON_JOB_CLOSE` produces exactly that signature: the kernel terminates
//! every member of the Job the instant the last handle closes, with no unwind
//! and no notification. If the long-running dashboard process were ever both
//! the owner *and* a member of such a Job, releasing the guard -- or simply
//! binding it somewhere short-lived -- would kill jefe and everything beneath
//! it, silently.
//!
//! `job_object_tests` proves that closing the handle reaps the contained tree,
//! and `job_inheritance_tests` proves a spawned worker inherits membership.
//! Neither answers the question #664 raises, which is about *who* is in the
//! Job: both stay inside the session-host role and cannot observe whether some
//! other call site put the dashboard there too.
//!
//! That is a reachability property, so it is proven over the source. The
//! containment guard is not observable from outside the process without
//! `IsProcessInJob`, and this crate forbids `unsafe`, so a runtime probe of
//! "is the dashboard in a Job it owns" is not available. The reachability chain
//! is short, total, and mechanically checkable, and these tests fail the moment
//! a new call site shortens it.
//!
//! Ownership model: `dev-docs/standards/windows-session-ownership.md`.

use std::path::Path;

/// The only production function permitted to assign the *current* process to a
/// Job that the same process owns.
const SELF_ASSIGN_CALL: &str = "JobContainment::enable_for_current_process()";

fn read_repo_text(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
}

/// The part of a source file that ships in the binary.
///
/// Test-gated items are compiled out of production builds, so a call made from
/// one cannot put a running process into a Job. Dropping the item each
/// test-`cfg` attribute guards -- rather than truncating the file at the first
/// such attribute -- matters here because this crate wires sibling test modules
/// (`#[cfg(all(test, windows))] #[path = "..."] mod x_tests;`) *above*
/// production functions, so truncation would blind the scan to most of the
/// file and let a real call site through unseen.
fn production_text(contents: &str) -> String {
    let mut kept = String::new();
    let mut lines = contents.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let guards_test_only_item = trimmed.starts_with("#[cfg(")
            && trimmed.contains("test")
            && !trimmed.contains("not(test");
        if !guards_test_only_item {
            kept.push_str(line);
            kept.push('\n');
            continue;
        }
        let mut depth = 0usize;
        let mut opened = false;
        for item_line in lines.by_ref() {
            for character in item_line.chars() {
                match character {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if opened {
                if depth == 0 {
                    break;
                }
            } else if item_line.trim_end().ends_with(';') {
                break;
            }
        }
    }
    kept
}

/// Every `.rs` file under `src/`, as `(repo-relative path, contents)`.
fn source_files() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut collected = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", directory.display()));
        for entry in entries {
            let entry =
                entry.unwrap_or_else(|error| panic!("could not read a source entry: {error}"));
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let contents = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
            collected.push((relative, contents));
        }
    }
    assert!(
        collected.len() > 50,
        "source scan found only {} files, so an empty scan could pass these contracts vacuously",
        collected.len()
    );
    collected
}

/// Self-assignment must be reachable from exactly one production call site.
///
/// A second call site is not automatically wrong, but it is exactly the change
/// that could put the dashboard inside a Job it owns, so it must be reviewed
/// against this contract rather than merged silently.
#[test]
fn only_the_worker_containment_helper_assigns_the_current_process_to_its_own_job() {
    let mut callers: Vec<String> = source_files()
        .into_iter()
        .filter(|(path, contents)| {
            path != "runtime/job_object.rs"
                && !path.ends_with("_tests.rs")
                && production_text(contents).contains(SELF_ASSIGN_CALL)
        })
        .map(|(path, _)| path)
        .collect();
    callers.sort();

    assert_eq!(
        callers,
        vec!["runtime/agent_launcher.rs".to_string()],
        "`{SELF_ASSIGN_CALL}` puts the calling process inside a KILL_ON_JOB_CLOSE Job it \
         also owns. Only the session-host launcher may do that, because the host is a \
         short-lived per-agent process whose death is *supposed* to reap its worker. \
         Reaching it from the dashboard would make a dropped handle kill jefe and every \
         agent with it -- silently, with no unwind and no crash record (issue #664)."
    );
}

/// The self-assigning helper must be reachable only from the launch-plan
/// runner, which is the session host's entrypoint and nothing else.
#[test]
fn worker_containment_is_established_only_while_running_a_launch_plan() {
    let launcher = read_repo_text("src/runtime/agent_launcher.rs");
    let call_sites = launcher.matches("establish_worker_containment()").count();

    assert_eq!(
        call_sites, 2,
        "expected exactly one definition and one call of `establish_worker_containment`, \
         found {call_sites} occurrences. Any additional call site must be checked against \
         the #664 contract: it decides which processes join a kill-on-close Job."
    );
    assert!(
        launcher.contains("let _containment = establish_worker_containment()?;"),
        "the containment guard must be bound to a named binding that lives until the \
         worker is waited on. `let _ = establish_worker_containment()?;` drops the guard \
         immediately, closing the Job handle and reaping the worker tree at once -- the \
         same silent mass termination #664 reports."
    );
}

/// The launch-plan runner must be gated behind the internal-launch argument.
///
/// This is the step that keeps the dashboard out of the Job: `main` reaches
/// `run_launch_plan` only after matching the internal-launch argument, and that
/// branch never returns to the dashboard, so a process that runs the TUI has
/// provably not established worker containment.
#[test]
fn the_launch_plan_runner_is_reachable_only_through_the_internal_launch_argument() {
    let main = read_repo_text("src/main.rs");
    let Some((_, entry)) = main.split_once("fn run_internal_agent_launch_if_requested()") else {
        panic!("src/main.rs must define `run_internal_agent_launch_if_requested`");
    };
    let body = entry.split_once("\nfn ").map_or(entry, |(body, _)| body);

    assert!(
        body.contains("INTERNAL_LAUNCH_ARGUMENT")
            && body.contains("return;")
            && body.contains("run_launch_plan"),
        "`run_internal_agent_launch_if_requested` must return early unless argv names the \
         internal-launch argument, before it can reach `run_launch_plan`. Without that \
         gate a normal `jefe` invocation would establish worker containment on the \
         dashboard process itself (issue #664)."
    );

    // Guard the guard: if `production_text` ever stripped too much, the scan
    // below would report "no callers" and pass vacuously. The launcher's
    // definition must survive while its test-module calls must not.
    let launcher_production = production_text(&read_repo_text("src/runtime/agent_launcher.rs"));
    assert!(
        launcher_production.contains("pub fn run_launch_plan("),
        "production_text dropped the definition it is meant to keep, so this scan would \
         pass without inspecting anything"
    );
    assert!(
        !launcher_production.contains("super::run_launch_plan("),
        "production_text kept a test-module call, so this scan cannot distinguish test \
         call sites from production ones"
    );

    let mut callers = source_files()
        .into_iter()
        .filter(|(_, contents)| {
            production_text(contents)
                .replace("fn run_launch_plan(", "")
                .contains("run_launch_plan(")
        })
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    callers.sort();
    assert_eq!(
        callers,
        vec!["main.rs".to_string()],
        "`run_launch_plan` establishes kill-on-close containment for the calling process. \
         It must be invoked from the internal-launch entrypoint only."
    );

    for tail in ["std::process::exit(code);", "std::process::exit(1);"] {
        assert!(
            body.contains(tail),
            "every exit path of the internal launch must terminate the process ({tail} \
             missing). Falling through into the dashboard would run the TUI inside the \
             session host's kill-on-close Job, so an unrelated host exit would take the \
             whole tree down (issue #664)."
        );
    }
}
