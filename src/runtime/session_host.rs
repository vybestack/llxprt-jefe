//! Per-session, content-addressed Windows host staging (issue #467 Slice 1).
//!
//! Native Windows psmux panes previously ran the live Jefe build/install target
//! as their long-lived launcher, locking it against rebuilds. This module plans
//! and stages an immutable copy of that image below a caller-supplied
//! session-host root so a running pane never owns the build target:
//!
//! ```text
//! <root>/<sanitized-session>/<sha256>/jefe-session-host.exe
//! ```
//!
//! Path planning is pure and deterministic; staging copies (never hardlinks)
//! the source through a same-directory unique temp file and atomically renames
//! it into place, reuses an existing digest artifact idempotently, and removes
//! only interrupted temp files owned by the current staging attempt. Unix and
//! remote launch paths do not use this module; the multiplexer selects the
//! staged copy only for native Windows panes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::sha256::Sha256;

use super::agent_executable::ResolvedAgentExecutable;
use super::errors::RuntimeError;
use super::multiplexer::MultiplexerPlan;

/// Fixed filename of the staged host image inside each digest directory.
pub const SESSION_HOST_BINARY: &str = "jefe-session-host.exe";

/// Directory segment below the resolved state-file parent that owns all
/// per-session staged host images (issue #467).
pub const SESSION_HOST_ROOT_SEGMENT: &str = "session-hosts";

/// Prefix used for in-progress staging temp files so interrupted attempts can be
/// identified and reclaimed without touching concurrent staging attempts.
const STAGING_TEMP_PREFIX: &str = "jefe-staging-tmp-";

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Pure, fully resolved per-session host staging plan.
///
/// Constructed from a session-host root, a session name, and the canonical
/// content bytes of the source image. The plan never touches the filesystem:
/// it only derives a deterministic, sanitized, content-addressed path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHostPlan {
    session_directory: PathBuf,
    staged_path: PathBuf,
}

impl SessionHostPlan {
    /// Resolve a deterministic plan for `session_name` addressed by `content`.
    ///
    /// `content` is the canonical byte image of the source host (typically the
    /// running Jefe executable). The session name is sanitized to a safe path
    /// segment; an empty or all-separator name is rejected because it would
    /// collapse onto the root or a sibling session.
    pub fn for_session(
        root: &Path,
        session_name: &str,
        content: &[u8],
    ) -> Result<Self, SessionHostError> {
        let sanitized = sanitize_session_name(session_name).ok_or_else(|| {
            SessionHostError::InvalidSessionName {
                session_name: redact_session_name(session_name),
            }
        })?;
        let digest = Sha256::digest(content).to_string();
        let session_directory = root.join(&sanitized);
        let digest_directory = session_directory.join(&digest);
        let staged_path = digest_directory.join(SESSION_HOST_BINARY);
        Ok(Self {
            session_directory,
            staged_path,
        })
    }

    /// Absolute staged binary path (`<root>/<session>/<digest>/<binary>`).
    #[must_use]
    pub fn staged_path(&self) -> &Path {
        &self.staged_path
    }

    /// Content-addressed digest directory holding the staged binary.
    ///
    /// Always equal to `staged_path`'s parent because the plan constructs the
    /// staged path as `digest_directory/binary`.
    #[must_use]
    pub fn digest_directory(&self) -> &Path {
        self.staged_path
            .parent()
            .unwrap_or(self.session_directory.as_path())
    }
}

/// Stage an immutable copy of `source` for `session_name` under `root`.
///
/// The source bytes are hashed to derive the content-addressed path, copied
/// through a same-directory unique temp file, and atomically renamed into
/// place. An existing artifact at the resolved digest is reused untouched
/// (idempotency). Interrupted temp files previously written by this staging
/// attempt are removed before staging; temp files belonging to concurrent
/// attempts are retained.
///
/// Use [`stage_session_host_with_attempt`] when a caller needs to bound cleanup
/// to a specific attempt tag (for example, to simulate or recover an
/// interrupted prior run).
pub fn stage_session_host(
    root: &Path,
    session_name: &str,
    source: &Path,
) -> Result<PathBuf, SessionHostError> {
    let attempt = default_attempt_tag();
    stage_session_host_with_attempt(root, session_name, source, &attempt)
}

/// Stage an immutable copy, scoping interrupted-temp cleanup to `attempt_tag`.
///
/// `attempt_tag` is folded into each temp filename this staging attempt
/// creates, so cleanup reclaims only temps carrying the same tag and leaves
/// concurrent staging attempts' temps intact.
pub fn stage_session_host_with_attempt(
    root: &Path,
    session_name: &str,
    source: &Path,
    attempt_tag: &str,
) -> Result<PathBuf, SessionHostError> {
    if attempt_tag.is_empty()
        || attempt_tag.contains(['/', '\\', '\0'])
        || attempt_tag.contains("..")
    {
        return Err(SessionHostError::InvalidAttemptTag);
    }
    let bytes = fs::read(source).map_err(|error| SessionHostError::SourceRead {
        path: safe_source_path(source),
        reason: error_kind_message(&error),
    })?;
    let plan = SessionHostPlan::for_session(root, session_name, &bytes)?;
    stage_with_plan(&plan, &bytes, attempt_tag)
}

