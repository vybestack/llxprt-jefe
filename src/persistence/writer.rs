//! Conflict-aware, revision-gated atomic persistence writes.
//!
//! The writer owns durable file replacement. It retains immutable candidate
//! bytes on failure, never removes user files, and removes only temporary files
//! it created before an unsuccessful replacement.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use super::sha256::Sha256;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Immutable serialized bytes retained for conflict or write recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftBytes(Box<[u8]>);

impl DraftBytes {
    /// Construct immutable draft bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes.into_boxed_slice())
    }
}

impl AsRef<[u8]> for DraftBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Expected authority hash at the start of a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedHash {
    /// The target must not exist.
    Absent,
    /// The target must contain bytes with this digest.
    Present(Sha256),
}

/// Backup policy for the authority currently on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPolicy {
    /// Replace without retaining a schema-1 backup.
    None,
    /// Retain the current schema-1 target in a content-addressed sibling.
    RetainSchema1,
}

/// One complete revisioned durable write request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicWrite {
    pub target: PathBuf,
    pub draft: DraftBytes,
    pub expected: ExpectedHash,
    pub revision: u64,
    pub backup: BackupPolicy,
}

/// Freshness decision made immediately before authority replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WritePhase {
    CreateParent,
    ReadTarget,
    CreateBackup,
    WriteBackup,
    SyncBackup,
    SyncBackupParent,
    CreateTemp,
    WriteTemp,
    SyncTemp,
    CheckFreshness,
    Replace,
    SyncParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    CreateParent,
    ReadTarget,
    CreateBackup,
    WriteBackup,
    SyncBackup,
    SyncBackupParent,
    CreateTemp,
    WriteTemp,
    SyncTemp,
    CheckFreshness,
    Replace,
    SyncParent,
}

#[cfg(test)]
impl From<WritePhase> for Phase {
    fn from(value: WritePhase) -> Self {
        match value {
            WritePhase::CreateParent => Self::CreateParent,
            WritePhase::ReadTarget => Self::ReadTarget,
            WritePhase::CreateBackup => Self::CreateBackup,
            WritePhase::WriteBackup => Self::WriteBackup,
            WritePhase::SyncBackup => Self::SyncBackup,
            WritePhase::SyncBackupParent => Self::SyncBackupParent,
            WritePhase::CreateTemp => Self::CreateTemp,
            WritePhase::WriteTemp => Self::WriteTemp,
            WritePhase::SyncTemp => Self::SyncTemp,
            WritePhase::CheckFreshness => Self::CheckFreshness,
            WritePhase::Replace => Self::Replace,
            WritePhase::SyncParent => Self::SyncParent,
        }
    }
}

/// Successful writer completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Authoritative { revision: u64, hash: Sha256 },
    Stale { revision: u64 },
}

/// Conflict or write failure retaining the complete candidate draft.
#[derive(Debug)]
pub struct WriteError {
    diagnostic: Box<Diagnostic>,
    draft: DraftBytes,
}

impl WriteError {
    /// Borrow the redacted typed diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Borrow immutable candidate bytes for explicit export or retry.
    #[must_use]
    pub const fn draft(&self) -> &DraftBytes {
        &self.draft
    }
}

/// Atomically make a matching, current revision authoritative.
pub fn write<F>(operation: AtomicWrite, fresh: F) -> Result<WriteOutcome, WriteError>
where
    F: FnOnce(u64) -> Freshness,
{
    write_with_phases(operation, fresh, |_| Ok(()))
}

#[cfg(test)]
pub(super) fn write_failing_at<F>(
    operation: AtomicWrite,
    fresh: F,
    failure: WritePhase,
) -> Result<WriteOutcome, WriteError>
where
    F: FnOnce(u64) -> Freshness,
{
    write_with_phases(operation, fresh, move |phase| {
        if phase == failure.into() {
            Err(std::io::Error::other("injected writer phase failure"))
        } else {
            Ok(())
        }
    })
}

fn write_with_phases<F, H>(
    operation: AtomicWrite,
    fresh: F,
    mut before: H,
) -> Result<WriteOutcome, WriteError>
where
    F: FnOnce(u64) -> Freshness,
    H: FnMut(Phase) -> std::io::Result<()>,
{
    write_inner(&operation, fresh, &mut before).map_err(|diagnostic| WriteError {
        diagnostic,
        draft: operation.draft,
    })
}

