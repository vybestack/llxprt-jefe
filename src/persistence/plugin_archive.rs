//! Package archive validation (issue #389 CW-09, acceptance rows A1–A5, A8).
//!
//! An accepted archive is **one** gzip member containing **one** POSIX
//! ustar/pax tar whose single root directory is `<plugin-id>-<version>/`.
//! Everything else is rejected before a single byte reaches the filesystem, so
//! this module produces a validated in-memory description and never writes.
//!
//! Two deliberate choices shape the reader:
//!
//! * **Single-member gzip.** [`flate2::read::GzDecoder`] stops at the end of
//!   the first member, which is what makes a concatenated member or a tacked-on
//!   suffix detectable. `MultiGzDecoder` would silently accept both and is
//!   never used.
//! * **Header before body.** Each entry's declared size is checked *before* its
//!   body is read, and the running expanded total is checked as it grows, so a
//!   header that lies about a small body cannot get a large allocation past the
//!   bound.
//!
//! Modes are normalized rather than honoured: an archive's setuid, setgid and
//! sticky bits are discarded, and every file becomes 0755 or 0644 depending
//! only on whether the archive marked it executable. Ownership and timestamps
//! are ignored entirely.

use std::collections::BTreeSet;
use std::io::Read;

use flate2::bufread::GzDecoder;
use tar::EntryType;

use crate::domain::plugin::limits::{
    ARCHIVE_ENTRY_LIMIT, ARCHIVE_EXPANDED_BYTE_LIMIT, MANIFEST_BYTE_LIMIT,
};
use crate::domain::plugin::{
    Manifest, ManifestReadError, PackageCoordinate, RelativePath, read_manifest,
};
use crate::domain::sha256::{Sha256, Sha256Hasher};
use crate::persistence::plugin_inventory::MANIFEST_FILE_NAME;

/// Mode given to an executable file.
const EXECUTABLE_MODE: u32 = 0o755;

/// Mode given to a non-executable file.
const RESOURCE_MODE: u32 = 0o644;

/// One validated regular file from an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFile {
    path: RelativePath,
    mode: u32,
    contents: Vec<u8>,
}

impl ArchiveFile {
    /// The path relative to the package root.
    #[must_use]
    pub const fn path(&self) -> &RelativePath {
        &self.path
    }

    /// The normalized mode this file will be created with.
    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    /// The file's exact bytes.
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }
}

/// A validated archive, ready to stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveContents {
    coordinate: PackageCoordinate,
    manifest: Manifest,
    files: Vec<ArchiveFile>,
    digest: Sha256,
}

impl ArchiveContents {
    /// The package identity the archive declares.
    #[must_use]
    pub const fn coordinate(&self) -> &PackageCoordinate {
        &self.coordinate
    }

    /// The validated manifest.
    #[must_use]
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Every regular file, in archive order.
    #[must_use]
    pub fn files(&self) -> &[ArchiveFile] {
        &self.files
    }

    /// SHA-256 over the package's normalized content.
    ///
    /// The digest covers each file's path, resulting mode and bytes in
    /// listing order — not the archive envelope. That makes it stable across
    /// recompression and identical for an archive install and a developer
    /// directory install of the same tree, which is what lets the two paths be
    /// compared at all.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256 {
        self.digest
    }
}

