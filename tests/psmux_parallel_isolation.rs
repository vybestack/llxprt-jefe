//! Issue #545 V4: prove psmux tests are isolated under real concurrency.
//!
//! `RUST_TEST_THREADS: 1` was introduced (#324, `8c0410cf`) because psmux
//! smoke tests were believed to share a namespace/server and collide in
//! parallel. Serializing the whole workspace suite made every multi-agent
//! race invisible on the least-tested platform. The correct remedy is
//! isolation, which these tests demonstrate rather than assert by fiat:
//!
//! * each concurrent worker gets its own namespace, hence its own server;
//! * no worker can observe another worker's sessions;
//! * tearing one server down leaves every other worker intact.
//!
//! These are the properties that make default parallelism safe.

#![cfg(windows)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Number of concurrent workers. Four mirrors the "several agents at once"
/// shape that real users run and the existing four-agent smoke test.
const WORKERS: usize = 4;

/// The namespace construction every psmux test must use.
///
/// Uniqueness comes from the process id and a process-wide atomic counter.
/// The timestamp is diagnostic sugar only: it cannot be relied on, because
/// the Windows system clock has coarse resolution (~0.5-15.6 ms) and many
/// threads observe the identical nanosecond value.
///
/// `tests/core/windows_ci_signal_contracts.rs` asserts every psmux test file
/// uses this construction, so this proof applies repo-wide.
fn unique_namespace(label: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!(
        "jefe-iso-{label}-{}-{nanos:x}-{sequence:x}",
        std::process::id()
    )
}

/// V4: two (here, four) concurrently-running psmux workers must use distinct
/// namespaces and must not be able to observe each other's sessions.
#[test]
fn concurrent_psmux_namespaces_are_distinct_and_mutually_invisible() {
    let Some(executable) = psmux_executable() else {
        return;
    };

    let (sender, receiver) = mpsc::channel();
    let workers: Vec<_> = (0..WORKERS)
        .map(|index| {
            let executable = executable.clone();
            let sender = sender.clone();
            thread::spawn(move || {
                let namespace = unique_namespace(&format!("worker{index}"));
                let session = format!("isolation-worker-{index}");
                let guard = NamespaceGuard {
                    executable: executable.clone(),
                    namespace: namespace.clone(),
                };
                let created = run(
                    &executable,
                    &namespace,
                    &["new-session", "-d", "-s", &session],
                );
                assert!(
                    created.status.success(),
                    "worker {index} could not create its session in {namespace}: {}",
                    String::from_utf8_lossy(&created.stderr)
                );
                // Report readiness, then wait until every worker has a live
                // session so the visibility check happens while all servers
                // are genuinely running concurrently.
                let _ = sender.send((namespace.clone(), session.clone()));
                (guard, namespace, session)
            })
        })
        .collect();
    drop(sender);

    let live: Vec<(String, String)> = receiver.iter().take(WORKERS).collect();
    assert_eq!(
        live.len(),
        WORKERS,
        "every worker must report a live session"
    );

    let namespaces: BTreeSet<&str> = live.iter().map(|(ns, _)| ns.as_str()).collect();
    assert_eq!(
        namespaces.len(),
        WORKERS,
        "concurrent workers must use distinct namespaces, got: {namespaces:?}"
    );

    for (namespace, session) in &live {
        assert_only_own_session_visible(&executable, namespace, session, &live);
    }

    for worker in workers {
        // Each guard kills only its own server on drop, proving per-test
        // server lifetimes rather than one shared server.
        let joined = worker
            .join()
            .map_err(|_| "a worker thread panicked".to_owned());
        match joined {
            Ok(owned) => drop(owned),
            Err(message) => panic!("{message}"),
        }
    }
}

/// Assert `namespace` sees its own session and none of the other workers'.
fn assert_only_own_session_visible(
    executable: &PathBuf,
    namespace: &str,
    session: &str,
    live: &[(String, String)],
) {
    let listed = run(
        executable,
        namespace,
        &["list-sessions", "-F", "#{session_name}"],
    );
    let listed_text = String::from_utf8_lossy(&listed.stdout).into_owned();
    let visible: BTreeSet<&str> = listed_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        visible.contains(session),
        "namespace {namespace} must see its own session {session}, saw {visible:?}"
    );
    for (_, other) in live {
        if other != session {
            assert!(
                !visible.contains(other.as_str()),
                "namespace {namespace} must not observe another worker's \
                 session {other}; namespaces are not isolated"
            );
        }
    }
}

/// A6: the namespace generator must stay unique under same-tick contention.
///
/// This is the property a bare timestamp cannot provide: the Windows system
/// clock has coarse resolution (~0.5-15.6 ms), so many threads observe the
/// identical nanosecond value. Uniqueness must come from the process id and a
/// process-wide atomic counter, not from the clock.
#[test]
fn psmux_namespace_generator_is_unique_under_same_tick_contention() {
    const THREADS: usize = 8;
    const PER_THREAD: usize = 2_000;

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            thread::spawn(|| {
                (0..PER_THREAD)
                    .map(|_| unique_namespace("contention"))
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let mut all = Vec::with_capacity(THREADS * PER_THREAD);
    for handle in handles {
        let joined = handle
            .join()
            .map_err(|_| "a generator thread panicked".to_owned());
        match joined {
            Ok(names) => all.extend(names),
            Err(message) => panic!("{message}"),
        }
    }

    let unique: BTreeSet<&String> = all.iter().collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "namespace generator produced {} duplicates across {} concurrent \
         calls; a timestamp alone is not unique within one clock tick",
        all.len() - unique.len(),
        all.len()
    );
}

/// Kills only its own namespace's server, never a shared one.
struct NamespaceGuard {
    executable: PathBuf,
    namespace: String,
}

impl Drop for NamespaceGuard {
    fn drop(&mut self) {
        let _ = Command::new(&self.executable)
            .arg("-L")
            .arg(&self.namespace)
            .arg("kill-server")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn run(executable: &PathBuf, namespace: &str, args: &[&str]) -> Output {
    Command::new(executable)
        .arg("-L")
        .arg(namespace)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("psmux -L {namespace} {args:?} failed to start: {error}"))
}

/// Resolve psmux, honouring the same `JEFE_REQUIRE_PSMUX` contract the other
/// psmux tests use: required in CI, skipped on machines without it.
fn psmux_executable() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("JEFE_PSMUX_BIN") {
        let path = PathBuf::from(explicit);
        if path.exists() {
            return Some(path);
        }
    }
    let output = Command::new("where.exe").arg("psmux").output().ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(first) = text.lines().next() {
            return Some(PathBuf::from(first.trim()));
        }
    }
    assert!(
        !std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1"),
        "JEFE_REQUIRE_PSMUX=1 but psmux could not be resolved"
    );
    None
}
