//! Validated identifier vocabulary for internal screen descriptors (issue #384).
//!
//! Every identifier in the descriptor registry is a newtype over a `'static`
//! string. The registry is closed — there is no external screen syntax and no
//! override source — so an identifier is always a compiled-in literal, never a
//! value assembled at runtime. Two things follow from that, and both matter:
//!
//! - the types are [`Copy`] and allocation-free, so a resolved layout can be
//!   built every frame without cloning strings;
//! - the constants are usable in `match` patterns, so code that branches on
//!   which screen is active reads as a closed match rather than a chain of
//!   string comparisons.
//!
//! A value arriving from outside the program (a persisted screen value) is
//! never parsed into an identifier directly. It is looked up in the registry
//! via [`crate::workbench::ScreenRegistry::resolve`], so an unknown value
//! cannot become a screen identity that no descriptor backs.
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
/// Maximum number of ports one panel may declare.
pub const MAX_PORTS_PER_PANEL: usize = 32;
/// Maximum number of immutable resource schemas one screen may declare.
pub const MAX_RESOURCES_PER_SCREEN: usize = 64;
/// Maximum number of exact fields one resource schema may declare.
pub const MAX_FIELDS_PER_RESOURCE: usize = 128;
/// Maximum number of relationships one screen may declare.
pub const MAX_RELATIONSHIPS_PER_SCREEN: usize = 64;
/// Maximum number of activation fields one custom screen may declare.
pub const MAX_ACTIVATION_FIELDS: usize = 32;
/// Maximum number of binding references one custom screen may declare.
pub const MAX_BINDINGS_PER_SCREEN: usize = 256;
/// Maximum byte length of the member part of a custom screen identifier.
pub const CUSTOM_MEMBER_BYTE_LIMIT: usize = 63;