/// Pure decision: whether the local launch should stage a session host, and
/// if so the `(root, session_name)` pair to stage under.
///
/// Staging is Windows-only: on Unix this always returns `None` so the
/// structurally unchanged tmux/SSH launch path is selected (AC9). A `None`
/// root or session name also returns `None`, preserving the legacy launch
/// path for existing callers and tests.
pub fn session_host_stage_request<'a>(
    root: Option<&'a Path>,
    session_name: Option<&'a str>,
) -> Option<(&'a Path, &'a str)> {
    if !cfg!(windows) {
        return None;
    }
    Some((root?, session_name?))
}

/// Resolve the multiplexer pane-command argv for a local launch.
///
/// On Windows when the manager supplied an explicit session-host root, stage
/// `std::env::current_exe()` below it and build the pane command with the
/// staged copy as the launcher (AC1). Otherwise fall back to the multiplexer's
/// direct agent launch path so Unix and remote behavior is structurally
/// unchanged (AC9).
pub fn resolve_local_pane_command(
    multiplexer: &MultiplexerPlan,
    executable: &ResolvedAgentExecutable,
    pane_args: &[std::ffi::OsString],
    environment: &[(std::ffi::OsString, std::ffi::OsString)],
    session_host: Option<(&Path, &str)>,
) -> Result<Vec<std::ffi::OsString>, RuntimeError> {
    match session_host.and_then(|(root, name)| session_host_stage_request(Some(root), Some(name))) {
        Some((root, name)) => {
            let source = std::env::current_exe().map_err(|_| {
                RuntimeError::Multiplexer(
                    super::multiplexer::MultiplexerError::CurrentExecutableUnavailable,
                )
            })?;
            let staged = stage_session_host(root, name, &source)
                .map_err(RuntimeError::SessionHostStaging)?;
            multiplexer
                .agent_pane_command_args_with_staged_host(
                    executable,
                    &staged,
                    pane_args,
                    environment,
                )
                .map_err(RuntimeError::Multiplexer)
        }
        None => multiplexer
            .agent_pane_command_args(executable, pane_args, environment)
            .map_err(RuntimeError::Multiplexer),
    }
}

fn stage_with_plan(
    plan: &SessionHostPlan,
    bytes: &[u8],
    attempt_tag: &str,
) -> Result<PathBuf, SessionHostError> {
    let digest_directory = plan.digest_directory();
    fs::create_dir_all(digest_directory).map_err(|error| SessionHostError::StagingCreateDir {
        path: digest_directory.to_path_buf(),
        reason: error_kind_message(&error),
    })?;

    reclaim_owned_temps(digest_directory, attempt_tag);

    if plan.staged_path().is_file() {
        return Ok(plan.staged_path().to_path_buf());
    }

    let temp_path = unique_temp_path(digest_directory, attempt_tag);
    fs::write(&temp_path, bytes).map_err(|error| SessionHostError::StagingWrite {
        path: temp_path.clone(),
        reason: error_kind_message(&error),
    })?;

    if fs::rename(&temp_path, plan.staged_path()).is_err() {
        // Another launcher may have won the same content-addressed rename.
        // Treat that as success; all contenders wrote identical digest bytes.
        if plan.staged_path().is_file() {
            let _ = fs::remove_file(&temp_path);
            return Ok(plan.staged_path().to_path_buf());
        }
        // Best-effort cleanup of our temp file before surfacing the failure;
        // a leftover temp from this attempt would be reclaimed on retry.
        let _ = fs::remove_file(&temp_path);
        return Err(SessionHostError::StagingRename {
            path: plan.staged_path().to_path_buf(),
        });
    }
    Ok(plan.staged_path().to_path_buf())
}

