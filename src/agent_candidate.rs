//! Generic, definition-driven candidate resolver (issue #382 CW-02 S2).
//!
//! This module is the filesystem/PATH boundary that consumes closed
//! [`AgentDefinition`] values plus a captured [`PathSnapshot`] and resolves
//! the first physically valid candidate in declared order. Per the issue's
//! deterministic algorithm #1:
//!
//! 1. Snapshot PATH once at startup.
//! 2. For each definition in ID order, inspect candidates in declaration order.
//! 3. `path-name` candidate values containing `/` are rejected except the
//!    typed `repository-llxprt` candidate (the one allowlisted product adapter).
//! 4. An `npm-package`/`uvx-package` candidate participates only when the
//!    agent's persisted version selector is nonblank; it resolves the runner
//!    (`npm`/`uvx`) from the same PATH snapshot and is skipped with a typed
//!    reason when the runner is absent.
//!
//! On a physically valid candidate the resolver canonicalizes the path and
//! captures a [`CandidateFingerprint`] before any probe. It **never** spawns a
//! process and owns no mutable registry or `AppState`: probing identity and
//! capabilities is the next slice (S3), and the registry is the immutable
//! [`crate::agent_registry::AgentTypeRegistry`].
//!
//! Product knowledge lives only in the shipped definition data and the typed
//! `repository-llxprt` candidate kind; this module is otherwise generic.

use std::path::{Path, PathBuf};

use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::{PathSnapshot, resolve_repository_local};
use crate::domain::agent_definition::AgentDefinition;
use crate::domain::agent_definition::type_id::CandidateKind;
use crate::runtime::AgentWrapperKind;

/// One definition's persisted version selector for its package-runner
/// candidates.
///
/// A package-runner candidate participates only when its selector is nonblank
/// (issue deterministic algorithm #1). The resolver takes a blank/nonblank
/// decision rather than the raw selector string so it stays free of the
/// product-specific selector normalization that belongs to S12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VersionSelector {
    npm: Option<&'static str>,
    uvx: Option<&'static str>,
}

impl VersionSelector {
    /// Construct an empty selector (no package-runner participation).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            npm: None,
            uvx: None,
        }
    }

    /// Construct from a runtime selector string for the given runner kind.
    ///
    /// Returns the empty selector when the trimmed value is blank, so the
    /// resolver's blank/nonblank decision is a single `has_*()` check.
    ///
    /// # Errors
    ///
    /// Returns [`VersionSelectorError::Length`] when the selector exceeds the
    /// closed byte bound, or [`VersionSelectorError::Nul`] when it contains a
    /// NUL byte.
    pub fn from_string(
        kind: PackageRunnerKind,
        selector: &str,
    ) -> Result<Self, VersionSelectorError> {
        if selector.trim().is_empty() {
            return Ok(Self::empty());
        }
        if selector.len() > SELECTOR_BYTE_LIMIT {
            return Err(VersionSelectorError::Length);
        }
        if selector.contains('\u{0}') {
            return Err(VersionSelectorError::Nul);
        }
        let interned = selector_intern(selector);
        Ok(match kind {
            PackageRunnerKind::Npm => Self {
                npm: Some(interned),
                uvx: None,
            },
            PackageRunnerKind::Uvx => Self {
                npm: None,
                uvx: Some(interned),
            },
        })
    }

    /// Whether the npm selector is nonblank.
    #[must_use]
    pub const fn has_npm(&self) -> bool {
        self.npm.is_some()
    }

    /// Whether the uvx selector is nonblank.
    #[must_use]
    pub const fn has_uvx(&self) -> bool {
        self.uvx.is_some()
    }
}

/// Maximum selector byte length (matches the closed string-value bound).
const SELECTOR_BYTE_LIMIT: usize = 4_096;

/// Intern a runtime selector so the resolver can carry `&'static str` without
/// a lifetime parameter. Real interning belongs to S12; this stub keeps S2
/// honest by leaking a bounded copy, which is acceptable because selectors are
/// captured once at startup from durable settings and never re-derived.
fn selector_intern(selector: &str) -> &'static str {
    // `Box::leak` is bounded by SELECTOR_BYTE_LIMIT (checked by the caller);
    // selectors are captured once at startup from durable settings and never
    // accumulate.
    Box::leak(selector.to_string().into_boxed_str())
}