/// Digest a package's normalized content.
fn content_digest(files: &[ArchiveFile]) -> Sha256 {
    let mut hasher = Sha256Hasher::new();
    for file in files {
        hasher.update(file.path.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update(file.mode.to_be_bytes().as_slice());
        hasher.update(
            u64::try_from(file.contents.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes()
                .as_slice(),
        );
        hasher.update(&file.contents);
    }
    hasher.finalize()
}

/// Read and validate a complete `tar.gz` package archive.
///
/// # Errors
///
/// Returns [`ArchiveError`] for a gzip or tar failure, a forbidden entry type
/// or path, an exceeded bound, a root directory that is absent, duplicated or
/// misnamed, a missing manifest, or a manifest whose identity contradicts the
/// root directory name.
pub fn read_archive(bytes: &[u8]) -> Result<ArchiveContents, ArchiveError> {
    let expanded = decompress_single_member(bytes)?;
    let mut collector = Collector::default();
    collector.absorb(&expanded)?;
    collector.finish()
}

/// Read and validate an unpacked package directory.
///
/// A developer install applies the identical containment, schema, mode and
/// digest rules as an archive install, so it produces the same
/// [`ArchiveContents`] rather than a parallel type with its own near-copy of
/// the rules. Source symlinks are never followed: a link is refused exactly as
/// an archive link entry is.
///
/// # Errors
///
/// Returns [`ArchiveError`] for the same reasons [`read_archive`] does.
pub fn read_directory(source: &std::path::Path) -> Result<ArchiveContents, ArchiveError> {
    let root = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ArchiveError::RootName {
            root: source.display().to_string(),
        })?
        .to_owned();
    let mut collector = Collector {
        root: Some(root),
        ..Collector::default()
    };
    collector.absorb_directory(source, source)?;
    collector
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    collector.finish()
}

/// Decompress exactly one gzip member, rejecting trailing bytes.
///
/// The bound is applied while decompressing rather than afterwards, so a small
/// archive that expands without limit is stopped as it grows.
fn decompress_single_member(bytes: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    // A `BufRead` decoder consumes exactly the bytes of the member it decodes
    // and leaves the rest, which is what makes "is there anything after the
    // member?" answerable. A plain `Read` decoder may pull ahead into its own
    // buffer, so the same question could not be asked of the inner reader.
    let mut remaining: &[u8] = bytes;
    let mut expanded = Vec::new();
    let ceiling = ARCHIVE_EXPANDED_BYTE_LIMIT.saturating_add(1);
    let read = GzDecoder::new(&mut remaining)
        .take(ceiling)
        .read_to_end(&mut expanded)
        .map_err(|error| ArchiveError::Gzip {
            reason: error.to_string(),
        })?;
    if u64::try_from(read).unwrap_or(u64::MAX) > ARCHIVE_EXPANDED_BYTE_LIMIT {
        return Err(ArchiveError::ExpandedTooLarge {
            limit: ARCHIVE_EXPANDED_BYTE_LIMIT,
        });
    }
    if !remaining.is_empty() {
        return Err(ArchiveError::TrailingBytes);
    }
    Ok(expanded)
}

/// Accumulates validated entries while walking the tar.
#[derive(Default)]
struct Collector {
    root: Option<String>,
    files: Vec<ArchiveFile>,
    seen: BTreeSet<String>,
    folded: BTreeSet<String>,
    expanded: u64,
    entries: usize,
}

impl Collector {
    /// Walk every entry of the expanded tar.
    fn absorb(&mut self, expanded: &[u8]) -> Result<(), ArchiveError> {
        let mut archive = tar::Archive::new(expanded);
        let entries = archive.entries().map_err(tar_error)?;
        for entry in entries {
            let mut entry = entry.map_err(tar_error)?;
            self.entries += 1;
            if self.entries > ARCHIVE_ENTRY_LIMIT {
                return Err(ArchiveError::TooManyEntries {
                    limit: ARCHIVE_ENTRY_LIMIT,
                });
            }
            let raw = String::from_utf8(entry.path_bytes().into_owned()).map_err(|_| {
                ArchiveError::ForbiddenPath {
                    path: String::from_utf8_lossy(&entry.path_bytes()).into_owned(),
                    reason: "path is not valid UTF-8",
                }
            })?;
            let kind = entry.header().entry_type();
            let mode = entry.header().mode().map_err(tar_error)?;
            let declared = entry.header().size().map_err(tar_error)?;
            self.absorb_entry(&raw, kind, mode, declared, &mut entry)?;
        }
        Ok(())
    }

    /// Validate one entry and, if it is a regular file, record it.
    fn absorb_entry(
        &mut self,
        raw: &str,
        kind: EntryType,
        mode: u32,
        declared: u64,
        body: &mut impl Read,
    ) -> Result<(), ArchiveError> {
        let trimmed = raw.trim_end_matches('/');
        let is_directory = kind == EntryType::Directory;
        if !matches!(kind, EntryType::Regular | EntryType::Directory) {
            return Err(ArchiveError::ForbiddenEntry {
                path: raw.to_owned(),
                kind: describe(kind),
            });
        }
        let relative = self.relative_of(trimmed, is_directory)?;
        let Some(relative) = relative else {
            return Ok(());
        };
        if is_directory {
            return Ok(());
        }
        self.record_path(&relative, raw)?;
        // The declared size is checked before the body is touched, so a header
        // that lies about a small body cannot get past the bound.
        if declared > u64::try_from(MANIFEST_BYTE_LIMIT).unwrap_or(u64::MAX) {
            return Err(ArchiveError::FileTooLarge {
                path: relative.as_str().to_owned(),
                limit: MANIFEST_BYTE_LIMIT,
            });
        }
        self.expanded = self.expanded.saturating_add(declared);
        if self.expanded > ARCHIVE_EXPANDED_BYTE_LIMIT {
            return Err(ArchiveError::ExpandedTooLarge {
                limit: ARCHIVE_EXPANDED_BYTE_LIMIT,
            });
        }
        let mut contents = Vec::new();
        body.read_to_end(&mut contents).map_err(tar_error)?;
        if u64::try_from(contents.len()).unwrap_or(u64::MAX)
            > u64::try_from(MANIFEST_BYTE_LIMIT).unwrap_or(u64::MAX)
        {
            return Err(ArchiveError::FileTooLarge {
                path: relative.as_str().to_owned(),
                limit: MANIFEST_BYTE_LIMIT,
            });
        }
        self.files.push(ArchiveFile {
            path: relative,
            mode: normalize_mode(mode),
            contents,
        });
        Ok(())
    }

    /// Resolve an archive path to a package-relative one, adopting the root
    /// directory from the first entry.
    ///
    /// Returns `None` for the root directory entry itself.
    fn relative_of(
        &mut self,
        trimmed: &str,
        is_directory: bool,
    ) -> Result<Option<RelativePath>, ArchiveError> {
        let (root, rest) = trimmed.split_once('/').unwrap_or((trimmed, ""));
        if root.is_empty() {
            return Err(ArchiveError::ForbiddenPath {
                path: trimmed.to_owned(),
                reason: "absolute paths are not permitted",
            });
        }
        match &self.root {
            None if is_directory && rest.is_empty() => {
                self.root = Some(root.to_owned());
                return Ok(None);
            }
            None => return Err(ArchiveError::NoRootDirectory),
            Some(existing) if existing == root => {}
            Some(_) => {
                return Err(ArchiveError::OutsideRoot {
                    path: trimmed.to_owned(),
                });
            }
        }
        if rest.is_empty() {
            return Ok(None);
        }
        RelativePath::parse(rest)
            .map(Some)
            .map_err(|_| ArchiveError::ForbiddenPath {
                path: trimmed.to_owned(),
                reason: "path is not a contained package-relative path",
            })
    }

    /// Reject a repeated path, including one that repeats only under case
    /// folding.
    fn record_path(&mut self, relative: &RelativePath, raw: &str) -> Result<(), ArchiveError> {
        if !self.seen.insert(relative.as_str().to_owned()) {
            return Err(ArchiveError::DuplicatePath {
                path: relative.as_str().to_owned(),
            });
        }
        if !self.folded.insert(relative.as_str().to_lowercase()) {
            return Err(ArchiveError::CaseFoldDuplicate {
                path: raw
                    .rsplit_once('/')
                    .map_or(raw, |(_, name)| name)
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Walk an unpacked package directory, applying the archive rules.
    fn absorb_directory(
        &mut self,
        root: &std::path::Path,
        directory: &std::path::Path,
    ) -> Result<(), ArchiveError> {
        let entries = std::fs::read_dir(directory).map_err(tar_error)?;
        for entry in entries {
            let entry = entry.map_err(tar_error)?;
            let path = entry.path();
            // `symlink_metadata` does not follow links, so a link is seen as a
            // link and refused rather than silently resolved to its target.
            let metadata = std::fs::symlink_metadata(&path).map_err(tar_error)?;
            let relative_text = path
                .strip_prefix(root)
                .ok()
                .and_then(|rest| rest.to_str())
                .ok_or_else(|| ArchiveError::OutsideRoot {
                    path: path.display().to_string(),
                })?
                .replace('\\', "/");
            if metadata.is_symlink() {
                return Err(ArchiveError::ForbiddenEntry {
                    path: relative_text,
                    kind: "symbolic link",
                });
            }
            if metadata.is_dir() {
                self.absorb_directory(root, &path)?;
                continue;
            }
            if !metadata.is_file() {
                return Err(ArchiveError::ForbiddenEntry {
                    path: relative_text,
                    kind: "special file",
                });
            }
            self.absorb_source_file(&path, &relative_text, &metadata)?;
        }
        Ok(())
    }

    /// Validate and record one regular file from an unpacked directory.
    fn absorb_source_file(
        &mut self,
        path: &std::path::Path,
        relative_text: &str,
        metadata: &std::fs::Metadata,
    ) -> Result<(), ArchiveError> {
        self.entries += 1;
        if self.entries > ARCHIVE_ENTRY_LIMIT {
            return Err(ArchiveError::TooManyEntries {
                limit: ARCHIVE_ENTRY_LIMIT,
            });
        }
        let relative =
            RelativePath::parse(relative_text).map_err(|_| ArchiveError::ForbiddenPath {
                path: relative_text.to_owned(),
                reason: "path is not a contained package-relative path",
            })?;
        self.record_path(&relative, relative_text)?;
        if metadata.len() > u64::try_from(MANIFEST_BYTE_LIMIT).unwrap_or(u64::MAX) {
            return Err(ArchiveError::FileTooLarge {
                path: relative.as_str().to_owned(),
                limit: MANIFEST_BYTE_LIMIT,
            });
        }
        self.expanded = self.expanded.saturating_add(metadata.len());
        if self.expanded > ARCHIVE_EXPANDED_BYTE_LIMIT {
            return Err(ArchiveError::ExpandedTooLarge {
                limit: ARCHIVE_EXPANDED_BYTE_LIMIT,
            });
        }
        let contents = std::fs::read(path).map_err(tar_error)?;
        self.files.push(ArchiveFile {
            path: relative,
            mode: normalize_mode(source_mode(metadata)),
            contents,
        });
        Ok(())
    }

    /// Resolve the package identity and validate the manifest.
    fn finish(mut self) -> Result<ArchiveContents, ArchiveError> {
        // Canonical path order, so an archive and a developer directory of the
        // same tree digest identically however their entries were listed. The
        // digest is the only thing that lets the two install paths be
        // compared, so it cannot depend on tar entry order.
        self.files.sort_by(|left, right| left.path.cmp(&right.path));
        let root = self.root.ok_or(ArchiveError::NoRootDirectory)?;
        let coordinate = split_root(&root)?;
        let manifest_bytes = self
            .files
            .iter()
            .find(|file| file.path.as_str() == MANIFEST_FILE_NAME)
            .map(|file| file.contents.clone())
            .ok_or(ArchiveError::MissingManifest)?;
        let manifest = read_manifest(&manifest_bytes).map_err(ArchiveError::Manifest)?;
        if manifest.coordinate() != &coordinate {
            return Err(ArchiveError::IdentityMismatch {
                root: coordinate.to_string(),
                manifest: manifest.coordinate().to_string(),
            });
        }
        let digest = content_digest(&self.files);
        Ok(ArchiveContents {
            coordinate,
            manifest,
            files: self.files,
            digest,
        })
    }
}

/// The executable bit of a source file, where the platform reports one.
#[cfg(unix)]
fn source_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode()
}

/// Windows has no executable bit, so a developer install produces resources.
#[cfg(not(unix))]
const fn source_mode(_metadata: &std::fs::Metadata) -> u32 {
    RESOURCE_MODE
}

/// Split `<plugin-id>-<canonical-semver>` into a coordinate.
///
/// A plugin id may itself contain hyphens, so the split is tried at every
/// hyphen and the first pair that parses as a whole wins. Ambiguity is not
/// possible in practice because a version never parses as an id fragment and
/// vice versa; a name that cannot be split at all is rejected.
fn split_root(root: &str) -> Result<PackageCoordinate, ArchiveError> {
    root.match_indices('-')
        .find_map(|(index, _)| {
            let (id, version) = root.split_at(index);
            PackageCoordinate::parse(id, &version[1..]).ok()
        })
        .ok_or_else(|| ArchiveError::RootName {
            root: root.to_owned(),
        })
}

/// Normalize an archive mode, discarding setuid, setgid, sticky and group or
/// other write bits.
const fn normalize_mode(mode: u32) -> u32 {
    if mode & 0o100 == 0 {
        RESOURCE_MODE
    } else {
        EXECUTABLE_MODE
    }
}

/// Name an entry type for a diagnostic.
const fn describe(kind: EntryType) -> &'static str {
    match kind {
        EntryType::Symlink => "symbolic link",
        EntryType::Link => "hard link",
        EntryType::Char => "character device",
        EntryType::Block => "block device",
        EntryType::Fifo => "FIFO",
        EntryType::Continuous => "continuous file",
        EntryType::GNUSparse => "sparse file",
        EntryType::XGlobalHeader => "global pax header",
        EntryType::GNULongName | EntryType::GNULongLink => "GNU long-name extension",
        _ => "unsupported entry",
    }
}

fn tar_error(error: std::io::Error) -> ArchiveError {
    ArchiveError::Tar {
        reason: error.to_string(),
    }
}

/// Why a package archive was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// The gzip layer failed, including a bad header or checksum.
    Gzip { reason: String },
    /// Data follows the single gzip member.
    TrailingBytes,
    /// The tar layer failed.
    Tar { reason: String },
    /// The archive declares no root directory.
    NoRootDirectory,
    /// The root directory is not `<plugin-id>-<canonical-semver>`.
    RootName { root: String },
    /// An entry lies outside the single root directory.
    OutsideRoot { path: String },
    /// An entry is of a type packages may not contain.
    ForbiddenEntry { path: String, kind: &'static str },
    /// An entry path is not a contained package-relative path.
    ForbiddenPath { path: String, reason: &'static str },
    /// Two entries normalize to one path.
    DuplicatePath { path: String },
    /// Two entries collide on a case-insensitive filesystem.
    CaseFoldDuplicate { path: String },
    /// One file exceeds the per-file bound.
    FileTooLarge { path: String, limit: usize },
    /// The expanded total exceeds its bound.
    ExpandedTooLarge { limit: u64 },
    /// The archive contains more entries than permitted.
    TooManyEntries { limit: usize },
    /// The root directory carries no manifest.
    MissingManifest,
    /// The manifest failed validation.
    Manifest(ManifestReadError),
    /// The manifest identity contradicts the root directory name.
    IdentityMismatch { root: String, manifest: String },
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gzip { reason } => write!(formatter, "not a single gzip member: {reason}"),
            Self::TrailingBytes => formatter
                .write_str("data follows the gzip member; concatenated archives are not accepted"),
            Self::Tar { reason } => write!(formatter, "malformed tar archive: {reason}"),
            Self::NoRootDirectory => {
                formatter.write_str("the archive has no single root directory")
            }
            Self::RootName { root } => write!(
                formatter,
                "root directory {root:?} is not <plugin-id>-<version>"
            ),
            Self::OutsideRoot { path } => {
                write!(formatter, "{path:?} lies outside the single root directory")
            }
            Self::ForbiddenEntry { path, kind } => {
                write!(
                    formatter,
                    "{path:?} is a {kind}, which packages may not contain"
                )
            }
            Self::ForbiddenPath { path, reason } => write!(formatter, "{path:?}: {reason}"),
            Self::DuplicatePath { path } => {
                write!(formatter, "{path:?} appears more than once")
            }
            Self::CaseFoldDuplicate { path } => write!(
                formatter,
                "{path:?} collides with another entry on a case-insensitive filesystem"
            ),
            Self::FileTooLarge { path, limit } => {
                write!(formatter, "{path:?} exceeds the {limit} byte file limit")
            }
            Self::ExpandedTooLarge { limit } => {
                write!(formatter, "expanded contents exceed {limit} bytes")
            }
            Self::TooManyEntries { limit } => {
                write!(formatter, "the archive has more than {limit} entries")
            }
            Self::MissingManifest => {
                write!(formatter, "the root directory has no {MANIFEST_FILE_NAME}")
            }
            Self::Manifest(error) => error.fmt(formatter),
            Self::IdentityMismatch { root, manifest } => write!(
                formatter,
                "the root directory names {root} but the manifest declares {manifest}"
            ),
        }
    }
}

impl std::error::Error for ArchiveError {}

#[cfg(test)]
#[path = "plugin_archive_tests.rs"]
mod tests;
