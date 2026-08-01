//! Generic, definition-driven executable candidate resolution.

use std::path::{Path, PathBuf};

use crate::agent_candidate_fingerprint::CandidateFingerprint;
pub(crate) use crate::agent_candidate_fingerprint::capture_candidate_fingerprint;
use crate::agent_candidate_path::{AgentWrapperKind, PathSnapshot, resolve_repository_local};
use crate::domain::agent_definition::type_id::{CandidateKind, ExecutableCandidate};
use crate::domain::agent_definition::{AgentDefinition, DefinitionSha256};

const SELECTOR_BYTE_LIMIT: usize = 4_096;

/// Package runner declared by a package-backed executable candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageRunnerKind {
    /// Node package runner.
    Npm,
    /// Python package runner.
    Uvx,
}

impl PackageRunnerKind {
    /// Bare executable name resolved for this runner.
    #[must_use]
    pub const fn executable_name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Uvx => "uvx",
        }
    }
}

/// Owned, normalized package version selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum VersionSelector {
    /// Blank selector: use direct executable candidates.
    #[default]
    Direct,
    /// Latest stable sentinel.
    Latest,
    /// Latest nightly sentinel.
    LatestNightly,
    /// Explicit npm tag/spec or package version.
    Explicit(String),
}

impl VersionSelector {
    /// Normalize a durable/form value while preserving the legacy selector rules.
    ///
    /// All whitespace and invisible clipboard characters are removed. Blank is
    /// direct; sentinels are case-insensitive; all other content is retained as
    /// an owned explicit value.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a NUL byte or a value above the closed bound.
    pub fn normalize(raw: &str) -> Result<Self, VersionSelectorError> {
        if raw.len() > SELECTOR_BYTE_LIMIT {
            return Err(VersionSelectorError::Length);
        }
        if raw.contains('\0') {
            return Err(VersionSelectorError::Nul);
        }
        let normalized: String = raw
            .trim()
            .chars()
            .filter(|character| {
                !character.is_whitespace()
                    && !matches!(
                        character,
                        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{00AD}'
                    )
            })
            .collect();
        if normalized.is_empty() {
            return Ok(Self::Direct);
        }
        if normalized.eq_ignore_ascii_case("latest") {
            return Ok(Self::Latest);
        }
        if normalized.eq_ignore_ascii_case("latestnightly") {
            return Ok(Self::LatestNightly);
        }
        Ok(Self::Explicit(normalized))
    }

    /// Blank/direct selection.
    #[must_use]
    pub const fn is_direct(&self) -> bool {
        matches!(self, Self::Direct)
    }

    /// Whether the selector resolves to a moving package-manager target whose
    /// published version advances over time (a dist-tag or "latest" sentinel).
    ///
    /// `Latest` and `LatestNightly` are volatile: a fresh `npm install` may
    /// resolve them to a newer build each time. `Direct` and `Explicit` are
    /// not — an explicit version is an immutable pin. The managed install cache
    /// uses this to re-resolve volatile selectors periodically (issue #554)
    /// instead of caching the first resolution forever.
    #[must_use]
    pub const fn is_volatile(&self) -> bool {
        matches!(self, Self::Latest | Self::LatestNightly)
    }

    /// Normalized persisted value; `None` means direct.
    #[must_use]
    pub fn normalized(&self) -> Option<&str> {
        match self {
            Self::Direct => None,
            Self::Latest => Some("latest"),
            Self::LatestNightly => Some("latestnightly"),
            Self::Explicit(value) => Some(value),
        }
    }

    /// Effective package-manager selector.
    #[must_use]
    pub fn effective(&self, runner: PackageRunnerKind) -> Option<&str> {
        match (runner, self) {
            (_, Self::Direct) => None,
            (PackageRunnerKind::Npm, Self::Latest)
            | (PackageRunnerKind::Uvx, Self::Latest | Self::LatestNightly) => Some("latest"),
            (PackageRunnerKind::Npm, Self::LatestNightly) => Some("nightly"),
            (_, Self::Explicit(value)) => Some(value),
        }
    }

    /// Closed package spec passed as one structural argv element.
    #[must_use]
    pub fn package_spec(&self, runner: PackageRunnerKind, package: &str) -> Option<String> {
        match (runner, self) {
            (_, Self::Direct) => None,
            (PackageRunnerKind::Npm, _) => self
                .effective(runner)
                .map(|selector| format!("{package}@{selector}")),
            (PackageRunnerKind::Uvx, Self::Latest | Self::LatestNightly) => {
                Some(package.to_owned())
            }
            (PackageRunnerKind::Uvx, Self::Explicit(selector)) => {
                Some(format!("{package}=={selector}"))
            }
        }
    }
}

/// Version-selector construction error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionSelectorError {
    /// Selector exceeds 4096 bytes.
    Length,
    /// Selector contains a NUL byte.
    Nul,
}

impl std::fmt::Display for VersionSelectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length => formatter.write_str("version selector must be 0..=4096 bytes"),
            Self::Nul => formatter.write_str("version selector must not contain a NUL byte"),
        }
    }
}

