//! White-box tests for the git subprocess timeout helper.

use super::*;

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

// ── run_child_with_timeout: cross-platform subprocess timeout (issue #230) ──

#[cfg(unix)]
#[test]
fn timeout_kills_long_running_child() {
    // `sleep 30` will exceed the 3-second timeout. The helper must kill it,
    // reap it, and return None.
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("30");
    let child = cmd.spawn().value_or_panic("spawn sleep");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "sleep");
    assert!(result.is_none(), "timed-out child must return None");
}

#[cfg(unix)]
#[test]
fn timeout_returns_output_for_fast_child() {
    // `true` exits immediately — the helper must return Some(output) with
    // a successful exit status.
    let mut cmd = std::process::Command::new("true");
    let child = cmd.spawn().value_or_panic("spawn true");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "true");
    let output = result.value_or_panic("fast child must produce output");
    assert!(output.status.success(), "true must exit 0");
}

#[cfg(unix)]
#[test]
fn timeout_captures_stdout() {
    // `echo hello` writes to stdout — the helper must capture it.
    let mut cmd = std::process::Command::new("echo");
    cmd.arg("hello").stdout(std::process::Stdio::piped());
    let child = cmd.spawn().value_or_panic("spawn echo");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "echo");
    let output = result.value_or_panic("echo must produce output");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
}

#[cfg(unix)]
#[test]
fn timeout_completes_child_exceeding_pipe_capacity() {
    // Regression for the pipe-buffer deadlock: a child that writes more than
    // the OS pipe capacity (commonly 64 KiB) before exiting would block on
    // the full pipe and never terminate under the old read-after-poll design,
    // producing a spurious timeout (None). With concurrent pipe draining, the
    // child must complete and its full output must be captured.
    //
    // `dd` is POSIX and present on Linux/macOS/BSD; it writes a deterministic
    // 256000-byte run of NUL bytes to stdout and exits 0. Invoked directly
    // (no shell), so no shell assumptions.
    let mut cmd = std::process::Command::new("dd");
    cmd.args(["if=/dev/zero", "bs=1000", "count=256"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn().value_or_panic("spawn dd");
    let result = super::run_child_with_timeout(child, Path::new("/test"), "dd");
    let output = result.value_or_panic("large-output child must complete, not time out");
    assert!(
        output.status.success(),
        "dd must exit 0, got {:?}",
        output.status
    );
    assert_eq!(
        output.stdout.len(),
        256_000,
        "full stdout ({} bytes) must be captured",
        output.stdout.len()
    );
}