fn write_inner<F, H>(
    operation: &AtomicWrite,
    fresh: F,
    before: &mut H,
) -> Result<WriteOutcome, Box<Diagnostic>>
where
    F: FnOnce(u64) -> Freshness,
    H: FnMut(Phase) -> std::io::Result<()>,
{
    let parent = selected_parent(&operation.target)?;
    run_phase(before, Phase::CreateParent, &operation.target)?;
    fs::create_dir_all(parent).map_err(|error| write_error(&operation.target, error))?;
    run_phase(before, Phase::ReadTarget, &operation.target)?;
    let current = read_expected_target(&operation.target, operation.expected)?;
    retain_requested_backup(operation, current.as_deref(), before)?;
    let temp = create_temp(&operation.target, operation.draft.as_ref(), before)?;
    if let Err(diagnostic) = run_phase(before, Phase::CheckFreshness, &operation.target) {
        remove_owned_temp(&temp.path);
        return Err(diagnostic);
    }
    if fresh(operation.revision) == Freshness::Stale {
        remove_owned_temp(&temp.path);
        return Ok(WriteOutcome::Stale {
            revision: operation.revision,
        });
    }
    drop(temp.file);
    if let Err(diagnostic) = run_phase(before, Phase::Replace, &operation.target) {
        remove_owned_temp(&temp.path);
        return Err(diagnostic);
    }
    if let Err(error) = atomic_replace(&temp.path, &operation.target) {
        remove_owned_temp(&temp.path);
        return Err(write_error(&operation.target, error));
    }
    run_phase(before, Phase::SyncParent, &operation.target)?;
    // The rename already succeeded, so the target holds the new bytes even if
    // this fails; only the directory entry's durability across a crash is
    // unconfirmed. Reporting the error keeps that uncertainty visible, so a
    // retry must re-read the target hash rather than reuse the stale expected
    // hash it computed before this write.
    sync_parent(parent).map_err(|error| write_error(&operation.target, error))?;
    Ok(WriteOutcome::Authoritative {
        revision: operation.revision,
        hash: Sha256::digest(operation.draft.as_ref()),
    })
}

fn retain_requested_backup<H>(
    operation: &AtomicWrite,
    current: Option<&[u8]>,
    before: &mut H,
) -> Result<(), Box<Diagnostic>>
where
    H: FnMut(Phase) -> std::io::Result<()>,
{
    if operation.backup == BackupPolicy::None {
        return Ok(());
    }
    let Some(bytes) = current else {
        return Err(write_detail(
            &operation.target,
            "schema-1 backup requested for an absent target",
        ));
    };
    retain_schema1_backup(&operation.target, bytes, before)
}

fn selected_parent(target: &Path) -> Result<&Path, Box<Diagnostic>> {
    target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| write_detail(target, "selected target has no writable parent directory"))
}

