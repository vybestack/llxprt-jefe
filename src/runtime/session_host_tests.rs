//! Behavioral contracts for per-session, content-addressed Windows host staging.
//!
//! Issue #467 Slice 1 (AC1, AC4, AC8, AC9): native Windows psmux panes must run
//! from an immutable copy of the Jefe image staged below the resolved
//! session-host root, so the live build/install target is never locked by a
//! running pane. These tests cover pure path planning, copy staging,
//! idempotency, interrupted-temp cleanup, and typed diagnostics. They are
//! platform-agnostic: staging is exercised on every target so the logic is
//! proven deterministically in CI regardless of host platform.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::session_host::{
    SessionCleanupOutcome, SessionHostError, SessionHostPlan, stage_session_host,
    stage_session_host_with_attempt, startup_cleanup_session_hosts,
};

const SESSION_HOST_BINARY: &str = "jefe-session-host.exe";

fn write_source(directory: &TempDir, bytes: &[u8]) -> PathBuf {
    let path = directory.path().join("jefe.exe");
    fs::write(&path, bytes).unwrap_or_else(|error| panic!("write source fixture: {error}"));
    path
}

fn write_default_source(directory: &TempDir) -> PathBuf {
    write_source(directory, b"jefe-image-v1")
}

fn hex_of(bytes: &[u8]) -> String {
    crate::domain::sha256::Sha256::digest(bytes).to_string()
}

#[test]
fn plan_produces_deterministic_sanitized_content_addressed_path() {
    let root = PathBuf::from("/state/session-hosts");
    let plan = SessionHostPlan::for_session(&root, "jefe-alpha-1", b"jefe-image-v1")
        .unwrap_or_else(|error| panic!("plan should resolve: {error}"));

    let expected_session = "jefe-alpha-1";
    let expected_digest = hex_of(b"jefe-image-v1");
    let expected = root
        .join(expected_session)
        .join(&expected_digest)
        .join(SESSION_HOST_BINARY);
    assert_eq!(plan.staged_path(), &expected);
    assert_eq!(
        plan.digest_directory(),
        &root.join(expected_session).join(&expected_digest)
    );
}

#[test]
fn plan_sanitizes_session_name_to_safe_path_segment() {
    let root = PathBuf::from("/state/session-hosts");
    // A real `RuntimeBinding.session_name` is always `jefe-<agent>`, but staging
    // must never trust that invariant for path safety.
    let plan = SessionHostPlan::for_session(&root, "../../etc/passwd", b"bytes")
        .unwrap_or_else(|error| panic!("plan should resolve: {error}"));
    let relative = plan
        .staged_path()
        .strip_prefix(&root)
        .unwrap_or_else(|error| panic!("staged path must stay under root: {error}"));
    let session_segment = relative
        .components()
        .next()
        .unwrap_or_else(|| panic!("staged path must have a session segment: {relative:?}"));
    let segment = session_segment.as_os_str().to_string_lossy();
    assert!(
        !segment.contains('/'),
        "session segment must not contain '/': {segment}"
    );
    assert!(
        !segment.contains(".."),
        "session segment must not contain traversal: {segment}"
    );
    assert!(
        segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "session segment must be sanitized: {segment}"
    );
}

#[test]
fn plan_rejects_empty_or_unsanitizable_session_names() {
    let root = PathBuf::from("/state/session-hosts");
    for invalid in ["", "   ", "---"] {
        assert!(
            matches!(
                SessionHostPlan::for_session(&root, invalid, b"bytes"),
                Err(SessionHostError::InvalidSessionName { .. })
            ),
            "session name should be rejected: {invalid:?}"
        );
    }
}

#[test]
fn plan_is_pure_and_does_not_touch_the_filesystem() {
    let root = PathBuf::from("/this/path/does/not/exist");
    let plan = SessionHostPlan::for_session(&root, "jefe-agent-9", b"payload")
        .unwrap_or_else(|error| panic!("plan must be pure: {error}"));
    // Reading the staged path must not require any directory to exist.
    assert!(!plan.staged_path().exists());
}