/// Which package runner a selector applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageRunnerKind {
    /// npm (`npm exec --yes --package=<package>@<selector> -- <binary>`).
    Npm,
    /// uvx (`uvx --from <package>==<selector> <binary>`).
    Uvx,
}

/// Version-selector construction error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionSelectorError {
    /// Selector longer than the closed bound.
    Length,
    /// Selector contains a NUL byte.
    Nul,
}

impl std::fmt::Display for VersionSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length => f.write_str("version selector must be 0..=4096 bytes"),
            Self::Nul => f.write_str("version selector must not contain a NUL byte"),
        }
    }
}

impl std::error::Error for VersionSelectorError {}

/// Typed reason a single candidate did not resolve.
///
/// Each variant is observable in tests and (later) in status projection; none
/// is a silent fallback. The resolver returns one skip reason per candidate
/// it could not select, plus the declaration index, so the first physically
/// valid candidate's selection is provably deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSkip {
    /// `path-name` candidate contained `/` and is therefore rejected.
    PathNameSlash {
        /// Declaration index within the definition's candidate list.
        index: usize,
    },
    /// Bare binary was not launchable on the captured PATH snapshot.
    NotFoundOnPath {
        /// Declaration index within the definition's candidate list.
        index: usize,
        /// Bare binary name that was searched.
        name: String,
    },
    /// Repository-local candidate's relative path was not a launchable file.
    RepositoryLocalNotLaunchable {
        /// Declaration index within the definition's candidate list.
        index: usize,
    },
    /// Package-runner candidate skipped because its selector is blank.
    PackageSelectorBlank {
        /// Declaration index within the definition's candidate list.
        index: usize,
    },
    /// Package runner (`npm`/`uvx`) absent from the captured PATH snapshot.
    RunnerAbsent {
        /// Declaration index within the definition's candidate list.
        index: usize,
        /// Runner kind that was searched.
        runner: PackageRunnerKind,
    },
    /// Physical fingerprint capture failed (canonicalize/metadata I/O).
    FingerprintCapture {
        /// Declaration index within the definition's candidate list.
        index: usize,
        /// Diagnostic detail without secret-bearing content.
        detail: String,
    },
}

impl CandidateSkip {
    /// Declaration index that produced this skip.
    #[must_use]
    pub const fn index(&self) -> usize {
        match self {
            Self::PathNameSlash { index }
            | Self::NotFoundOnPath { index, .. }
            | Self::RepositoryLocalNotLaunchable { index }
            | Self::PackageSelectorBlank { index }
            | Self::RunnerAbsent { index, .. }
            | Self::FingerprintCapture { index, .. } => *index,
        }
    }
}

impl std::fmt::Display for CandidateSkip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathNameSlash { index } => {
                write!(f, "candidate {index}: path-name must not contain '/'")
            }
            Self::NotFoundOnPath { index, name } => {
                write!(f, "candidate {index}: {name} not found on PATH")
            }
            Self::RepositoryLocalNotLaunchable { index } => {
                write!(
                    f,
                    "candidate {index}: repository-local binary not launchable"
                )
            }
            Self::PackageSelectorBlank { index } => {
                write!(f, "candidate {index}: package selector is blank")
            }
            Self::RunnerAbsent { index, runner } => match runner {
                PackageRunnerKind::Npm => write!(f, "candidate {index}: npm absent from PATH"),
                PackageRunnerKind::Uvx => write!(f, "candidate {index}: uvx absent from PATH"),
            },
            Self::FingerprintCapture { index, detail } => {
                write!(f, "candidate {index}: fingerprint capture failed: {detail}")
            }
        }
    }
}

impl std::error::Error for CandidateSkip {}

/// A physically resolved, fingerprinted candidate ready for probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCandidate {
    /// Declaration index that was selected.
    index: usize,
    /// Canonical, absolute executable path.
    executable: PathBuf,
    /// Launchable-file wrapper kind the runtime must apply.
    wrapper_kind: AgentWrapperKind,
    /// Physical fingerprint captured before probe.
    fingerprint: CandidateFingerprint,
}

