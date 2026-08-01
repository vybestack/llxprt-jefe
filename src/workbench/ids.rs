//! Validated identifier vocabulary for internal screen descriptors (issue #384).
//!
//! Every identifier in the descriptor registry is a parsed newtype: once a
//! [`ScreenId`], [`PanelId`], [`RouteId`], or [`PanelTypeId`] exists, its bytes
//! are already known to satisfy the closed grammar, so no consumer re-validates
//! and no consumer can fabricate an unchecked identifier from a raw string.
//!
//! The grammar is deliberately narrow so identifiers stay stable, comparable,
//! and safe to embed in goldens and persisted state:
//!
//! - lowercase ASCII letters, digits, and the separators `.`, `-`, `_`;
//! - non-empty, at most [`ID_BYTE_LIMIT`] bytes;
//! - no leading separator, no trailing separator, no doubled separator;
//! - [`ScreenId`] additionally requires one of the reserved namespaces
//!   (`core.`, `github.`, `local.`) so screen identity is globally partitioned.
//!
//! This module is I/O-free and depends on nothing project-internal.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum byte length of any descriptor identifier.
pub const ID_BYTE_LIMIT: usize = 128;
/// Maximum number of screen descriptors in one registry.
pub const MAX_SCREENS: usize = 64;
/// Maximum number of panels in one screen descriptor.
pub const MAX_PANELS_PER_SCREEN: usize = 16;
/// Minimum number of children a split layout node may declare.
pub const MIN_SPLIT_CHILDREN: usize = 2;
/// Maximum number of children a split layout node may declare.
pub const MAX_SPLIT_CHILDREN: usize = 8;
/// Maximum nesting depth of a layout tree (the root leaf/split is depth 1).
pub const MAX_LAYOUT_DEPTH: usize = 8;

/// Reserved screen-identifier namespaces, in declaration order.
pub const SCREEN_NAMESPACES: [&str; 3] = ["core.", "github.", "local."];

/// Categorized reason an identifier failed the closed grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdError {
    /// The value has no bytes.
    Empty,
    /// The value exceeds [`ID_BYTE_LIMIT`] bytes.
    TooLong,
    /// The value contains a byte outside lowercase ASCII, digits, and `.`/`-`/`_`.
    InvalidByte,
    /// The value starts with a separator.
    LeadingSeparator,
    /// The value ends with a separator.
    TrailingSeparator,
    /// The value contains two adjacent separators.
    DoubledSeparator,
    /// A screen identifier is not in a reserved namespace.
    UnknownNamespace,
}

impl IdError {
    /// User-facing description of the violated rule.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Empty => "identifier is empty",
            Self::TooLong => "identifier exceeds 128 bytes",
            Self::InvalidByte => {
                "identifier may only contain lowercase ASCII letters, digits, '.', '-', and '_'"
            }
            Self::LeadingSeparator => "identifier starts with a separator",
            Self::TrailingSeparator => "identifier ends with a separator",
            Self::DoubledSeparator => "identifier contains two adjacent separators",
            Self::UnknownNamespace => {
                "screen identifier must start with 'core.', 'github.', or 'local.'"
            }
        }
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description())
    }
}

impl std::error::Error for IdError {}

const fn is_separator(byte: u8) -> bool {
    matches!(byte, b'.' | b'-' | b'_')
}

/// Validate the grammar shared by every descriptor identifier.
fn check_plain_grammar(value: &str) -> Result<(), IdError> {
    let bytes = value.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err(IdError::Empty);
    };
    if bytes.len() > ID_BYTE_LIMIT {
        return Err(IdError::TooLong);
    }
    if is_separator(first) {
        return Err(IdError::LeadingSeparator);
    }
    // A trailing separator is reported before generic byte validity so callers
    // get the most specific reason for `core.` style values.
    let Some(&last) = bytes.last() else {
        return Err(IdError::Empty);
    };
    if is_separator(last) {
        return Err(IdError::TrailingSeparator);
    }
    for &byte in bytes {
        if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || is_separator(byte)) {
            return Err(IdError::InvalidByte);
        }
    }
    if bytes
        .windows(2)
        .any(|pair| is_separator(pair[0]) && is_separator(pair[1]))
    {
        return Err(IdError::DoubledSeparator);
    }
    Ok(())
}

/// Declare a validated identifier newtype over the plain grammar.
macro_rules! plain_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Parse and validate an identifier.
            ///
            /// # Errors
            ///
            /// Returns the specific [`IdError`] describing the violated rule.
            pub fn parse(value: &str) -> Result<Self, IdError> {
                check_plain_grammar(value)?;
                Ok(Self(value.to_owned()))
            }

            /// Borrow the validated identifier bytes.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

plain_id! {
    /// Identity of one panel within a screen descriptor.
    PanelId
}
plain_id! {
    /// Navigation route a screen descriptor is reachable through.
    RouteId
}
plain_id! {
    /// Kind of content a panel renders (drives renderer selection and the
    /// PTY-panel guarantee).
    PanelTypeId
}

/// Stable identity of one screen descriptor.
///
/// Unlike the other identifiers this one must sit in a reserved namespace so
/// screen identity is partitioned between built-in application screens
/// (`core.`), GitHub-backed screens (`github.`), and workspace-local screens
/// (`local.`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenId(String);

impl ScreenId {
    /// Parse and validate a namespaced screen identifier.
    ///
    /// # Errors
    ///
    /// Returns the specific [`IdError`] describing the violated rule.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        check_plain_grammar(value)?;
        if !SCREEN_NAMESPACES
            .iter()
            .any(|namespace| value.starts_with(namespace))
        {
            return Err(IdError::UnknownNamespace);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the validated identifier bytes.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ScreenId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Process-unique identity of one resolved screen instance.
///
/// Every [`crate::workbench::ResolvedLayout`] carries the instance it was
/// resolved for, so a consumer can prove it read the same snapshot the renderer
/// used rather than a separately derived one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenInstanceId(u64);

static NEXT_SCREEN_INSTANCE: AtomicU64 = AtomicU64::new(1);

impl ScreenInstanceId {
    /// Allocate the next distinct instance identity.
    #[must_use]
    pub fn next() -> Self {
        Self(NEXT_SCREEN_INSTANCE.fetch_add(1, Ordering::Relaxed))
    }

    /// The raw counter value, for goldens and diagnostics.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ScreenInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "instance-{}", self.0)
    }
}