#[test]
fn staging_copies_source_bytes_under_root_without_hardlink() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let staged = stage_session_host(root.path(), "jefe-agent-1", &source)
        .unwrap_or_else(|error| panic!("staging should succeed: {error}"));

    assert!(staged.exists(), "staged binary should exist");
    let staged_bytes =
        fs::read(&staged).unwrap_or_else(|error| panic!("read staged binary: {error}"));
    assert_eq!(staged_bytes, b"jefe-image-v1");

    // Copy (not hardlink): the staged inode must be independent of the source so
    // replacing the source never touches the running image (AC4).
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let source_meta =
            fs::metadata(&source).unwrap_or_else(|error| panic!("source metadata: {error}"));
        let staged_meta =
            fs::metadata(&staged).unwrap_or_else(|error| panic!("staged metadata: {error}"));
        assert_ne!(
            source_meta.ino(),
            staged_meta.ino(),
            "staging must not hardlink"
        );
        assert_eq!(staged_meta.nlink(), 1, "staged file must stand alone");
    }
    // Cross-platform content-addressed check: identical bytes verify the copy.
    let source_bytes =
        fs::read(&source).unwrap_or_else(|error| panic!("read source binary: {error}"));
    assert_eq!(source_bytes, staged_bytes);

    // The staged path matches the pure planner's prediction for this content.
    let expected = SessionHostPlan::for_session(root.path(), "jefe-agent-1", b"jefe-image-v1")
        .unwrap_or_else(|error| panic!("plan: {error}"));
    assert_eq!(staged, *expected.staged_path());
}

#[test]
fn staging_is_idempotent_and_reuses_existing_digest_artifact() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let first = stage_session_host(root.path(), "jefe-agent-1", &source)
        .unwrap_or_else(|error| panic!("first staging: {error}"));
    let first_modified = fs::metadata(&first)
        .and_then(|metadata| metadata.modified())
        .unwrap_or_else(|error| panic!("first mtime: {error}"));

    // Ensure the second observation's timestamp can differ if rewritten.
    std::thread::sleep(std::time::Duration::from_millis(20));

    let second = stage_session_host(root.path(), "jefe-agent-1", &source)
        .unwrap_or_else(|error| panic!("second staging: {error}"));

    assert_eq!(first, second, "idempotent staging must reuse same path");
    let second_modified = fs::metadata(&second)
        .and_then(|metadata| metadata.modified())
        .unwrap_or_else(|error| panic!("second mtime: {error}"));
    assert_eq!(
        first_modified, second_modified,
        "idempotent staging must leave existing artifact untouched"
    );
}

#[test]
fn staging_different_content_produces_distinct_artifacts() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let first_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src 1: {error}"));
    let second_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src 2: {error}"));

    let first_source = write_source(&first_dir, b"jefe-image-v1");
    let second_source = write_source(&second_dir, b"jefe-image-v2");

    let first = stage_session_host(root.path(), "jefe-agent-1", &first_source)
        .unwrap_or_else(|error| panic!("first staging: {error}"));
    let second = stage_session_host(root.path(), "jefe-agent-1", &second_source)
        .unwrap_or_else(|error| panic!("second staging: {error}"));

    assert_ne!(first, second, "distinct digests stage separately");
    assert!(
        second.exists(),
        "the current artifact is staged and retained"
    );
}

/// Issue #542, deliverable 6: a rebuild changes the digest and stages a new
/// directory. Nothing swept those between startups, so every rebuild during a
/// long-lived session left another host image behind. A superseded generation
/// that no process holds is unowned, and unowned artifacts are collected.
#[test]
fn staging_prunes_superseded_generations_that_no_process_holds() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));

    let mut superseded = Vec::new();
    for build in 1..=3_u8 {
        let source = write_source(&source_dir, &[b'j', b'e', b'f', b'e', build]);
        let staged = stage_session_host(root.path(), "jefe-agent-1", &source)
            .unwrap_or_else(|error| panic!("staging build {build}: {error}"));
        superseded.push(staged);
    }
    let current = superseded.pop().unwrap_or_else(|| panic!("staged nothing"));

    assert!(current.exists(), "the current generation is retained");
    for stale in &superseded {
        assert!(
            !stale.exists(),
            "a superseded host generation that nothing holds must not \
             accumulate across rebuilds: {} survived",
            stale.display()
        );
    }
    let session_directory = current
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("staged path has a session directory"));
    let generations = fs::read_dir(session_directory)
        .unwrap_or_else(|error| panic!("read session dir: {error}"))
        .count();
    assert_eq!(
        generations, 1,
        "three rebuilds must leave exactly one live generation"
    );
}

