//! Contained harness workspace: creation, materialization, and no-follow
//! containment (issue #380).
//!
//! The workspace is a unique mode-0700 directory. Before every open,
//! mutation, capture, and launch the runner resolves existing ancestors with
//! no-follow handles (`O_NOFOLLOW|O_DIRECTORY` via safe `custom_flags`) and
//! verifies their physical identity — `(dev, ino)` recorded at first
//! observation — remains below the workspace. A changed identity is
//! `HAR-E004`; there is no check-then-follow path. `unsafe` is forbidden, so
//! flags are per-platform constants and identity comes from `std::fs`
//! metadata rather than raw syscalls.

use std::collections::BTreeMap;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::contract::{DirSpec, FileSpec, RelPath, WorkspaceSpec};
use super::error::HarnessError;
use super::limits::MAX_BYTES;

#[cfg(target_os = "macos")]
const O_NOFOLLOW: i32 = 0x0100;
#[cfg(target_os = "macos")]
const O_DIRECTORY: i32 = 0x0010_0000;
#[cfg(target_os = "linux")]
const O_NOFOLLOW: i32 = 0o400_000;
#[cfg(target_os = "linux")]
const O_DIRECTORY: i32 = 0o200_000;

/// Directories the deterministic environment roots in the workspace.
pub const ENV_DIRS: &[&str] = &[
    "home",
    "tmp",
    "bin",
    "jefe-config",
    "jefe-state",
    "jefe-plugins",
];

/// Physical identity of a filesystem object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
}

impl Identity {
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
}

/// A live contained workspace.
#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    root_identity: Identity,
    /// Held open for the whole run so the root's original identity is pinned.
    _root_handle: File,
    /// First-observation identity per relative directory path.
    identities: BTreeMap<String, Identity>,
}

impl Workspace {
    /// Create a unique mode-0700 workspace, its deterministic env dirs, and
    /// materialize the scenario fixtures in declaration order.
    ///
    /// # Errors
    ///
    /// `HAR-E005` for I/O failures, `HAR-E004` for containment violations.
    pub fn create(spec: &WorkspaceSpec) -> Result<Self, HarnessError> {
        let root = unique_root()?;
        DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .map_err(|err| HarnessError::process(format!("create workspace: {err}")))?;
        let root_handle = open_dir_nofollow(&root)?;
        let root_metadata = root_handle
            .metadata()
            .map_err(|err| HarnessError::process(format!("stat workspace root: {err}")))?;
        let mut workspace = Self {
            root_identity: Identity::of(&root_metadata),
            _root_handle: root_handle,
            root,
            identities: BTreeMap::new(),
        };
        for name in ENV_DIRS {
            let path = RelPath((*name).to_string());
            workspace.mkdir(&DirSpec { path, mode: 0o700 })?;
        }
        for dir in &spec.dirs {
            workspace.mkdir(dir)?;
        }
        for file in &spec.files {
            workspace.write_file(file)?;
        }
        Ok(workspace)
    }

    /// Absolute workspace root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a relative path for an operation, verifying every existing
    /// ancestor with no-follow handles. Returns the absolute target path.
    ///
    /// # Errors
    ///
    /// `HAR-E004` when any ancestor is a symlink, escapes the workspace, or
    /// has a changed identity.
    pub fn resolve(&mut self, rel: &RelPath) -> Result<PathBuf, HarnessError> {
        self.verify_root()?;
        let components: Vec<&str> = rel.as_str().split('/').collect();
        let mut prefix = String::new();
        for component in &components[..components.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            self.verify_ancestor(&prefix)?;
        }
        Ok(self.root.join(rel.as_str()))
    }

    /// Apply a `mkdir` operation with the declared mode.
    ///
    /// # Errors
    ///
    /// `HAR-E004` for containment violations, `HAR-E005` for I/O failures.
    pub fn mkdir(&mut self, dir: &DirSpec) -> Result<(), HarnessError> {
        let target = self.resolve(&dir.path)?;
        DirBuilder::new()
            .mode(dir.mode)
            .create(&target)
            .map_err(|err| {
                HarnessError::process(format!("mkdir '{}': {err}", dir.path.as_str()))
            })?;
        let handle = open_dir_nofollow(&target)?;
        let metadata = handle
            .metadata()
            .map_err(|err| HarnessError::process(format!("stat '{}': {err}", dir.path.as_str())))?;
        self.identities
            .insert(dir.path.as_str().to_string(), Identity::of(&metadata));
        Ok(())
    }

    /// Apply a `write` operation: create or replace the file with the
    /// declared mode and content. The target itself is opened `O_NOFOLLOW`.
    ///
    /// # Errors
    ///
    /// `HAR-E004` for containment violations, `HAR-E005` for I/O failures.
    pub fn write_file(&mut self, file: &FileSpec) -> Result<(), HarnessError> {
        let target = self.resolve(&file.path)?;
        let mut handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags(O_NOFOLLOW)
            .mode(file.mode)
            .open(&target)
            .map_err(|err| classify_open_error(&file.path, &err))?;
        // An existing file keeps its original mode; enforce the declared one.
        handle
            .set_permissions(std::os::unix::fs::PermissionsExt::from_mode(file.mode))
            .map_err(|err| {
                HarnessError::process(format!("chmod '{}': {err}", file.path.as_str()))
            })?;
        handle.write_all(file.content.bytes()).map_err(|err| {
            HarnessError::process(format!("write '{}': {err}", file.path.as_str()))
        })?;
        Ok(())
    }