fn read_expected_target(
    target: &Path,
    expected: ExpectedHash,
) -> Result<Option<Vec<u8>>, Box<Diagnostic>> {
    match fs::read(target) {
        Ok(bytes) => {
            if let ExpectedHash::Present(hash) = expected
                && Sha256::digest(&bytes) == hash
            {
                return Ok(Some(bytes));
            }
            Err(conflict(
                target,
                "selected target bytes changed before save",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if expected == ExpectedHash::Absent {
                Ok(None)
            } else {
                Err(conflict(target, "selected target disappeared before save"))
            }
        }
        Err(error) => Err(write_error(target, error)),
    }
}

fn retain_schema1_backup<H>(
    target: &Path,
    bytes: &[u8],
    before: &mut H,
) -> Result<(), Box<Diagnostic>>
where
    H: FnMut(Phase) -> std::io::Result<()>,
{
    let backup = backup_path(target, Sha256::digest(bytes))?;
    match fs::read(&backup) {
        Ok(existing) if existing == bytes => {
            run_phase(before, Phase::SyncBackupParent, &backup)?;
            return sync_parent(selected_parent(&backup)?)
                .map_err(|error| write_error(&backup, error));
        }
        Ok(_) => {
            return Err(write_detail(
                &backup,
                "schema-1 backup path contains different bytes",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(write_error(&backup, error)),
    }
    run_phase(before, Phase::CreateBackup, &backup)?;
    let mut file = open_user_only(&backup).map_err(|error| write_error(&backup, error))?;
    if let Err(diagnostic) = run_phase(before, Phase::WriteBackup, &backup) {
        remove_created_backup(&backup, file);
        return Err(diagnostic);
    }
    if let Err(error) = file.write_all(bytes) {
        remove_created_backup(&backup, file);
        return Err(write_error(&backup, error));
    }
    if let Err(diagnostic) = run_phase(before, Phase::SyncBackup, &backup) {
        remove_created_backup(&backup, file);
        return Err(diagnostic);
    }
    if let Err(error) = file.sync_all() {
        remove_created_backup(&backup, file);
        return Err(write_error(&backup, error));
    }
    drop(file);
    run_phase(before, Phase::SyncBackupParent, &backup)?;
    sync_parent(selected_parent(&backup)?).map_err(|error| write_error(&backup, error))
}

fn remove_created_backup(path: &Path, file: File) {
    drop(file);
    let _ = fs::remove_file(path);
}

fn backup_path(target: &Path, hash: Sha256) -> Result<PathBuf, Box<Diagnostic>> {
    let name = target
        .file_name()
        .ok_or_else(|| write_detail(target, "selected target has no file name"))?;
    let mut backup_name = name.to_os_string();
    backup_name.push(format!(".schema1.{hash}.bak"));
    Ok(target.with_file_name(backup_name))
}

struct OwnedTemp {
    path: PathBuf,
    file: File,
}

fn create_temp<H>(target: &Path, bytes: &[u8], before: &mut H) -> Result<OwnedTemp, Box<Diagnostic>>
where
    H: FnMut(Phase) -> std::io::Result<()>,
{
    for _ in 0..64 {
        let path = temp_path(target)?;
        run_phase(before, Phase::CreateTemp, target)?;
        match open_user_only(&path) {
            Ok(mut file) => {
                write_and_sync_temp(target, &path, bytes, &mut file, before)?;
                return Ok(OwnedTemp { path, file });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(write_error(target, error)),
        }
    }
    Err(write_detail(
        target,
        "could not allocate a unique temporary file",
    ))
}

fn write_and_sync_temp<H>(
    target: &Path,
    path: &Path,
    bytes: &[u8],
    file: &mut File,
    before: &mut H,
) -> Result<(), Box<Diagnostic>>
where
    H: FnMut(Phase) -> std::io::Result<()>,
{
    if let Err(diagnostic) = run_phase(before, Phase::WriteTemp, target) {
        remove_owned_temp(path);
        return Err(diagnostic);
    }
    if let Err(error) = file.write_all(bytes) {
        remove_owned_temp(path);
        return Err(write_error(target, error));
    }
    if let Err(diagnostic) = run_phase(before, Phase::SyncTemp, target) {
        remove_owned_temp(path);
        return Err(diagnostic);
    }
    file.sync_all().map_err(|error| {
        remove_owned_temp(path);
        write_error(target, error)
    })
}

fn temp_path(target: &Path) -> Result<PathBuf, Box<Diagnostic>> {
    let name = target
        .file_name()
        .ok_or_else(|| write_detail(target, "selected target has no file name"))?;
    let ordinal = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".jefe-tmp-{}-{ordinal}", std::process::id()));
    Ok(target.with_file_name(temp_name))
}

#[cfg(unix)]
fn open_user_only(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_user_only(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn remove_owned_temp(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(not(windows))]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    let from = path_to_windows_text(from)?;
    let to = path_to_windows_text(to)?;
    // MOVEFILE is an ordinary constant type in winsafe, so the flags cannot be
    // combined; REPLACE_EXISTING is the one this replacement depends on, and
    // the caller has already flushed the draft to disk.
    winsafe::MoveFileEx(&from, Some(&to), winsafe::co::MOVEFILE::REPLACE_EXISTING)
        .map_err(|error| std::io::Error::other(error.to_string()))
}

#[cfg(windows)]
fn path_to_windows_text(path: &Path) -> std::io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not Unicode"))
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> std::io::Result<()> {
    File::open(parent)?.sync_all()
}

// Mirrors the `unix` arm so the three call sites stay platform-agnostic. These
// platforms provide no way to flush a directory entry, and the rename itself
// already carries the durability guarantee, so this reports the parent's
// reachability rather than inventing a success it did not verify.
#[cfg(not(unix))]
fn sync_parent(parent: &Path) -> std::io::Result<()> {
    parent.metadata().map(|_| ())
}

fn run_phase<H>(before: &mut H, phase: Phase, path: &Path) -> Result<(), Box<Diagnostic>>
where
    H: FnMut(Phase) -> std::io::Result<()>,
{
    before(phase).map_err(|error| write_error(path, error))
}

fn conflict(path: &Path, detail: &str) -> Box<Diagnostic> {
    diagnostic(
        CfgCode::E007,
        path,
        "reload the selected file and reapply the intended edit",
        detail,
    )
}

fn write_error(path: &Path, error: impl std::fmt::Display) -> Box<Diagnostic> {
    write_detail(path, &error.to_string())
}

fn write_detail(path: &Path, detail: &str) -> Box<Diagnostic> {
    diagnostic(
        CfgCode::E104,
        path,
        "preserve the draft and resolve the filesystem write failure",
        detail,
    )
}

fn diagnostic(code: CfgCode, path: &Path, correction: &str, detail: &str) -> Box<Diagnostic> {
    let canonical = path.to_string_lossy().into_owned();
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(canonical),
        None,
        correction,
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    Box::new(diagnostic)
}