/// The counterpart guarantee, and the one #467 exists to protect: a rebuild may
/// never orphan or disarm a live tree.
///
/// On Windows a running host's image is mapped as an image section and cannot
/// be unlinked, which is what makes "no process holds it" decidable without
/// liveness bookkeeping. Here that unlinkable image is modelled by a path
/// `remove_file` must refuse, the same substitution
/// `cleanup_session_directory_reports_retained_when_artifact_cannot_be_removed_as_directory`
/// uses, so the retention rule is proven on every platform rather than only
/// where a real image lock is reproducible.
#[test]
fn staging_retains_a_superseded_generation_whose_image_cannot_be_unlinked() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));

    let held_source = write_source(&source_dir, b"jefe-image-held");
    let held = stage_session_host(root.path(), "jefe-agent-1", &held_source)
        .unwrap_or_else(|error| panic!("staging held host: {error}"));
    let held_generation = held
        .parent()
        .unwrap_or_else(|| panic!("staged path has a generation directory"))
        .to_path_buf();
    fs::remove_file(&held).unwrap_or_else(|error| panic!("replace image: {error}"));
    fs::create_dir(&held).unwrap_or_else(|error| panic!("unlinkable image: {error}"));
    fs::write(held.join("mapped"), b"in use")
        .unwrap_or_else(|error| panic!("image contents: {error}"));
    let companion = held_generation.join("generation.marker");
    fs::write(&companion, b"marker").unwrap_or_else(|error| panic!("companion: {error}"));

    let rebuilt_source = write_source(&source_dir, b"jefe-image-rebuilt");
    let rebuilt = stage_session_host(root.path(), "jefe-agent-1", &rebuilt_source)
        .unwrap_or_else(|error| panic!("staging rebuilt host: {error}"));

    assert!(rebuilt.exists(), "the rebuilt generation is staged");
    assert!(
        held.exists(),
        "a generation whose image cannot be unlinked must survive the \
         rebuild; removing it would orphan or disarm the live tree"
    );
    assert!(
        companion.exists(),
        "an abandoned generation must be left intact, not half-deleted \
         around the image the operating system refused to release"
    );
}

#[test]
fn staging_removes_interrupted_temp_artifacts_owned_by_this_attempt() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let plan = SessionHostPlan::for_session(root.path(), "jefe-agent-1", b"jefe-image-v1")
        .unwrap_or_else(|error| panic!("plan: {error}"));
    // Simulate an interrupted prior staging: a leftover temp file in the digest
    // directory owned by this staging attempt.
    fs::create_dir_all(plan.digest_directory())
        .unwrap_or_else(|error| panic!("create digest dir: {error}"));
    let leftover = plan
        .digest_directory()
        .join(format!("{SESSION_HOST_BINARY}.jefe-staging-tmp-legacy"));
    fs::write(&leftover, b"partial").unwrap_or_else(|error| panic!("write leftover temp: {error}"));

    let staged = stage_session_host_with_attempt(root.path(), "jefe-agent-1", &source, "legacy")
        .unwrap_or_else(|error| panic!("staging after leftover: {error}"));

    assert!(staged.exists(), "staged binary exists");
    assert!(
        !leftover.exists(),
        "interrupted temp owned by this attempt must be cleaned"
    );
}

#[test]
fn staging_leaves_unrelated_temp_artifacts_for_other_attempts_untouched() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let plan = SessionHostPlan::for_session(root.path(), "jefe-agent-1", b"jefe-image-v1")
        .unwrap_or_else(|error| panic!("plan: {error}"));
    fs::create_dir_all(plan.digest_directory())
        .unwrap_or_else(|error| panic!("create digest dir: {error}"));
    // A concurrent staging attempt's temp file must not be deleted by this one.
    let other = plan
        .digest_directory()
        .join(format!("{SESSION_HOST_BINARY}.jefe-staging-tmp-concurrent"));
    fs::write(&other, b"other-attempt")
        .unwrap_or_else(|error| panic!("write concurrent temp: {error}"));

    let staged = stage_session_host_with_attempt(root.path(), "jefe-agent-1", &source, "this-try")
        .unwrap_or_else(|error| panic!("staging with concurrent temp: {error}"));

    assert!(staged.exists());
    assert!(
        other.exists(),
        "temp owned by a different staging attempt must be retained"
    );
}