/// Namespace every externally authored screen must sit in.
pub const CUSTOM_SCREEN_NAMESPACE: &str = "local.";

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
    /// An externally authored screen identifier is not in the `local.`
    /// namespace.
    NotCustomNamespace,
    /// The member part of a custom screen identifier violates its narrower
    /// grammar.
    InvalidCustomMember,
    /// A versioned type identifier is not `<name>@<version>`.
    MissingTypeVersion,
    /// The version part of a versioned type identifier is not a positive
    /// decimal integer without leading zeros.
    InvalidTypeVersion,
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
            Self::NotCustomNamespace => {
                "externally authored screen identifier must start with 'local.'"
            }
            Self::InvalidCustomMember => "custom screen member must match [a-z][a-z0-9-]{0,62}",
            Self::MissingTypeVersion => "versioned type identifier must be '<name>@<version>'",
            Self::InvalidTypeVersion => {
                "versioned type version must be a positive decimal integer without leading zeros"
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

/// Check any text against the shared identifier grammar.
///
/// Callers validate before interning, so text that can never become an
/// identifier never consumes a slot in the process-lifetime table.
///
/// # Errors
///
/// Returns the specific [`IdError`] describing the violated rule.
pub fn check_identifier(value: &str) -> Result<(), IdError> {
    check_plain_grammar(value)
}

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
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(&'static str);

        impl $name {
            /// Declare a compiled-in identifier.
            ///
            /// The grammar is not checked here because a `const` cannot return
            /// an error. Every declared identifier is checked by
            /// [`Self::check`] in the descriptor validation path and in tests,
            /// so a malformed literal fails before it can reach a renderer.
            #[must_use]
            pub const fn from_static(value: &'static str) -> Self {
                Self(value)
            }

            /// Parse and validate a compiled-in identifier.
            ///
            /// # Errors
            ///
            /// Returns the specific [`IdError`] describing the violated rule.
            pub fn parse(value: &'static str) -> Result<Self, IdError> {
                check_plain_grammar(value)?;
                Ok(Self(value))
            }

            /// Check that this identifier satisfies the grammar.
            ///
            /// # Errors
            ///
            /// Returns the specific [`IdError`] describing the violated rule.
            pub fn check(self) -> Result<(), IdError> {
                check_plain_grammar(self.0)
            }

            /// Borrow the identifier bytes.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.0)
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
plain_id! {
    /// Identity of one screen lowered from a selected plugin package's
    /// descriptor file.
    ///
    /// Unlike [`CustomScreenId`], a package screen's identity is owner-qualified
    /// by the declaring package (e.g. `vendor.pkg.review`) rather than confined
    /// to the `local.` namespace.  Ownership is enforced by manifest validation
    /// before the identity reaches the lowerer, so the plain grammar check
    /// here is sufficient.
    PluginScreenId
}
/// Positive, monotonic, session-only identity of one instantiated panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PanelInstanceId(u64);

#[derive(Debug)]
pub(super) struct PanelInstanceAllocator {
    next: AtomicU64,
}

impl PanelInstanceAllocator {
    pub(super) const fn starting_at(next: u64) -> Self {
        Self {
            next: AtomicU64::new(next),
        }
    }

    pub(super) fn next(&self) -> Result<PanelInstanceId, PanelInstanceIdExhausted> {
        let mut next = self.next.load(Ordering::Relaxed);
        loop {
            if next == 0 || next == u64::MAX {
                return Err(PanelInstanceIdExhausted);
            }
            match self.next.compare_exchange_weak(
                next,
                next + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(PanelInstanceId(next)),
                Err(observed) => next = observed,
            }
        }
    }
}

static PANEL_INSTANCE_ALLOCATOR: PanelInstanceAllocator = PanelInstanceAllocator::starting_at(1);

#[cfg(test)]
std::thread_local! {
    static PANEL_INSTANCE_ALLOCATION_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The process cannot allocate another positive, non-reused panel identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelInstanceIdExhausted;

impl fmt::Display for PanelInstanceIdExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the process cannot allocate another panel instance")
    }
}

impl std::error::Error for PanelInstanceIdExhausted {}

impl PanelInstanceId {
    /// Allocate the next process-unique panel instance identity.
    ///
    /// Exhaustion is unrecoverable for callers without a transactional error
    /// path, so this entry point terminates before zero or reuse can occur.
    #[must_use]
    pub fn next() -> Self {
        match Self::try_next() {
            Ok(instance) => instance,
            Err(_) => std::process::abort(),
        }
    }

    pub(crate) fn try_next() -> Result<Self, PanelInstanceIdExhausted> {
        let instance = PANEL_INSTANCE_ALLOCATOR.next()?;
        #[cfg(test)]
        PANEL_INSTANCE_ALLOCATION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        Ok(instance)
    }

    #[cfg(test)]
    pub(crate) fn test_allocation_count() -> u64 {
        PANEL_INSTANCE_ALLOCATION_COUNT.with(std::cell::Cell::get)
    }

    /// Reconstitute an identity allocated by the screen-instance owner.
    #[must_use]
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// The raw session-local identity value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

plain_id! {
    /// Identity of one typed port within a panel.
    PortId
}

/// Identity of the value a port carries, qualified by its version.
///
/// Two ports may only be joined by a relationship when their versioned types
/// are identical, so the version is part of identity rather than metadata: a
/// panel that starts emitting `github.issue@2` no longer satisfies a target that
/// declared `github.issue@1`, and the mismatch is a validation failure instead
/// of a silent shape change at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VersionedTypeId {
    value: &'static str,
    version: u64,
}

impl VersionedTypeId {
    /// Parse `<name>@<version>`.
    ///
    /// The name follows the plain identifier grammar and the version is a
    /// positive decimal integer without leading zeros, so one type version has
    /// exactly one spelling.
    ///
    /// # Errors
    ///
    /// Returns the specific [`IdError`] describing the violated rule.
    pub fn parse(value: &'static str) -> Result<Self, IdError> {
        if value.len() > ID_BYTE_LIMIT {
            return Err(IdError::TooLong);
        }
        let Some((name, version)) = value.split_once('@') else {
            return Err(IdError::MissingTypeVersion);
        };
        check_plain_grammar(name)?;
        let version = check_type_version(version)?;
        Ok(Self { value, version })
    }

    /// Borrow the full `<name>@<version>` text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.value
    }

    /// Borrow the name part.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.value
            .split_once('@')
            .map_or(self.value, |(name, _)| name)
    }

    /// Numeric schema version.
    #[must_use]
    pub const fn version(self) -> u64 {
        self.version
    }
}

impl fmt::Display for VersionedTypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.value)
    }
}

fn check_type_version(version: &str) -> Result<u64, IdError> {
    let bytes = version.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err(IdError::InvalidTypeVersion);
    };
    if !first.is_ascii_digit() || first == b'0' {
        return Err(IdError::InvalidTypeVersion);
    }
    if bytes.iter().any(|byte| !byte.is_ascii_digit()) {
        return Err(IdError::InvalidTypeVersion);
    }
    version.parse().map_err(|_| IdError::InvalidTypeVersion)
}

/// Identity of one externally authored screen.
///
/// The grammar is narrower than the plain one because the identifier and the
/// file name are the same value: `local.<member>` is declared inside
/// `<member>.screen.toml`, so a member that cannot be a file-name stem cannot be
/// a screen identity either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomScreenId(&'static str);