fn reclaim_owned_temps(digest_directory: &Path, attempt_tag: &str) {
    let expected_suffix = format!("{STAGING_TEMP_PREFIX}{attempt_tag}");
    let Ok(entries) = fs::read_dir(digest_directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
            if name.contains(&expected_suffix) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

fn unique_temp_path(digest_directory: &Path, attempt_tag: &str) -> PathBuf {
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let pid = std::process::id();
    digest_directory.join(format!(
        "{SESSION_HOST_BINARY}.{STAGING_TEMP_PREFIX}{attempt_tag}-{pid:x}-{nanos:x}-{sequence:x}"
    ))
}

fn default_attempt_tag() -> String {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("pid{pid:x}-t{nanos:x}")
}

/// Reduce a session name to a safe single path segment, or `None` if it carries
/// no usable content. Path separators, traversal, and other filesystem-special
/// bytes are replaced with `-`; a result that is empty or only separators is
/// rejected so staging can never escape `root` or alias another session.
fn sanitize_session_name(session_name: &str) -> Option<String> {
    let sanitized: String = session_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

/// Replace any non-path-safe characters in a session name before it appears in a
/// diagnostic, so error messages cannot inject path separators or traversal.
fn redact_session_name(session_name: &str) -> String {
    session_name
        .chars()
        .map(|ch| if ch.is_ascii_graphic() { ch } else { '?' })
        .collect()
}

/// Render a source path for diagnostics without canonicalizing (which could
/// fail) and without echoing any file contents.
fn safe_source_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn error_kind_message(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not found".to_owned(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        std::io::ErrorKind::AlreadyExists => "already exists".to_owned(),
        _ => error.to_string(),
    }
}

/// Typed staging failures. Each variant names the failing operation and a safe
/// path; none ever echoes source or staged bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionHostError {
    /// The session name could not be sanitized to a safe path segment.
    InvalidSessionName { session_name: String },
    /// The staging-attempt tag is not safe for use in a filename.
    InvalidAttemptTag,
    /// The source host image could not be read.
    SourceRead { path: PathBuf, reason: String },
    /// A staging directory could not be created.
    StagingCreateDir { path: PathBuf, reason: String },
    /// The staged temp copy could not be written.
    StagingWrite { path: PathBuf, reason: String },
    /// The atomic rename into the final staged path failed.
    StagingRename { path: PathBuf },
}

impl std::error::Error for SessionHostError {}

impl std::fmt::Display for SessionHostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSessionName { session_name } => write!(
                formatter,
                "session host session name is not a safe path segment: {session_name}"
            ),
            Self::InvalidAttemptTag => formatter
                .write_str("session host staging attempt tag is not a safe filename segment"),
            Self::SourceRead { path, reason } => write!(
                formatter,
                "session host source image '{}' could not be read: {reason}",
                path.display()
            ),
            Self::StagingCreateDir { path, reason } => write!(
                formatter,
                "session host staging directory '{}' could not be created: {reason}",
                path.display()
            ),
            Self::StagingWrite { path, reason } => write!(
                formatter,
                "session host staged copy '{}' could not be written: {reason}",
                path.display()
            ),
            Self::StagingRename { path } => write!(
                formatter,
                "session host staged copy could not be renamed into place at '{}'",
                path.display()
            ),
        }
    }
}

// ── Issue #467 Slice 2: per-session cleanup (AC7) and startup sweep (AC8) ───
//
// `cleanup_session_directory` is the kill-path owner of a single session's host
// directory. `startup_cleanup_session_hosts` is the startup sweep that removes
// only unreferenced/dead session directories and interrupted staging temp
// files, retaining live psmux sessions, persisted references, and
// ambiguous/unprobeable artifacts. Both are pure-filesystem and never touch the
// build/install target.

/// Outcome of a single-session cleanup attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCleanupOutcome {
    /// The session's host directory existed and was removed.
    Removed,
    /// No host directory existed for this session.
    Absent,
    /// The directory existed but could not be removed; it is retained for a
    /// later retry. The kill that triggered the cleanup has already terminated
    /// the psmux/tmux session, so a retained directory is harmless leakage
    /// that the next startup sweep (`startup_cleanup_session_hosts`) will
    /// reclaim if it remains unreferenced.
    RetainedForRetry,
}

/// Remove the host directory owned by `session_name` below `root`.
///
/// `root` is the manager's explicit session-host root and `session_name` is the
/// existing `RuntimeBinding.session_name` (e.g. `jefe-<agent>`). Only this
/// session's directory is removed; unrelated sessions are never touched. The
/// session name is sanitized through the same planner used for staging so the
/// derived directory is identical to the staging target. A failure to remove
/// is reported as [`SessionCleanupOutcome::RetainedForRetry`] rather than an
/// `Err`, so a kill caller never aborts after the psmux/tmux session is gone.
/// The only `Err` path is an unsanitizable session name, which is a programmer
/// error rather than a filesystem failure.
pub fn cleanup_session_directory(
    root: &Path,
    session_name: &str,
) -> Result<SessionCleanupOutcome, SessionHostError> {
    let sanitized = sanitize_session_name(session_name).ok_or_else(|| {
        SessionHostError::InvalidSessionName {
            session_name: redact_session_name(session_name),
        }
    })?;
    let directory = root.join(&sanitized);
    if !directory.exists() {
        return Ok(SessionCleanupOutcome::Absent);
    }
    match fs::remove_dir_all(&directory) {
        Ok(()) => Ok(SessionCleanupOutcome::Removed),
        Err(_) => Ok(SessionCleanupOutcome::RetainedForRetry),
    }
}