#[test]
fn concurrent_staging_of_the_same_image_is_idempotent() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let paths = std::thread::scope(|scope| {
        let first = scope.spawn(|| stage_session_host(root.path(), "jefe-agent-1", &source));
        let second = scope.spawn(|| stage_session_host(root.path(), "jefe-agent-1", &source));
        [
            first
                .join()
                .unwrap_or_else(|_| panic!("first staging thread panicked")),
            second
                .join()
                .unwrap_or_else(|_| panic!("second staging thread panicked")),
        ]
    });
    let first = paths[0]
        .as_ref()
        .unwrap_or_else(|error| panic!("first concurrent staging: {error}"));
    let second = paths[1]
        .as_ref()
        .unwrap_or_else(|error| panic!("second concurrent staging: {error}"));
    assert_eq!(first, second);
    assert_eq!(
        fs::read(first).unwrap_or_else(|error| panic!("read staged image: {error}")),
        b"jefe-image-v1"
    );
}

#[test]
fn staging_rejects_attempt_tags_that_can_escape_the_digest_directory() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    for attempt_tag in ["", "../escape", "nested/path", r"nested\path"] {
        let result =
            stage_session_host_with_attempt(root.path(), "jefe-agent-1", &source, attempt_tag);
        assert!(
            matches!(result, Err(SessionHostError::InvalidAttemptTag)),
            "unsafe attempt tag must be rejected: {attempt_tag:?}: {result:?}"
        );
    }
}

#[test]
fn staging_preserves_staged_copy_after_source_is_replaced() {
    // AC4: a staged/running copy must permit replacing the source image.
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let staged = stage_session_host(root.path(), "jefe-agent-1", &source)
        .unwrap_or_else(|error| panic!("staging: {error}"));

    // Replace the source image while the staged copy is "running".
    fs::write(&source, b"jefe-image-v2-rebuilt")
        .unwrap_or_else(|error| panic!("replace source image: {error}"));

    let staged_bytes =
        fs::read(&staged).unwrap_or_else(|error| panic!("read staged after replace: {error}"));
    assert_eq!(
        staged_bytes, b"jefe-image-v1",
        "staged copy must be immutable across source replacement"
    );
}

#[test]
fn staging_errors_are_typed_and_name_operation_and_safe_path() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let missing = PathBuf::from("/definitely/missing/source/jefe.exe");

    let error = match stage_session_host(root.path(), "jefe-agent-1", &missing) {
        Ok(path) => panic!("missing source should not stage: {}", path.display()),
        Err(error) => error,
    };
    let diagnostic = error.to_string();
    assert!(
        matches!(error, SessionHostError::SourceRead { .. }),
        "expected SourceRead, got {error:?}"
    );
    assert!(
        diagnostic.to_lowercase().contains("source"),
        "diagnostic must name the operation context: {diagnostic}"
    );
    assert!(
        diagnostic.contains("jefe.exe"),
        "diagnostic must surface the safe path: {diagnostic}"
    );
}

#[test]
fn staging_error_never_leaks_source_bytes() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = source_dir.path().join("jefe.exe");
    // Deliberately do not write the source so the read fails; the planner is
    // given no bytes, and the diagnostic must never echo file contents.
    let error = match stage_session_host(root.path(), "jefe-agent-1", &source) {
        Ok(path) => panic!("missing source should not stage: {}", path.display()),
        Err(error) => error,
    };
    let diagnostic = error.to_string();
    assert!(
        !diagnostic.contains("SECRET-MARKER"),
        "diagnostic must not leak source bytes: {diagnostic}"
    );
}

// ── Issue #467 Slice 2: per-session cleanup (AC7) ──────────────────────────
//
// A successful explicit local kill removes only the killed session's host
// directory; unrelated sessions and ambiguous artifacts are never touched.
// Cleanup failures are best-effort and retained for retry rather than aborting
// the kill.

fn stage_session_fixture(root: &TempDir, source: &Path, session_name: &str) -> PathBuf {
    stage_session_host(root.path(), session_name, source)
        .unwrap_or_else(|error| panic!("fixture staging for {session_name}: {error}"))
}

#[test]
fn cleanup_session_directory_removes_only_the_target_session_host_directory() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let target = stage_session_fixture(&root, &source, "jefe-alpha");
    let sibling = stage_session_fixture(&root, &source, "jefe-beta");

    assert!(target.exists(), "target fixture must exist");
    assert!(sibling.exists(), "sibling fixture must exist");

    let outcome =
        crate::runtime::session_host::cleanup_session_directory(root.path(), "jefe-alpha")
            .unwrap_or_else(|error| panic!("cleanup_session_directory should succeed: {error}"));

    assert!(
        matches!(outcome, SessionCleanupOutcome::Removed),
        "successful cleanup of an existing session directory should report Removed"
    );
    assert!(
        !target.exists(),
        "killed session's host directory must be removed"
    );
    assert!(
        sibling.exists(),
        "unrelated session's host directory must be retained"
    );
}

