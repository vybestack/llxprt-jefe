//! The package install transaction (issue #389 CW-09, acceptance rows A6, A7, A9).
//!
//! Installing is two phases with exactly one irreversible step between them.
//!
//! **Before the rename** everything happens inside a private staging directory
//! under `<config>/plugins/.staging`. The archive is fully validated, every
//! file is written with a normalized mode, and everything is flushed to disk.
//! A failure anywhere in this phase removes only the staging directory: the
//! installed tree is untouched, so a failed install is indistinguishable from
//! one that never started.
//!
//! **The rename** is the commit. It is atomic, and the destination must not
//! already exist, so an install can never partially overwrite a version that is
//! already installed. If the rename succeeds but the final parent sync does
//! not, the durable result is genuinely unknown — the directory may or may not
//! survive a crash — so that reports [`PluginCode::IndeterminateCommit`]
//! (`PLG-E503`) and the caller rescans the physical tree rather than assuming
//! either outcome or overwriting to "fix" it.
//!
//! Nothing here executes a provider. Installing is a filesystem transaction.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::plugin_archive::{ArchiveContents, ArchiveError, read_archive, read_directory};
use crate::domain::plugin::{PackageCoordinate, PluginCode};
use crate::domain::sha256::Sha256;

/// Directory under `<config>/plugins` holding uncommitted installs.
const STAGING_DIRECTORY: &str = ".staging";

/// Directory under `<config>/plugins` holding committed packages.
const INSTALLED_DIRECTORY: &str = "installed";

/// Mode for the staging root: private to this user.
///
/// Declared on every platform so the call sites stay platform-neutral; the
/// POSIX mode is applied only where the filesystem has one.
const STAGING_MODE: u32 = 0o700;

/// Mode for a created package directory.
///
/// Declared on every platform for the same reason as [`STAGING_MODE`].
const DIRECTORY_MODE: u32 = 0o755;

/// What a committed install produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    coordinate: PackageCoordinate,
    destination: PathBuf,
    digest: Sha256,
}

impl InstallOutcome {
    /// The installed package identity.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// Where the package now lives.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// The content digest of what was installed.
    #[must_use]
    pub const fn digest(&self) -> Sha256 {
        self.digest
    }
}

/// Install a validated `tar.gz` package archive.
///
/// # Errors
///
/// Returns [`InstallError`] when the archive is invalid, the destination
/// already exists, a filesystem step fails, or the commit is indeterminate.
pub fn install_archive(plugins_dir: &Path, bytes: &[u8]) -> Result<InstallOutcome, InstallError> {
    let contents = read_archive(bytes).map_err(InstallError::Archive)?;
    commit(plugins_dir, &contents)
}

/// Install an unpacked package directory, as `plugin install DIR --developer`.
///
/// # Errors
///
/// Returns [`InstallError`] for the same reasons [`install_archive`] does.
pub fn install_developer_directory(
    plugins_dir: &Path,
    source: &Path,
) -> Result<InstallOutcome, InstallError> {
    let contents = read_directory(source).map_err(InstallError::Archive)?;
    commit(plugins_dir, &contents)
}

/// Stage, flush, and atomically commit validated contents.
fn commit(plugins_dir: &Path, contents: &ArchiveContents) -> Result<InstallOutcome, InstallError> {
    let destination = plugins_dir
        .join(INSTALLED_DIRECTORY)
        .join(contents.coordinate().id().as_str())
        .join(contents.coordinate().version().as_str());
    if destination.exists() {
        return Err(InstallError::DestinationExists {
            path: destination.clone(),
        });
    }
    let staging = create_staging(plugins_dir)?;
    // Everything from here to the rename is undone by removing `staging`, so a
    // failure leaves the installed tree exactly as it was.
    let staged = match stage(&staging, contents) {
        Ok(staged) => staged,
        Err(error) => {
            discard(&staging);
            return Err(error);
        }
    };
    if let Some(parent) = destination.parent()
        && let Err(error) = create_directory_all(parent)
    {
        discard(&staging);
        return Err(error);
    }
    if let Err(error) =
        fs::rename(&staged, &destination).map_err(|error| InstallError::Filesystem {
            path: staged.clone(),
            reason: error.to_string(),
        })
    {
        discard(&staging);
        return Err(error);
    }
    discard(&staging);
    // The rename has happened. From here a failure cannot be undone, only
    // reported honestly.
    sync_ancestors(&destination).map_err(|reason| InstallError::IndeterminateCommit {
        destination: destination.clone(),
        reason,
    })?;
    Ok(InstallOutcome {
        coordinate: contents.coordinate().clone(),
        destination,
        digest: contents.content_digest(),
    })
}

/// Create a unique, private staging directory.
fn create_staging(plugins_dir: &Path) -> Result<PathBuf, InstallError> {
    let root = plugins_dir.join(STAGING_DIRECTORY);
    create_directory_all(&root)?;
    // `create_directory_all` gives a package directory its public 0755 mode.
    // The staging root is not a package directory: it holds uncommitted
    // contents, so it is narrowed to this user before anything is written
    // beneath it.
    set_mode(&root, STAGING_MODE)?;
    let mut token = [0u8; 16];
    getrandom::fill(&mut token).map_err(|error| InstallError::Filesystem {
        path: root.clone(),
        reason: format!("cannot name a staging directory: {error}"),
    })?;
    let staging = root.join(hex(&token));
    fs::create_dir(&staging).map_err(|error| InstallError::Filesystem {
        path: staging.clone(),
        reason: error.to_string(),
    })?;
    set_mode(&staging, STAGING_MODE)?;
    Ok(staging)
}