    /// Apply a `remove` operation: delete a file, symlink, or directory tree.
    ///
    /// # Errors
    ///
    /// `HAR-E004` for containment violations, `HAR-E005` for I/O failures.
    pub fn remove(&mut self, rel: &RelPath) -> Result<(), HarnessError> {
        let target = self.resolve(rel)?;
        let metadata = std::fs::symlink_metadata(&target)
            .map_err(|err| HarnessError::process(format!("remove '{}': {err}", rel.as_str())))?;
        let result = if metadata.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        result.map_err(|err| HarnessError::process(format!("remove '{}': {err}", rel.as_str())))?;
        let prefix = format!("{}/", rel.as_str());
        self.identities
            .retain(|path, _| path != rel.as_str() && !path.starts_with(&prefix));
        Ok(())
    }

    /// Read a file's bytes through a no-follow open, bounded by the contract
    /// size limit.
    ///
    /// # Errors
    ///
    /// `HAR-E004` for containment violations, `HAR-E005` for I/O failures or
    /// oversized files.
    pub fn read_file(&mut self, rel: &RelPath) -> Result<Vec<u8>, HarnessError> {
        let target = self.resolve(rel)?;
        let handle = OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW)
            .open(&target)
            .map_err(|err| classify_open_error(rel, &err))?;
        let metadata = handle
            .metadata()
            .map_err(|err| HarnessError::process(format!("stat '{}': {err}", rel.as_str())))?;
        if metadata.len() > MAX_BYTES as u64 {
            return Err(HarnessError::process(format!(
                "file '{}' is {} bytes (max {MAX_BYTES})",
                rel.as_str(),
                metadata.len()
            )));
        }
        let mut reader = handle;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut bytes)
            .map_err(|err| HarnessError::process(format!("read '{}': {err}", rel.as_str())))?;
        Ok(bytes)
    }

    /// Whether a path exists (no-follow), verifying containment on the walk.
    ///
    /// # Errors
    ///
    /// `HAR-E004` for containment violations on ancestors.
    pub fn exists(&mut self, rel: &RelPath) -> Result<bool, HarnessError> {
        let target = self.resolve(rel)?;
        Ok(std::fs::symlink_metadata(&target).is_ok())
    }

    /// Verify the root path still resolves to the pinned physical identity.
    fn verify_root(&self) -> Result<(), HarnessError> {
        let handle = open_dir_nofollow(&self.root)?;
        let metadata = handle
            .metadata()
            .map_err(|err| HarnessError::process(format!("stat workspace root: {err}")))?;
        if Identity::of(&metadata) != self.root_identity {
            return Err(HarnessError::containment(
                "workspace root identity changed".to_string(),
            ));
        }
        Ok(())
    }

    /// Open one ancestor with a no-follow handle and verify its identity
    /// against the first-observation record.
    fn verify_ancestor(&mut self, prefix: &str) -> Result<(), HarnessError> {
        let absolute = self.root.join(prefix);
        let handle = open_dir_nofollow(&absolute)
            .map_err(|err| HarnessError::containment(format!("ancestor '{prefix}': {err}")))?;
        let metadata = handle
            .metadata()
            .map_err(|err| HarnessError::process(format!("stat ancestor '{prefix}': {err}")))?;
        let identity = Identity::of(&metadata);
        match self.identities.get(prefix) {
            Some(recorded) if *recorded != identity => Err(HarnessError::containment(format!(
                "ancestor '{prefix}' physical identity changed"
            ))),
            Some(_) => Ok(()),
            None => {
                self.identities.insert(prefix.to_string(), identity);
                Ok(())
            }
        }
    }
}

/// Open a directory with `O_NOFOLLOW|O_DIRECTORY`. A symlink at the final
/// component fails at the kernel boundary — there is no check-then-follow.
fn open_dir_nofollow(path: &Path) -> Result<File, HarnessError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_DIRECTORY)
        .open(path)
        .map_err(|err| {
            HarnessError::containment(format!(
                "no-follow open of '{}' failed: {err}",
                path.display()
            ))
        })
}

fn classify_open_error(rel: &RelPath, err: &std::io::Error) -> HarnessError {
    // ELOOP (symlink under O_NOFOLLOW) is a containment violation; everything
    // else is a process/I-O failure.
    if err.raw_os_error() == Some(ELOOP) {
        HarnessError::containment(format!("'{}' is a symlink", rel.as_str()))
    } else {
        HarnessError::process(format!("open '{}': {err}", rel.as_str()))
    }
}

#[cfg(target_os = "macos")]
const ELOOP: i32 = 62;
#[cfg(target_os = "linux")]
const ELOOP: i32 = 40;

fn unique_root() -> Result<PathBuf, HarnessError> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| HarnessError::process(format!("clock error: {err}")))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "jefe-harness-{}-{sequence}-{nanos:x}",
        std::process::id()
    )))
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod workspace_tests;