#[test]
fn cleanup_session_directory_reports_absent_when_no_directory_exists() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let outcome =
        crate::runtime::session_host::cleanup_session_directory(root.path(), "jefe-never-staged")
            .unwrap_or_else(|error| {
                panic!("cleanup of absent directory should not error: {error}")
            });
    assert!(
        matches!(outcome, SessionCleanupOutcome::Absent),
        "cleanup of an absent session directory should report Absent, got {outcome:?}"
    );
}

#[test]
fn cleanup_session_directory_rejects_unsanitizable_session_name_without_touching_root() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);
    let retained = stage_session_fixture(&root, &source, "jefe-keeper");

    let result = crate::runtime::session_host::cleanup_session_directory(root.path(), "---");
    assert!(
        matches!(result, Err(SessionHostError::InvalidSessionName { .. })),
        "unsanitizable session name must be rejected: {result:?}"
    );
    assert!(
        retained.exists(),
        "an invalid session name must not allow any directory to be removed"
    );
}

#[test]
fn cleanup_session_directory_reports_retained_when_artifact_cannot_be_removed_as_directory() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let session_path = root.path().join("jefe-locked");
    fs::write(&session_path, b"interrupted artifact")
        .unwrap_or_else(|error| panic!("write invalid session artifact: {error}"));

    let outcome =
        crate::runtime::session_host::cleanup_session_directory(root.path(), "jefe-locked")
            .unwrap_or_else(|error| panic!("cleanup should retain on failure, not error: {error}"));

    assert!(
        matches!(outcome, SessionCleanupOutcome::RetainedForRetry),
        "cleanup failure must be retained for retry, got {outcome:?}"
    );
    assert!(
        session_path.exists(),
        "retained session artifact must still exist after failed directory cleanup"
    );
}

// ── Issue #467 Slice 2: startup cleanup (AC8) ──────────────────────────────
//
// Startup cleanup scans the session-host root and removes only unreferenced
// and dead session directories plus interrupted staging temp files. Live
// psmux sessions, persisted-reference directories, and ambiguous/unprobeable
// artifacts are retained.

#[derive(Debug, Clone)]
struct ProbeSet {
    alive: std::collections::HashSet<String>,
    unprobeable: std::collections::HashSet<String>,
}

impl ProbeSet {
    fn new() -> Self {
        Self {
            alive: std::collections::HashSet::new(),
            unprobeable: std::collections::HashSet::new(),
        }
    }
    fn alive_session(mut self, name: &str) -> Self {
        self.alive.insert(name.to_owned());
        self
    }
    fn unprobeable_session(mut self, name: &str) -> Self {
        self.unprobeable.insert(name.to_owned());
        self
    }
}

fn probe_for(set: ProbeSet) -> impl Fn(&str) -> crate::runtime::liveness::SessionLiveness {
    move |name: &str| {
        if set.alive.contains(name) {
            crate::runtime::liveness::SessionLiveness::Alive
        } else if set.unprobeable.contains(name) {
            crate::runtime::liveness::SessionLiveness::Unavailable
        } else {
            crate::runtime::liveness::SessionLiveness::Missing
        }
    }
}

#[test]
fn startup_cleanup_removes_unreferenced_and_dead_session_directories() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let unreferenced = stage_session_fixture(&root, &source, "jefe-unreferenced");
    let dead = stage_session_fixture(&root, &source, "jefe-dead");

    let report = startup_cleanup_session_hosts(
        root.path(),
        &[], // no persisted references
        probe_for(ProbeSet::new()),
    )
    .unwrap_or_else(|error| panic!("startup cleanup should succeed: {error}"));

    assert!(
        !unreferenced.exists(),
        "unreferenced session directory must be removed"
    );
    assert!(
        !dead.exists(),
        "session directory with a Missing pane probe must be removed"
    );
    assert!(
        report
            .removed_session_directories
            .iter()
            .any(|dir| { dir.ends_with("jefe-unreferenced") || dir.ends_with("jefe-dead") }),
        "report must enumerate the removed session directories: {report:?}"
    );
}