impl CustomScreenId {
    /// Parse a `local.<member>` identifier.
    ///
    /// # Errors
    ///
    /// Returns the specific [`IdError`] describing the violated rule.
    pub fn parse(value: &'static str) -> Result<Self, IdError> {
        let Some(member) = value.strip_prefix(CUSTOM_SCREEN_NAMESPACE) else {
            return Err(IdError::NotCustomNamespace);
        };
        check_custom_member(member)?;
        Ok(Self(value))
    }

    /// Borrow the full `local.<member>` text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// Borrow the member part, which is also the file-name stem.
    #[must_use]
    pub fn member(self) -> &'static str {
        self.0
            .strip_prefix(CUSTOM_SCREEN_NAMESPACE)
            .unwrap_or(self.0)
    }
}

impl fmt::Display for CustomScreenId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Check the `[a-z][a-z0-9-]{0,62}` member grammar.
///
/// # Errors
///
/// Returns [`IdError::InvalidCustomMember`] for any violation, because the rule
/// is one closed pattern rather than a set of independently reportable rules.
pub fn check_custom_member(member: &str) -> Result<(), IdError> {
    let bytes = member.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err(IdError::InvalidCustomMember);
    };
    if bytes.len() > CUSTOM_MEMBER_BYTE_LIMIT
        || !first.is_ascii_lowercase()
        || bytes
            .iter()
            .any(|&byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    {
        return Err(IdError::InvalidCustomMember);
    }
    Ok(())
}

/// Identity of any descriptor in the composed registry.
///
/// Screen *routing* stays a closed [`ScreenId`] match, because every routable
/// screen must also be rendered and labelled. Screen *description* is open, so
/// that a lowered custom screen can be validated, resolved, and laid out by the
/// same code as a compiled one without inventing a second descriptor type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScreenIdentity {
    /// A screen compiled into this executable.
    Compiled(ScreenId),
    /// A screen lowered from a user definition file.
    Custom(CustomScreenId),
    /// A screen lowered from a selected plugin package's descriptor file.
    Package(PluginScreenId),
}

impl ScreenIdentity {
    /// The stable identity string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compiled(id) => id.as_str(),
            Self::Custom(id) => id.as_str(),
            Self::Package(id) => id.as_str(),
        }
    }

    /// The compiled screen this identity names, if it names one.
    #[must_use]
    pub const fn compiled(self) -> Option<ScreenId> {
        match self {
            Self::Compiled(id) => Some(id),
            Self::Custom(_) | Self::Package(_) => None,
        }
    }

    /// Check that this identity satisfies its grammar.
    ///
    /// # Errors
    ///
    /// Returns the specific [`IdError`] describing the violated rule.
    pub fn check(self) -> Result<(), IdError> {
        match self {
            Self::Compiled(id) => id.check(),
            Self::Custom(id) => CustomScreenId::parse(id.as_str()).map(|_| ()),
            Self::Package(id) => id.check(),
        }
    }
}

impl From<ScreenId> for ScreenIdentity {
    fn from(id: ScreenId) -> Self {
        Self::Compiled(id)
    }
}

/// A compiled screen is equal to the identity that names it.
///
/// This lets a caller compare the open identity against a compiled screen
/// directly (`state.screen() == ScreenId::Issues`). A lowered package or custom
/// screen is never equal to any compiled screen, so the comparison is honest:
/// "are we on the compiled Issues screen?" is `false` for every other identity.
impl PartialEq<ScreenId> for ScreenIdentity {
    fn eq(&self, other: &ScreenId) -> bool {
        matches!(self, Self::Compiled(id) if *id == *other)
    }
}

/// A compiled screen is equal to the identity that names it (reflexive arm).
impl PartialEq<ScreenIdentity> for ScreenId {
    fn eq(&self, other: &ScreenIdentity) -> bool {
        other == self
    }
}

impl fmt::Display for ScreenIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable identity of one screen.
///
/// This is the runtime screen vocabulary and the sole authority for "which
/// screen is active". What a screen *contains* — its panels, focus order, and
/// layout — lives in its descriptor, never here.
///
/// Identity is the namespaced stable string returned by [`Self::as_str`], not
/// the variant's position. Persistence writes and reads that string, and
/// [`Self::from_stable`] resolves by string, so reordering this enum cannot
/// change which screen a restored session opens on.
///
/// It is an enum rather than an open set of constants because every screen must
/// be rendered, routed, and labelled: an exhaustive `match` makes adding a
/// screen a compile error at each of those places instead of a silent fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ScreenId {
    /// Repositories, agents over the embedded terminal, and the preview pane.
    #[default]
    Dashboard,
    /// The split repository view.
    Repositories,
    /// The GitHub issues screen.
    Issues,
    /// The GitHub pull-requests screen.
    PullRequests,
    /// The GitHub workflow-runs screen.
    Actions,
    /// The errors screen.
    Errors,
    /// The Terminal Manager screen.
    Terminals,
    /// The Settings screen.
    Settings,
}