impl std::error::Error for VersionSelectorError {}

/// Complete package metadata retained by a resolved candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageSelection {
    runner: PackageRunnerKind,
    package: String,
    binary: String,
    selector: VersionSelector,
}

impl PackageSelection {
    fn new(
        runner: PackageRunnerKind,
        package: &str,
        binary: &str,
        selector: VersionSelector,
    ) -> Self {
        Self {
            runner,
            package: package.to_owned(),
            binary: binary.to_owned(),
            selector,
        }
    }

    /// Declared runner.
    #[must_use]
    pub const fn runner(&self) -> PackageRunnerKind {
        self.runner
    }

    /// Declared package.
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Declared binary.
    #[must_use]
    pub fn binary(&self) -> &str {
        &self.binary
    }

    /// Owned normalized selector.
    #[must_use]
    pub const fn selector(&self) -> &VersionSelector {
        &self.selector
    }

    /// Effective package spec.
    #[must_use]
    pub fn package_spec(&self) -> String {
        self.selector
            .package_spec(self.runner, &self.package)
            .unwrap_or_default()
    }
}

/// Whether a resolved candidate launches directly or through a package runner.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CandidateLaunch {
    /// Direct resolved executable.
    Direct,
    /// Package runner plus complete package selection metadata.
    Package(PackageSelection),
}

/// Typed reason one candidate did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSkip {
    PathNameSlash {
        index: usize,
    },
    NotFoundOnPath {
        index: usize,
        name: String,
    },
    RepositoryLocalNotLaunchable {
        index: usize,
    },
    PackageSelectorBlank {
        index: usize,
    },
    DirectSuppressedBySelector {
        index: usize,
    },
    RunnerAbsent {
        index: usize,
        runner: PackageRunnerKind,
    },
    FingerprintCapture {
        index: usize,
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
            | Self::DirectSuppressedBySelector { index }
            | Self::RunnerAbsent { index, .. }
            | Self::FingerprintCapture { index, .. } => *index,
        }
    }
}

impl std::fmt::Display for CandidateSkip {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathNameSlash { index } => write!(
                formatter,
                "candidate {index}: path-name must not contain '/'"
            ),
            Self::NotFoundOnPath { index, name } => {
                write!(formatter, "candidate {index}: {name} not found on PATH")
            }
            Self::RepositoryLocalNotLaunchable { index } => write!(
                formatter,
                "candidate {index}: repository-local binary not launchable"
            ),
            Self::PackageSelectorBlank { index } => {
                write!(formatter, "candidate {index}: package selector is blank")
            }
            Self::DirectSuppressedBySelector { index } => write!(
                formatter,
                "candidate {index}: direct candidate suppressed by package selector"
            ),
            Self::RunnerAbsent { index, runner } => write!(
                formatter,
                "candidate {index}: {} absent from PATH",
                runner.executable_name()
            ),
            Self::FingerprintCapture { index, detail } => write!(
                formatter,
                "candidate {index}: fingerprint capture failed: {detail}"
            ),
        }
    }
}

impl std::error::Error for CandidateSkip {}

/// A physically resolved, fingerprinted candidate ready for probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCandidate {
    index: usize,
    executable: PathBuf,
    wrapper_kind: AgentWrapperKind,
    fingerprint: CandidateFingerprint,
    launch: CandidateLaunch,
}

impl ResolvedCandidate {
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    #[must_use]
    pub const fn wrapper_kind(&self) -> AgentWrapperKind {
        self.wrapper_kind
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &CandidateFingerprint {
        &self.fingerprint
    }
    #[must_use]
    pub const fn launch(&self) -> &CandidateLaunch {
        &self.launch
    }
    #[must_use]
    pub fn package(&self) -> Option<&PackageSelection> {
        match &self.launch {
            CandidateLaunch::Direct => None,
            CandidateLaunch::Package(selection) => Some(selection),
        }
    }

    /// Complete identity used to decide whether probe evidence is still current.
    #[must_use]
    pub fn generation_key(&self, definition: &AgentDefinition) -> CandidateGenerationKey {
        CandidateGenerationKey {
            definition_sha256: definition.sha256(),
            index: self.index,
            executable: self.executable.clone(),
            wrapper_kind: self.wrapper_kind,
            fingerprint: self.fingerprint.clone(),
            launch: self.launch.clone(),
        }
    }
}

/// Candidate inputs whose change requires a new probe generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateGenerationKey {
    definition_sha256: DefinitionSha256,
    index: usize,
    executable: PathBuf,
    wrapper_kind: AgentWrapperKind,
    fingerprint: CandidateFingerprint,
    launch: CandidateLaunch,
}

/// Failure to advance a probe generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeGenerationOverflow;

impl std::fmt::Display for ProbeGenerationOverflow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("probe generation exhausted")
    }
}

impl std::error::Error for ProbeGenerationOverflow {}

