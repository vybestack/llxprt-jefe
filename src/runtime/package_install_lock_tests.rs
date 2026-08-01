// Issue #556: cross-process managed-install lock.
//
// Included into `package_install_lock::tests`.
//
// A lock that is only exercised from one process proves nothing, so the
// contended cases run a second real OS process. Re-executing the test binary
// through `std::env::current_exe` is the repository's established way to obtain
// one (`process_tests.rs`, `tests/harness_v1.rs`) and needs no extra `[[bin]]`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tempfile::TempDir;

const DIGEST: &str = "0123456789abcdef0123456789abcdef";

/// Directory the holder child operates in. Absent for every ordinary run.
const HOLDER_DIR_ENV: &str = "JEFE_TEST_INSTALL_LOCK_HOLDER_DIR";
const HOLDER_TEST_PATH: &str = "runtime::package_install_lock::tests::install_lock_holder_process";

const HELD_MARKER: &str = "held";
const RELEASE_MARKER: &str = "release";

/// Longest a test will wait for a child to reach a state or exit.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

fn fast_policy(ceiling: Duration) -> LockPolicy {
    LockPolicy {
        ceiling,
        poll_interval: Duration::from_millis(10),
    }
}

fn lock_dir() -> TempDir {
    tempfile::tempdir().unwrap_or_else(|error| panic!("lock dir: {error}"))
}

fn lock_path(dir: &Path) -> PathBuf {
    dir.join("digest.lock")
}

/// Wait for `path` to appear, failing the test rather than hanging forever.
fn await_path(path: &Path, what: &str) {
    let deadline = Instant::now() + CHILD_DEADLINE;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}: {}", path.display());
}

/// A second OS process that takes the lock and holds it until told to stop.
///
/// Inert unless the parent test supplies the channel directory, so an ordinary
/// `cargo test` run executes it as a no-op.
#[test]
fn install_lock_holder_process() {
    let Some(dir) = std::env::var_os(HOLDER_DIR_ENV) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let held = acquire(&lock_path(&dir), DIGEST, LockPolicy::production())
        .unwrap_or_else(|error| panic!("holder could not acquire the lock: {error}"));
    std::fs::write(dir.join(HELD_MARKER), "held")
        .unwrap_or_else(|error| panic!("publish held marker: {error}"));
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !dir.join(RELEASE_MARKER).exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(held);
}

/// A running holder child, always reaped even if the test panics.
///
/// The holder owns its channel directory so the compiler, rather than a
/// convention, guarantees the directory outlives the child process.
struct HolderProcess {
    child: std::process::Child,
    directory: TempDir,
}

impl HolderProcess {
    /// Spawn the holder and return once it has actually taken the lock.
    fn start() -> Self {
        let directory = lock_dir();
        let binary = std::env::current_exe().unwrap_or_else(|error| panic!("current_exe: {error}"));
        let child = std::process::Command::new(binary)
            .args(["--exact", HOLDER_TEST_PATH, "--nocapture", "--test-threads=1"])
            .env(HOLDER_DIR_ENV, directory.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("spawn lock holder: {error}"));
        let held = directory.path().join(HELD_MARKER);
        let holder = Self { child, directory };
        await_path(&held, "the holder to take the lock");
        holder
    }

    /// Channel directory, which is also the directory holding the lock file.
    fn directory(&self) -> &Path {
        self.directory.path()
    }

    /// The contended lock.
    fn lock_path(&self) -> PathBuf {
        lock_path(self.directory())
    }

    /// Ask the holder to release the lock and exit normally.
    fn release(&mut self) {
        std::fs::write(self.directory().join(RELEASE_MARKER), "release")
            .unwrap_or_else(|error| panic!("publish release marker: {error}"));
        let status = self.wait_bounded("release");
        assert!(status.success(), "lock holder failed: {status:?}");
    }

    /// Kill the holder without giving it any chance to clean up.
    fn kill(&mut self) {
        self.child
            .kill()
            .unwrap_or_else(|error| panic!("kill lock holder: {error}"));
        self.wait_bounded("kill");
    }

    /// Reap the child within [`CHILD_DEADLINE`].
    ///
    /// A plain `wait` would turn a hung holder into a stalled CI run instead of
    /// a failing test, so the wait is bounded and the child is killed if it
    /// overruns.
    fn wait_bounded(&mut self, what: &str) -> std::process::ExitStatus {
        let deadline = Instant::now() + CHILD_DEADLINE;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("wait for lock holder ({what}): {error}"),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        panic!("lock holder did not exit within the deadline after {what}");
    }
}

