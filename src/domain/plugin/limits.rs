//! Closed-contract bounds for the plugin package domain (issue #389 CW-09).
//!
//! Every limit named here is the exact value the issue mandates. They are the
//! single source of truth for each N/N+1 boundary test in the focused unit
//! modules, so a bound is never restated as a literal at a call site.

/// Maximum plugin-identifier byte length.
pub const PLUGIN_ID_BYTE_LIMIT: usize = 128;

/// Minimum number of dot-separated labels in a plugin identifier.
///
/// A plugin identifier is always vendor-qualified, which is what keeps it
/// distinguishable from the single-label built-in owner namespaces.
pub const PLUGIN_ID_MINIMUM_LABELS: usize = 2;

/// First labels reserved for owners built into this executable.
///
/// A plugin may never claim one of these, because `core.`, `github.`, and
/// `local.` name the built-in screen and workspace owner namespaces already
/// published by [`crate::config_owners::builtin_owner_catalog`].
pub const RESERVED_FIRST_LABELS: [&str; 3] = ["core", "github", "local"];