/// Aggregate report produced by [`startup_cleanup_session_hosts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartupCleanupReport {
    /// Session directories (`<root>/<session>`) removed because they were
    /// unreferenced and their psmux pane was Missing.
    pub removed_session_directories: Vec<PathBuf>,
    /// Live session directories retained because the supplied probe reported
    /// them Alive. Recorded so callers can observe the retention decision.
    pub retained_live_session_directories: Vec<PathBuf>,
    /// Interrupted staging temp files reclaimed from retained session
    /// directories.
    pub removed_temp_files: Vec<PathBuf>,
}

/// Sweep `root` at startup, removing only unreferenced and dead session host
/// directories plus interrupted staging temp files (AC8).
///
/// `persisted_references` is the set of session names with a persisted
/// `RuntimeBinding` (supplied by `app_init` after state load). `probe` decides
/// whether a session name corresponds to a live psmux session; the manager
/// probes live local sessions before deletion. A directory is removed when it
/// is **neither** referenced **nor** live. Directories that cannot be mapped
/// back to a session name (ambiguous artifacts), whose probe is unavailable,
/// or that the caller cannot classify are always retained — startup never
/// deletes a directory it cannot positively identify as unreferenced and dead.
pub fn startup_cleanup_session_hosts(
    root: &Path,
    persisted_references: &[String],
    probe: impl Fn(&str) -> crate::runtime::liveness::SessionLiveness,
) -> Result<StartupCleanupReport, SessionHostError> {
    use crate::runtime::liveness::SessionLiveness;

    let mut report = StartupCleanupReport::default();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => {
            return Err(SessionHostError::StagingCreateDir {
                path: root.to_path_buf(),
                reason: error_kind_message(&error),
            });
        }
    };

    let referenced: std::collections::HashSet<&str> =
        persisted_references.iter().map(String::as_str).collect();

    for entry in entries.flatten() {
        let path = entry.path();
        // Only directories can be session-host directories; stray files at the
        // root are ambiguous artifacts and retained.
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let Some(session_name) = path.file_name().and_then(|name| name.to_str()) else {
            // Non-Unicode entry: ambiguous, retain.
            continue;
        };
        // Reclaim interrupted staging temp files inside the directory before
        // deciding whether to remove the directory itself. This reclaims
        // leftover temps even from retained (live/referenced) sessions.
        reclaim_interrupted_temps_in(&path, &mut report.removed_temp_files);

        // A directory whose name cannot be inverted to a staging session name
        // is ambiguous (e.g. stray artifact dirs, unrelated tools). Retain it.
        let derived_session = invert_session_directory_name(session_name);
        let Some(derived_session) = derived_session else {
            continue;
        };
        let is_referenced =
            referenced.contains(derived_session.as_str()) || referenced.contains(session_name);
        match probe(&derived_session) {
            SessionLiveness::Alive => {
                report.retained_live_session_directories.push(path);
            }
            SessionLiveness::Unavailable => {
                // Unprobeable: retain rather than risk deleting a live session
                // whose probe transiently failed.
            }
            SessionLiveness::Missing if is_referenced => {
                // Persisted reference retains the directory even when the pane
                // probe reports Missing (e.g. the binding is being restored).
            }
            SessionLiveness::Missing => {
                if fs::remove_dir_all(&path).is_ok() {
                    report.removed_session_directories.push(path);
                }
            }
        }
    }

    Ok(report)
}

/// Reclaim interrupted staging temp files inside a session directory.
///
/// Temp files are written as `<binary>.jefe-staging-tmp-<attempt>` (see
/// [`unique_temp_path`]); any file whose name contains the staging temp prefix
/// is reclaimed. The staged binary itself and unrelated files are never
/// removed.
fn reclaim_interrupted_temps_in(session_directory: &Path, removed: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(session_directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Temps live alongside the staged binary inside the digest
            // directory, so recurse one level to reach them.
            reclaim_interrupted_temps_in(&path, removed);
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.contains(STAGING_TEMP_PREFIX) && fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
}

/// Best-effort inversion of a sanitized session directory name back to the
/// `RuntimeBinding.session_name` (`jefe-<agent>`) it was staged from.
///
/// The staging planner sanitizes by replacing every non-alphanumeric byte with
/// `-`, so the inversion is only unambiguous when the directory name already
/// matches the `jefe-<agent>` contract (alphanumeric plus `-`). Any directory
/// whose name does not start with `jefe-` is treated as ambiguous and the
/// caller retains it.
fn invert_session_directory_name(directory_name: &str) -> Option<String> {
    if directory_name.starts_with("jefe-") && !directory_name.is_empty() {
        Some(directory_name.to_owned())
    } else {
        None
    }
}