impl ResolvedCandidate {
    /// Declaration index that was selected.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Canonical, absolute executable path.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Launchable-file wrapper kind the runtime must apply.
    #[must_use]
    pub const fn wrapper_kind(&self) -> AgentWrapperKind {
        self.wrapper_kind
    }

    /// Physical fingerprint captured before probe.
    #[must_use]
    pub fn fingerprint(&self) -> &CandidateFingerprint {
        &self.fingerprint
    }
}

/// Outcome of resolving one definition's candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateResolution {
    /// First physically valid candidate in declared order.
    Resolved(ResolvedCandidate),
    /// Every candidate was skipped; carries each skip in declaration order so
    /// the caller (and tests) can observe the typed, deterministic reasons.
    NotFound(Vec<CandidateSkip>),
}

impl CandidateResolution {
    /// Whether this is the resolved variant.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }

    /// Borrow the resolved candidate if present.
    #[must_use]
    pub fn resolved(&self) -> Option<&ResolvedCandidate> {
        match self {
            Self::Resolved(c) => Some(c),
            Self::NotFound(_) => None,
        }
    }
}

/// Generic, definition-driven candidate resolver over a captured PATH
/// snapshot.
///
/// Stateless: carries only the inputs the deterministic algorithm needs. It
/// consumes any slice of closed `AgentDefinition` values and resolves each in
/// declaration order. It does not own a mutable registry or `AppState`, and it
/// never spawns a process.
#[derive(Debug, Clone)]
pub struct AgentCandidateResolver<'a> {
    snapshot: &'a PathSnapshot,
    repository_root: PathBuf,
    selectors: VersionSelectors,
}

/// Per-definition version selectors carried by the resolver.
#[derive(Debug, Clone, Default)]
struct VersionSelectors {
    npm: VersionSelector,
    uvx: VersionSelector,
}

impl<'a> AgentCandidateResolver<'a> {
    /// Construct a resolver bound to a captured PATH snapshot and repository
    /// root (for the typed `repository-llxprt` candidate).
    #[must_use]
    pub fn new(snapshot: &'a PathSnapshot, repository_root: PathBuf) -> Self {
        Self {
            snapshot,
            repository_root,
            selectors: VersionSelectors::default(),
        }
    }

    /// Set the npm package selector (nonblank for participation).
    #[must_use]
    pub fn with_npm_selector(mut self, selector: VersionSelector) -> Self {
        self.selectors.npm = selector;
        self
    }

    /// Set the uvx package selector (nonblank for participation).
    #[must_use]
    pub fn with_uvx_selector(mut self, selector: VersionSelector) -> Self {
        self.selectors.uvx = selector;
        self
    }

    /// Resolve the first physically valid candidate for one definition in
    /// declared order.
    ///
    /// Pure read over the captured snapshot: canonicalize + metadata only.
    #[must_use]
    pub fn resolve(&self, definition: &AgentDefinition) -> CandidateResolution {
        let mut skips: Vec<CandidateSkip> = Vec::new();
        for (index, candidate) in definition.candidates.iter().enumerate() {
            match self.resolve_one(index, candidate) {
                ResolveOne::Resolved(found) => {
                    return CandidateResolution::Resolved(found);
                }
                ResolveOne::Skip(skip) => skips.push(skip),
            }
        }
        CandidateResolution::NotFound(skips)
    }