/// Render bytes as lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Write every file into staging and flush it, returning the staged root.
fn stage(staging: &Path, contents: &ArchiveContents) -> Result<PathBuf, InstallError> {
    let staged = staging.join(contents.coordinate().id().as_str());
    create_directory_all(&staged)?;
    for file in contents.files() {
        let target = staged.join(file.path().as_str());
        if let Some(parent) = target.parent() {
            create_directory_all(parent)?;
        }
        write_file(&target, file.contents(), file.mode())?;
    }
    sync_tree(&staged)?;
    Ok(staged)
}

/// Write one file with an explicit mode and flush it to disk.
fn write_file(path: &Path, contents: &[u8], mode: u32) -> Result<(), InstallError> {
    let failure = |reason: String| InstallError::Filesystem {
        path: path.to_path_buf(),
        reason,
    };
    let mut file = File::create(path).map_err(|error| failure(error.to_string()))?;
    file.write_all(contents)
        .map_err(|error| failure(error.to_string()))?;
    file.sync_all()
        .map_err(|error| failure(error.to_string()))?;
    drop(file);
    set_mode(path, mode)
}

/// Create a directory and every missing ancestor.
fn create_directory_all(path: &Path) -> Result<(), InstallError> {
    fs::create_dir_all(path).map_err(|error| InstallError::Filesystem {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    set_mode_if_created(path)
}

/// Give a created package directory its fixed mode.
#[cfg(unix)]
fn set_mode_if_created(path: &Path) -> Result<(), InstallError> {
    set_mode(path, DIRECTORY_MODE)
}

/// Windows has no POSIX mode to apply.
#[cfg(not(unix))]
fn set_mode_if_created(path: &Path) -> Result<(), InstallError> {
    set_mode(path, DIRECTORY_MODE)
}

/// Apply an explicit POSIX mode.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        InstallError::Filesystem {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    })
}

/// Windows carries no POSIX mode, so the intended mode cannot be applied.
///
/// Rather than silently succeeding for any path at all, this confirms the
/// target the caller meant to secure actually exists. Reporting "mode applied"
/// for a directory that is not there would hide a broken install behind a
/// platform difference.
#[cfg(not(unix))]
fn set_mode(path: &Path, _mode: u32) -> Result<(), InstallError> {
    fs::metadata(path)
        .map(|_| ())
        .map_err(|error| InstallError::Filesystem {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
}

/// Flush every directory of the staged tree so the rename has something
/// durable to move.
fn sync_tree(root: &Path) -> Result<(), InstallError> {
    let failure = |path: &Path, error: std::io::Error| InstallError::Filesystem {
        path: path.to_path_buf(),
        reason: error.to_string(),
    };
    let entries = fs::read_dir(root).map_err(|error| failure(root, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| failure(root, error))?;
        if entry.path().is_dir() {
            sync_tree(&entry.path())?;
        }
    }
    File::open(root)
        .and_then(|handle| handle.sync_all())
        .map_err(|error| failure(root, error))
}

/// Flush the destination and its parents after the commit.
fn sync_ancestors(destination: &Path) -> Result<(), String> {
    let mut cursor = Some(destination);
    while let Some(path) = cursor {
        File::open(path)
            .and_then(|handle| handle.sync_all())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        cursor = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        // Two levels above the version directory is `installed`, which is as
        // far as this transaction owns.
        if cursor.is_some_and(|parent| parent.ends_with(INSTALLED_DIRECTORY)) {
            let owner = parent_of(path);
            File::open(owner)
                .and_then(|handle| handle.sync_all())
                .map_err(|error| format!("{}: {error}", owner.display()))?;
            return Ok(());
        }
    }
    Ok(())
}

/// The parent of `path`, or `path` itself at the filesystem root.
fn parent_of(path: &Path) -> &Path {
    path.parent().unwrap_or(path)
}

/// Remove an uncommitted staging directory, ignoring a failure to do so.
///
/// Staging is a scratch area: if it cannot be removed the install result is
/// still correct, and reporting a cleanup failure over the real outcome would
/// bury the actual diagnosis.
fn discard(staging: &Path) {
    let _ = fs::remove_dir_all(staging);
}

/// Why an install did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// The archive or source directory failed validation.
    Archive(ArchiveError),
    /// This exact version is already installed.
    DestinationExists { path: PathBuf },
    /// A filesystem step failed before the commit.
    Filesystem { path: PathBuf, reason: String },
    /// The rename committed but its durability is unconfirmed.
    IndeterminateCommit {
        destination: PathBuf,
        reason: String,
    },
}

impl InstallError {
    /// The stable operator-visible code, where this failure has one.
    #[must_use]
    pub const fn code(&self) -> Option<PluginCode> {
        match self {
            Self::IndeterminateCommit { .. } => Some(PluginCode::IndeterminateCommit),
            Self::Archive(_) | Self::DestinationExists { .. } | Self::Filesystem { .. } => None,
        }
    }

    /// Whether the installed tree is known to be unchanged.
    #[must_use]
    pub const fn installed_tree_unchanged(&self) -> bool {
        !matches!(self, Self::IndeterminateCommit { .. })
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archive(error) => error.fmt(formatter),
            Self::DestinationExists { path } => write!(
                formatter,
                "{} already exists; a version is never overwritten",
                path.display()
            ),
            Self::Filesystem { path, reason } => {
                write!(formatter, "{}: {reason}", path.display())
            }
            Self::IndeterminateCommit {
                destination,
                reason,
            } => write!(
                formatter,
                "{}: {} committed but its durability is unconfirmed: {reason}",
                PluginCode::IndeterminateCommit,
                destination.display()
            ),
        }
    }
}

impl std::error::Error for InstallError {}

#[cfg(test)]
#[path = "plugin_install_tests.rs"]
mod tests;