/// Retain the generation for identical evidence, otherwise increment it.
pub fn next_probe_generation(
    previous: Option<&CandidateGenerationKey>,
    current: &CandidateGenerationKey,
    generation: u64,
) -> Result<u64, ProbeGenerationOverflow> {
    if previous == Some(current) {
        Ok(generation)
    } else {
        generation.checked_add(1).ok_or(ProbeGenerationOverflow)
    }
}

/// Outcome of resolving one definition's candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateResolution {
    Resolved(ResolvedCandidate),
    NotFound(Vec<CandidateSkip>),
}

impl CandidateResolution {
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }
    #[must_use]
    pub const fn resolved(&self) -> Option<&ResolvedCandidate> {
        match self {
            Self::Resolved(candidate) => Some(candidate),
            Self::NotFound(_) => None,
        }
    }
}

/// Generic resolver over one captured PATH snapshot and one definition selector.
#[derive(Debug, Clone)]
pub struct AgentCandidateResolver<'a> {
    snapshot: &'a PathSnapshot,
    repository_root: PathBuf,
    selector: VersionSelector,
}

impl<'a> AgentCandidateResolver<'a> {
    #[must_use]
    pub fn new(snapshot: &'a PathSnapshot, repository_root: PathBuf) -> Self {
        Self {
            snapshot,
            repository_root,
            selector: VersionSelector::Direct,
        }
    }

    /// Set this definition's sole generic version selector.
    #[must_use]
    pub fn with_version_selector(mut self, selector: VersionSelector) -> Self {
        self.selector = selector;
        self
    }

    #[must_use]
    pub fn resolve(&self, definition: &AgentDefinition) -> CandidateResolution {
        let mut skips = Vec::new();
        for (index, candidate) in definition.candidates.iter().enumerate() {
            match self.resolve_one(index, candidate) {
                ResolveOne::Resolved(found) => return CandidateResolution::Resolved(found),
                ResolveOne::Skip(skip) => skips.push(skip),
            }
        }
        CandidateResolution::NotFound(skips)
    }

    fn resolve_one(&self, index: usize, candidate: &ExecutableCandidate) -> ResolveOne {
        match &candidate.kind {
            CandidateKind::PathName { name } => {
                if !self.selector.is_direct() {
                    return ResolveOne::skip(CandidateSkip::DirectSuppressedBySelector { index });
                }
                if name.contains('/') {
                    return ResolveOne::skip(CandidateSkip::PathNameSlash { index });
                }
                let Some((path, wrapper)) = self.snapshot.resolve_binary(name) else {
                    return ResolveOne::skip(CandidateSkip::NotFoundOnPath {
                        index,
                        name: name.clone(),
                    });
                };
                Self::fingerprint(index, path, wrapper, CandidateLaunch::Direct)
            }
            CandidateKind::RepositoryLlxprt => {
                if !self.selector.is_direct() {
                    return ResolveOne::skip(CandidateSkip::DirectSuppressedBySelector { index });
                }
                let Some((path, wrapper)) = resolve_repository_local(
                    self.snapshot,
                    &self.repository_root,
                    &candidate.value,
                ) else {
                    return ResolveOne::skip(CandidateSkip::RepositoryLocalNotLaunchable { index });
                };
                Self::fingerprint(index, path, wrapper, CandidateLaunch::Direct)
            }
            CandidateKind::NpmPackage { package, binary } => {
                self.resolve_package(index, PackageRunnerKind::Npm, package, binary)
            }
            CandidateKind::UvxPackage { package, binary } => {
                self.resolve_package(index, PackageRunnerKind::Uvx, package, binary)
            }
        }
    }

    fn resolve_package(
        &self,
        index: usize,
        runner: PackageRunnerKind,
        package: &str,
        binary: &str,
    ) -> ResolveOne {
        if self.selector.is_direct() {
            return ResolveOne::skip(CandidateSkip::PackageSelectorBlank { index });
        }
        let Some((path, wrapper)) = self.snapshot.resolve_binary(runner.executable_name()) else {
            return ResolveOne::skip(CandidateSkip::RunnerAbsent { index, runner });
        };
        let selection = PackageSelection::new(runner, package, binary, self.selector.clone());
        Self::fingerprint(index, path, wrapper, CandidateLaunch::Package(selection))
    }

    fn fingerprint(
        index: usize,
        path: PathBuf,
        wrapper_kind: AgentWrapperKind,
        launch: CandidateLaunch,
    ) -> ResolveOne {
        match capture_candidate_fingerprint(&path) {
            Ok(fingerprint) => ResolveOne::Resolved(ResolvedCandidate {
                index,
                executable: fingerprint.canonical_path().to_path_buf(),
                wrapper_kind,
                fingerprint,
                launch,
            }),
            Err(error) => ResolveOne::skip(CandidateSkip::FingerprintCapture {
                index,
                detail: error.to_string(),
            }),
        }
    }
}

enum ResolveOne {
    Resolved(ResolvedCandidate),
    Skip(CandidateSkip),
}
impl ResolveOne {
    fn skip(reason: CandidateSkip) -> Self {
        Self::Skip(reason)
    }
}

#[cfg(test)]
#[path = "agent_candidate_tests.rs"]
mod tests;