#[test]
fn startup_cleanup_retains_directories_for_live_psmux_sessions() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let live = stage_session_fixture(&root, &source, "jefe-live");

    let report = startup_cleanup_session_hosts(
        root.path(),
        &[], // no persisted reference; liveness is the only reason to retain
        probe_for(ProbeSet::new().alive_session("jefe-live")),
    )
    .unwrap_or_else(|error| panic!("startup cleanup should succeed: {error}"));

    assert!(
        live.exists(),
        "live psmux session directory must be retained even without a persisted reference"
    );
    assert!(
        report.removed_session_directories.is_empty(),
        "no session directories should be removed when only a live session exists: {report:?}"
    );
    assert!(
        report
            .retained_live_session_directories
            .iter()
            .any(|dir| dir.ends_with("jefe-live")),
        "report must enumerate the retained live session directory: {report:?}"
    );
}

#[test]
fn startup_cleanup_retains_directories_referenced_by_persisted_bindings() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let referenced = stage_session_fixture(&root, &source, "jefe-referenced");
    let persisted_references = vec!["jefe-referenced".to_owned()];

    let report = startup_cleanup_session_hosts(
        root.path(),
        &persisted_references,
        probe_for(ProbeSet::new()), // probe says Missing, but the reference retains it
    )
    .unwrap_or_else(|error| panic!("startup cleanup should succeed: {error}"));

    assert!(
        referenced.exists(),
        "persisted-reference directory must be retained even when the pane probe is Missing"
    );
    assert!(
        report.removed_session_directories.is_empty(),
        "no session directories should be removed when a persisted reference exists: {report:?}"
    );
}

#[test]
fn startup_cleanup_retains_unprobeable_and_ambiguous_directories() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let unprobeable = stage_session_fixture(&root, &source, "jefe-unprobeable");

    // An artifact whose sanitized session name cannot be inverted back to a
    // RuntimeBinding session name (ambiguity) must also be retained. Seed a
    // directory whose name does not match the jefe-<agent> contract.
    let ambiguous_name = "stray-artifact-dir";
    let ambiguous = root.path().join(ambiguous_name);
    std::fs::create_dir_all(&ambiguous)
        .unwrap_or_else(|error| panic!("seed ambiguous artifact: {error}"));

    let report = startup_cleanup_session_hosts(
        root.path(),
        &[],
        probe_for(
            ProbeSet::new()
                .unprobeable_session("jefe-unprobeable")
                .unprobeable_session(ambiguous_name),
        ),
    )
    .unwrap_or_else(|error| panic!("startup cleanup should succeed: {error}"));

    assert!(
        unprobeable.exists(),
        "unprobeable session directory must be retained"
    );
    assert!(
        ambiguous.exists(),
        "ambiguous session directory must be retained"
    );
    assert!(
        report.removed_session_directories.is_empty(),
        "no session directories should be removed when every entry is ambiguous/unprobeable: {report:?}"
    );
}

#[test]
fn startup_cleanup_reclaims_interrupted_staging_temp_files() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("temp root: {error}"));
    let source_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("temp src: {error}"));
    let source = write_default_source(&source_dir);

    let staged = stage_session_fixture(&root, &source, "jefe-with-leftover");
    // Seed an interrupted staging temp file inside the live session's digest
    // directory to prove startup reclaims leftover temps without removing the
    // live session directory itself.
    let plan = SessionHostPlan::for_session(root.path(), "jefe-with-leftover", b"jefe-image-v1")
        .unwrap_or_else(|error| panic!("plan: {error}"));
    let leftover = plan.digest_directory().join(format!(
        "{SESSION_HOST_BINARY}.jefe-staging-tmp-interrupted"
    ));
    std::fs::write(&leftover, b"partial bytes")
        .unwrap_or_else(|error| panic!("seed leftover temp: {error}"));

    let report = startup_cleanup_session_hosts(
        root.path(),
        &["jefe-with-leftover".to_owned()],
        probe_for(ProbeSet::new().alive_session("jefe-with-leftover")),
    )
    .unwrap_or_else(|error| panic!("startup cleanup should succeed: {error}"));

    assert!(
        staged.exists(),
        "live referenced session directory must be retained"
    );
    assert!(
        !leftover.exists(),
        "interrupted staging temp file must be reclaimed"
    );
    assert!(
        report
            .removed_temp_files
            .iter()
            .any(|path| path == &leftover),
        "report must enumerate the reclaimed temp file: {report:?}"
    );
}
