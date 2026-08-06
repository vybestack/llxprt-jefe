//! Identity of a jefe *installation*, and the namespace that isolates its
//! multiplexer server.
//!
//! A running jefe is identified by the config/state location it was launched
//! from. That location is what the operator chose, it is what jefe reads and
//! writes, and it is what distinguishes two jefes running side by side. It is
//! emphatically not the machine or the account: those are global to the box, so
//! keying on them collapsed every jefe on the machine into a single namespace,
//! and they change without the installation changing, which silently orphaned
//! every running session (issue #547).
//!
//! This module is the pure core of that idea. It maps a state path to an
//! [`InstallationId`] and records a [`NamespaceOrigin`] explaining where the
//! value came from, so the answer can be reported rather than guessed at. It
//! reads no environment and performs no I/O; resolving the effective paths is
//! the boundary's job (see `installation.rs`), which keeps every input visible
//! to callers and to tests. `tests/core/namespace_derivation_contract.rs`
//! enforces both of those properties on the source.
//!
//! The same identity drives both platforms. Windows renders it as a `-L`
//! server namespace and Unix as a private socket file name, so a worktree is
//! isolated the same way everywhere rather than only where someone remembered
//! to do it.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Prefix shared by every derived identity, so jefe's servers are recognizable
/// among unrelated multiplexer servers owned by the same user.
const IDENTITY_PREFIX: &str = "jefe-";

/// Longest accepted explicit override.
///
/// On Unix the identity becomes a socket file name under a directory that is
/// already close to the kernel's `sun_path` limit, so an unbounded override
/// would surface as a cryptic bind failure rather than as a clear rejection.
const MAX_OVERRIDE_LENGTH: usize = 64;

/// Reduce a state path to the material that names its installation.
///
/// Normalization is deliberately lexical rather than `std::fs::canonicalize`:
/// the state file does not exist before the first save, so canonicalization
/// would fail exactly when a fresh installation needs an identity, and on
/// Windows it emits `\\?\` verbatim prefixes that would not match the same
/// location spelled normally.
///
/// Separator style, trailing separators and ASCII casing are spelling rather
/// than identity. Folding them is what keeps a machine or account rename from
/// moving the namespace out from under running sessions.
fn identity_material(state_path: &Path) -> String {
    let unified: String = state_path
        .to_string_lossy()
        .chars()
        .map(|character| if character == '\\' { '/' } else { character })
        .collect();
    let trimmed = unified.trim_end_matches('/');
    let normalized = if trimmed.is_empty() {
        &unified
    } else {
        trimmed
    };
    normalized.to_ascii_lowercase()
}

/// Hash identity material into a short, wire-safe token.
///
/// Deliberately private. The hash accepts arbitrary bytes, so a caller
/// elsewhere could key the namespace on anything at all, which is precisely how
/// hostname and account material got in. Callers go through the constructors
/// below, which accept only a path or a validated override.
fn hash_identity_material(material: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in material.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Why the active [`InstallationId`] has the value it has.
///
/// Carried alongside the identity so `jefe doctor` and the UI can explain the
/// namespace instead of merely printing it. A namespace nobody can account for
/// is how sessions go missing without anyone noticing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceOrigin {
    /// Derived from the resolved state path. The normal case.
    StatePath(PathBuf),
    /// Set explicitly by the operator for deliberate isolation, such as A/B
    /// testing two multiplexer builds against each other.
    Override(String),
    /// A throwaway namespace owned by one isolated automation run.
    IsolatedRun(PathBuf),
}

impl NamespaceOrigin {
    /// The state path this identity was derived from, when there was one.
    #[must_use]
    pub fn state_path(&self) -> Option<&Path> {
        match self {
            Self::StatePath(path) | Self::IsolatedRun(path) => Some(path),
            Self::Override(_) => None,
        }
    }

    /// Whether the operator asked for this namespace explicitly.
    ///
    /// An override is deliberate isolation, so it is reported loudly: it is the
    /// one way to end up separated from your own running sessions on purpose.
    #[must_use]
    pub const fn is_override(&self) -> bool {
        matches!(self, Self::Override(_))
    }
}