impl Drop for HolderProcess {
    fn drop(&mut self) {
        // A panicking test must not leave a process holding the lock.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn an_uncontended_lock_is_acquired_and_released() {
    let dir = lock_dir();
    let path = lock_path(dir.path());
    {
        let _guard = acquire(&path, DIGEST, LockPolicy::production())
            .unwrap_or_else(|error| panic!("acquire: {error}"));
        assert!(path.exists(), "an acquired lock must exist on disk");
    }
    let reacquired = acquire(&path, DIGEST, fast_policy(Duration::from_millis(200)));
    assert!(
        reacquired.is_ok(),
        "a released lock must be immediately reacquirable: {reacquired:?}"
    );
}

#[test]
fn a_lock_held_by_a_live_process_is_never_taken_over() {
    // A4b: a live holder is respected for the whole ceiling, however long its
    // install runs. Declaring a live holder stale is the mistake npm's fixed
    // five-second threshold made (issue #425 Problem B).
    let mut holder = HolderProcess::start();

    let outcome = acquire(
        &holder.lock_path(),
        DIGEST,
        fast_policy(Duration::from_millis(200)),
    );

    let error = outcome
        .err()
        .unwrap_or_else(|| panic!("a live holder's lock must never be taken over"));
    assert!(
        matches!(error, PackageRuntimeError::InstallLockUnavailable(_)),
        "waiting out a live holder must fail closed with a lock error, got {error:?}"
    );

    // A1 (lock level): once the holder is finished, the waiter proceeds.
    holder.release();
    let after_release = acquire(
        &holder.lock_path(),
        DIGEST,
        fast_policy(Duration::from_secs(30)),
    );
    assert!(
        after_release.is_ok(),
        "the lock must become available once the holder releases it: {after_release:?}"
    );
}

#[test]
fn a_lock_whose_holder_was_killed_is_available_without_recovery() {
    // A4a: the holder dies without releasing anything. The kernel owns the
    // lock, so there is no stale state to detect and no timeout to wait out.
    let mut holder = HolderProcess::start();
    holder.kill();

    let outcome = acquire(
        &holder.lock_path(),
        DIGEST,
        fast_policy(Duration::from_secs(30)),
    );

    assert!(
        outcome.is_ok(),
        "a killed holder's lock must be available again with no recovery step: {outcome:?}"
    );
}

#[test]
fn a_lock_that_cannot_be_opened_is_typed_bounded_and_redacted() {
    // A5: acquisition failure is typed and carries no absolute path.
    let dir = lock_dir();
    let missing = dir.path().join("absent-directory").join("digest.lock");

    let error = acquire(&missing, DIGEST, fast_policy(Duration::from_millis(50)))
        .err()
        .unwrap_or_else(|| panic!("a lock under a missing directory cannot be opened"));

    assert!(
        matches!(error, PackageRuntimeError::InstallLockUnavailable(_)),
        "lock acquisition failure must be its own variant, got {error:?}"
    );
    assert_diagnostic_is_bounded_and_redacted(&error.to_string(), dir.path());
}

#[test]
fn a_wait_timeout_is_typed_bounded_and_redacted() {
    // A5: the ceiling failure carries the same bounded, redacted shape.
    let mut holder = HolderProcess::start();

    let error = acquire(
        &holder.lock_path(),
        DIGEST,
        fast_policy(Duration::from_millis(100)),
    )
    .err()
    .unwrap_or_else(|| panic!("waiting out a live holder must fail"));

    assert_diagnostic_is_bounded_and_redacted(&error.to_string(), holder.directory());
    holder.release();
}

/// Longest a rendered diagnostic may be: the bounded detail plus the fixed
/// `Display` prefix each variant prepends.
const MAX_DIAGNOSTIC_CHARS: usize = MAX_DETAIL_CHARS + 64;

/// A managed-install diagnostic must stay short and must not leak the cache
/// location, which contains the user's home directory and account name.
fn assert_diagnostic_is_bounded_and_redacted(diagnostic: &str, cache_root: &Path) {
    assert!(
        diagnostic.chars().count() <= MAX_DIAGNOSTIC_CHARS,
        "diagnostic must be bounded: {diagnostic:?}"
    );
    assert!(
        !diagnostic.contains(&cache_root.display().to_string()),
        "diagnostic must not embed the cache location: {diagnostic:?}"
    );
    assert!(
        !diagnostic.contains(std::path::MAIN_SEPARATOR),
        "diagnostic must not embed any path: {diagnostic:?}"
    );
    assert!(
        diagnostic.contains("digest=0123456789ab"),
        "diagnostic must correlate to the selector digest: {diagnostic:?}"
    );
}
