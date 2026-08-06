//! Provider declaration and exact host-triple binary selection
//! (issue #389 CW-09, acceptance row D8).
//!
//! A package either declares no provider at all or declares one binary per
//! exact build host triple. Selection is exact: a near-miss triple resolves to
//! [`ProviderSelection::UnsupportedPlatform`], never to an approximate binary,
//! because `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl` differ in
//! a way that shows up as a load failure rather than a diagnosable message.
//!
//! Nothing here executes anything. Selection yields a declared relative path;
//! starting a provider is CW-10's concern.

use std::collections::BTreeMap;
use std::fmt;

use super::values::{HostTriple, RelativePath};

/// How a package's provider runs, if at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderMode {
    /// The package declares no provider process.
    None,
    /// The provider is started per invocation and exits.
    OneShot,
    /// The provider is started once and stays resident.
    Persistent,
}

impl ProviderMode {
    /// Every mode, in declaration order.
    pub const ALL: [Self; 3] = [Self::None, Self::OneShot, Self::Persistent];

    /// The lower-kebab-case name used on the wire.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OneShot => "one-shot",
            Self::Persistent => "persistent",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_wire() == value)
    }

    /// Whether this mode starts a process at all.
    #[must_use]
    pub const fn is_executable(self) -> bool {
        matches!(self, Self::OneShot | Self::Persistent)
    }
}

impl fmt::Display for ProviderMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_wire())
    }
}

/// A package's validated provider declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    mode: ProviderMode,
    binaries: BTreeMap<HostTriple, RelativePath>,
}

impl Provider {
    /// Validate a provider declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when mode `none` declares a binary, when an
    /// executable mode declares none, or when a host triple is declared twice.
    pub fn parse(
        mode: ProviderMode,
        binaries: Vec<(HostTriple, RelativePath)>,
    ) -> Result<Self, ProviderError> {
        let mut map = BTreeMap::new();
        for (triple, path) in binaries {
            if map.insert(triple.clone(), path).is_some() {
                return Err(ProviderError::DuplicateTriple {
                    triple: triple.as_str().to_owned(),
                });
            }
        }
        if mode.is_executable() {
            if map.is_empty() {
                return Err(ProviderError::ExecutableWithoutBinaries);
            }
        } else if !map.is_empty() {
            return Err(ProviderError::NoneDeclaresBinaries);
        }
        Ok(Self {
            mode,
            binaries: map,
        })
    }

    /// The declared mode.
    #[must_use]
    pub const fn mode(&self) -> ProviderMode {
        self.mode
    }

    /// Whether this package declares a provider process.
    #[must_use]
    pub const fn is_executable(&self) -> bool {
        self.mode.is_executable()
    }

    /// Declared binaries, ordered by host triple.
    #[must_use]
    pub const fn binaries(&self) -> &BTreeMap<HostTriple, RelativePath> {
        &self.binaries
    }

    /// Resolve the binary for a host, matching the triple exactly.
    #[must_use]
    pub fn select(&self, host: &HostTriple) -> ProviderSelection<'_> {
        if !self.is_executable() {
            return ProviderSelection::NotDeclared;
        }
        self.binaries
            .get(host)
            .map_or(ProviderSelection::UnsupportedPlatform, |path| {
                ProviderSelection::Ready(path)
            })
    }

    /// The operator-facing reason this host has no binary, if that is the case.
    #[must_use]
    pub fn unsupported_message(&self, host: &HostTriple) -> Option<String> {
        match self.select(host) {
            ProviderSelection::UnsupportedPlatform => {
                Some(format!("no binary for {}", host.as_str()))
            }
            ProviderSelection::NotDeclared | ProviderSelection::Ready(_) => None,
        }
    }
}

/// The outcome of resolving a provider binary for one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSelection<'a> {
    /// The package declares no provider, which is not a failure.
    NotDeclared,
    /// The declared binary for this exact host.
    Ready(&'a RelativePath),
    /// A provider is declared, but not for this host.
    UnsupportedPlatform,
}

/// Why a provider declaration is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Mode `none` declared a binary.
    NoneDeclaresBinaries,
    /// An executable mode declared no binary at all.
    ExecutableWithoutBinaries,
    /// One host triple was declared more than once.
    DuplicateTriple { triple: String },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoneDeclaresBinaries => {
                formatter.write_str("a provider of mode none may not declare binaries")
            }
            Self::ExecutableWithoutBinaries => {
                formatter.write_str("an executable provider must declare at least one binary")
            }
            Self::DuplicateTriple { triple } => {
                write!(formatter, "host triple {triple:?} is declared twice")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