    fn resolve_one(
        &self,
        index: usize,
        candidate: &crate::domain::agent_definition::type_id::ExecutableCandidate,
    ) -> ResolveOne {
        match &candidate.kind {
            CandidateKind::PathName { name } => {
                if name.contains('/') {
                    return ResolveOne::skip(CandidateSkip::PathNameSlash { index });
                }
                let Some((path, wrapper_kind)) = self.snapshot.resolve_binary(name) else {
                    return ResolveOne::skip(CandidateSkip::NotFoundOnPath {
                        index,
                        name: name.clone(),
                    });
                };
                Self::fingerprint(index, path, wrapper_kind)
            }
            CandidateKind::RepositoryLlxprt => {
                // The relative path is the candidate's validated `value`; S1
                // validation rejects `..`, absolute paths, and overlong values.
                let relative = candidate.value.clone();
                let Some((path, wrapper_kind)) =
                    resolve_repository_local(self.snapshot, &self.repository_root, &relative)
                else {
                    return ResolveOne::skip(CandidateSkip::RepositoryLocalNotLaunchable { index });
                };
                Self::fingerprint(index, path, wrapper_kind)
            }
            CandidateKind::NpmPackage { package, binary } => {
                let Some((runner_path, wrapper_kind)) = self.resolve_runner(PackageRunnerKind::Npm)
                else {
                    return ResolveOne::skip(CandidateSkip::RunnerAbsent {
                        index,
                        runner: PackageRunnerKind::Npm,
                    });
                };
                if !self.selectors.npm.has_npm() {
                    return ResolveOne::skip(CandidateSkip::PackageSelectorBlank { index });
                }
                // Package-runner plan argv belongs to S12; S2 only proves the
                // runner resolves and (when requested) fingerprints it.
                let _ = (package, binary);
                Self::fingerprint(index, runner_path, wrapper_kind)
            }
            CandidateKind::UvxPackage { package, binary } => {
                let Some((runner_path, wrapper_kind)) = self.resolve_runner(PackageRunnerKind::Uvx)
                else {
                    return ResolveOne::skip(CandidateSkip::RunnerAbsent {
                        index,
                        runner: PackageRunnerKind::Uvx,
                    });
                };
                if !self.selectors.uvx.has_uvx() {
                    return ResolveOne::skip(CandidateSkip::PackageSelectorBlank { index });
                }
                let _ = (package, binary);
                Self::fingerprint(index, runner_path, wrapper_kind)
            }
        }
    }

    fn resolve_runner(&self, kind: PackageRunnerKind) -> Option<(PathBuf, AgentWrapperKind)> {
        match kind {
            PackageRunnerKind::Npm => self.snapshot.resolve_binary("npm"),
            PackageRunnerKind::Uvx => self.snapshot.resolve_binary("uvx"),
        }
    }

    fn fingerprint(index: usize, path: PathBuf, wrapper_kind: AgentWrapperKind) -> ResolveOne {
        match capture_fingerprint(&path) {
            Ok(fingerprint) => ResolveOne::Resolved(ResolvedCandidate {
                index,
                executable: fingerprint.canonical_path().to_path_buf(),
                wrapper_kind,
                fingerprint,
            }),
            Err(detail) => ResolveOne::skip(CandidateSkip::FingerprintCapture { index, detail }),
        }
    }
}

/// Internal per-candidate outcome.
enum ResolveOne {
    Resolved(ResolvedCandidate),
    Skip(CandidateSkip),
}

impl ResolveOne {
    fn skip(reason: CandidateSkip) -> Self {
        Self::Skip(reason)
    }
}

/// Capture the canonical path and physical metadata fingerprint of one file.
///
/// `dev`/`ino` are captured only where the platform exposes them via
/// `std::os::unix::fs::MetadataExt`. Canonicalize follows symlinks so a
/// repository-local symlink tree resolves to the same canonical target as a
/// direct PATH entry.
fn capture_fingerprint(path: &Path) -> Result<CandidateFingerprint, String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let metadata = std::fs::metadata(&canonical).map_err(|e| e.to_string())?;
    let size = metadata.len();
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let (dev, ino) = capture_dev_ino(&metadata);
    Ok(CandidateFingerprint::new(
        canonical, dev, ino, size, mtime_secs,
    ))
}

#[cfg(unix)]
fn capture_dev_ino(metadata: &std::fs::Metadata) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;
    (Some(metadata.dev()), Some(metadata.ino()))
}

#[cfg(not(unix))]
fn capture_dev_ino(_metadata: &std::fs::Metadata) -> (Option<u64>, Option<u64>) {
    (None, None)
}

#[cfg(test)]
#[path = "agent_candidate_tests.rs"]
mod tests;