impl fmt::Display for NamespaceOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatePath(path) => {
                write!(formatter, "derived from state path {}", path.display())
            }
            Self::Override(raw) => write!(formatter, "explicit override {raw}"),
            Self::IsolatedRun(path) => write!(
                formatter,
                "isolated run under state path {}",
                path.display()
            ),
        }
    }
}

/// Why an explicit namespace override was refused.
///
/// Rejection is typed rather than silent: an override that quietly fell back to
/// the derived namespace would attach the operator to the very sessions they
/// asked to be separated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceError {
    /// The override was empty or only whitespace.
    Empty,
    /// The override exceeded [`MAX_OVERRIDE_LENGTH`].
    TooLong {
        /// Length that was supplied.
        length: usize,
    },
    /// The override contained a character that is unsafe in a server name or
    /// socket file name.
    IllegalCharacter {
        /// The first offending character.
        character: char,
    },
}

impl fmt::Display for NamespaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "namespace override is empty"),
            Self::TooLong { length } => write!(
                formatter,
                "namespace override is {length} characters; the limit is {MAX_OVERRIDE_LENGTH} \
                 because it becomes a socket file name on Unix"
            ),
            Self::IllegalCharacter { character } => write!(
                formatter,
                "namespace override contains {character:?}; use ASCII letters, digits, '-' or '_'"
            ),
        }
    }
}

impl std::error::Error for NamespaceError {}

/// Stable identity of one jefe installation.
///
/// Rendered into whatever the local multiplexer uses for isolation, so the
/// value is constrained to characters that are safe both as a psmux `-L` server
/// name and as a Unix socket file name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstallationId(String);

impl InstallationId {
    /// The stable identity of the installation rooted at `state_path`.
    #[must_use]
    pub fn for_state_path(state_path: &Path) -> Self {
        let hash = hash_identity_material(&identity_material(state_path));
        Self(format!("{IDENTITY_PREFIX}{hash:016x}"))
    }

    /// A throwaway identity for one isolated run of that installation.
    ///
    /// Extends the stable identity rather than replacing it, so an abandoned
    /// server is still attributable to the installation that created it.
    #[must_use]
    pub fn unique_for_state_path(state_path: &Path) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let stable = Self::for_state_path(state_path);
        Self(format!(
            "{stable}-{:x}-{nanos:x}-{counter:x}",
            std::process::id()
        ))
    }

    /// An identity the operator named explicitly.
    ///
    /// Validated rather than trusted, because the value ends up as a file name
    /// on Unix and as a server name on Windows.
    pub fn from_override(raw: &str) -> Result<Self, NamespaceError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(NamespaceError::Empty);
        }
        if trimmed.len() > MAX_OVERRIDE_LENGTH {
            return Err(NamespaceError::TooLong {
                length: trimmed.len(),
            });
        }
        if let Some(character) = trimmed
            .chars()
            .find(|character| !matches!(character, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_'))
        {
            return Err(NamespaceError::IllegalCharacter { character });
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The identity as it is handed to the multiplexer.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstallationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The active installation identity together with its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationIdentity {
    id: InstallationId,
    origin: NamespaceOrigin,
}

impl InstallationIdentity {
    /// The identity of the installation rooted at `state_path`.
    #[must_use]
    pub fn for_state_path(state_path: &Path) -> Self {
        Self {
            id: InstallationId::for_state_path(state_path),
            origin: NamespaceOrigin::StatePath(state_path.to_path_buf()),
        }
    }

    /// A throwaway identity for one isolated run of that installation.
    #[must_use]
    pub fn isolated_run(state_path: &Path) -> Self {
        Self {
            id: InstallationId::unique_for_state_path(state_path),
            origin: NamespaceOrigin::IsolatedRun(state_path.to_path_buf()),
        }
    }

    /// An identity the operator named explicitly.
    pub fn from_override(raw: &str) -> Result<Self, NamespaceError> {
        Ok(Self {
            id: InstallationId::from_override(raw)?,
            origin: NamespaceOrigin::Override(raw.trim().to_owned()),
        })
    }

    /// The identity handed to the multiplexer.
    #[must_use]
    pub const fn id(&self) -> &InstallationId {
        &self.id
    }

    /// Where the identity came from.
    #[must_use]
    pub const fn origin(&self) -> &NamespaceOrigin {
        &self.origin
    }
}

impl fmt::Display for InstallationIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.id, self.origin)
    }
}