impl ScreenId {
    /// Every screen, in registry order.
    pub const ALL: [Self; 8] = [
        Self::Dashboard,
        Self::Repositories,
        Self::Issues,
        Self::PullRequests,
        Self::Actions,
        Self::Errors,
        Self::Terminals,
        Self::Settings,
    ];

    /// The stable identity string, which is what persistence and descriptors
    /// agree on.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "core.dashboard",
            Self::Repositories => "core.repositories",
            Self::Issues => "github.issues",
            Self::PullRequests => "github.pull-requests",
            Self::Actions => "github.actions",
            Self::Errors => "core.errors",
            Self::Terminals => "core.terminals",
            Self::Settings => "core.settings",
        }
    }

    /// Resolve a screen from its stable identity string.
    ///
    /// Matching is by string, never by position, so an external value can only
    /// name a screen that exists.
    #[must_use]
    pub fn from_stable(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.as_str() == value)
    }

    /// Check that this screen's stable identity satisfies the grammar and sits
    /// in a reserved namespace.
    ///
    /// # Errors
    ///
    /// Returns the specific [`IdError`] describing the violated rule.
    pub fn check(self) -> Result<(), IdError> {
        let value = self.as_str();
        check_plain_grammar(value)?;
        if SCREEN_NAMESPACES
            .iter()
            .any(|namespace| value.starts_with(namespace))
        {
            Ok(())
        } else {
            Err(IdError::UnknownNamespace)
        }
    }
}

impl fmt::Display for ScreenId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Process-unique identity of one resolved screen instance.
///
/// Every [`crate::workbench::ResolvedLayout`] carries the instance it was
/// resolved for, so a consumer can prove it read the same snapshot the renderer
/// used rather than a separately derived one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenInstanceId(u64);

#[derive(Debug)]
pub(super) struct ScreenInstanceAllocator {
    next: AtomicU64,
}

impl ScreenInstanceAllocator {
    pub(super) const fn starting_at(next: u64) -> Self {
        Self {
            next: AtomicU64::new(next),
        }
    }

    pub(super) fn next(&self) -> Result<ScreenInstanceId, ScreenInstanceIdExhausted> {
        let mut next = self.next.load(Ordering::Relaxed);
        loop {
            if next == 0 || next == u64::MAX {
                return Err(ScreenInstanceIdExhausted);
            }
            match self.next.compare_exchange_weak(
                next,
                next + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(ScreenInstanceId(next)),
                Err(observed) => next = observed,
            }
        }
    }
}

static SCREEN_INSTANCE_ALLOCATOR: ScreenInstanceAllocator = ScreenInstanceAllocator::starting_at(1);

#[cfg(test)]
std::thread_local! {
    static SCREEN_INSTANCE_ALLOCATION_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The process cannot allocate another positive, non-reused screen identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenInstanceIdExhausted;

impl fmt::Display for ScreenInstanceIdExhausted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the process cannot allocate another screen instance")
    }
}

impl std::error::Error for ScreenInstanceIdExhausted {}

impl ScreenInstanceId {
    /// Allocate the next distinct instance identity.
    ///
    /// Exhaustion is unrecoverable for callers without a transactional error
    /// path, so this entry point terminates before zero or reuse can occur.
    #[must_use]
    pub fn next() -> Self {
        match Self::try_next() {
            Ok(instance) => instance,
            Err(_) => std::process::abort(),
        }
    }

    pub(crate) fn try_next() -> Result<Self, ScreenInstanceIdExhausted> {
        let instance = SCREEN_INSTANCE_ALLOCATOR.next()?;
        #[cfg(test)]
        SCREEN_INSTANCE_ALLOCATION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        Ok(instance)
    }

    #[cfg(test)]
    pub(crate) fn test_allocation_count() -> u64 {
        SCREEN_INSTANCE_ALLOCATION_COUNT.with(std::cell::Cell::get)
    }

    /// The identity a preview resolves under.
    ///
    /// A preview is geometry nothing is drawn from and nothing compares
    /// against, so it needs an identity that is stable rather than distinct:
    /// allocating one per projection would make repeatedly projecting the same
    /// state change the process. Zero is never allocated, so a preview can
    /// never be mistaken for a live instance.
    #[must_use]
    pub const fn preview() -> Self {
        Self(0)
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

/// Runtime identity of one open screen. This is the same process-unique identity
/// carried by navigation and resolved layout; it is never a definition ID.
pub type OpenScreenId = ScreenInstanceId;
