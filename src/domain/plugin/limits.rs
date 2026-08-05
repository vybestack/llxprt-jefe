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

/// Maximum bytes in one package-relative path.
pub const PACKAGE_PATH_BYTE_LIMIT: usize = 1_024;

/// Maximum components in one package-relative path.
pub const PACKAGE_PATH_DEPTH_LIMIT: usize = 16;

/// Maximum bytes in a secret reference's environment-variable name.
pub const SECRET_ENV_BYTE_LIMIT: usize = 128;

/// Maximum enum choices declared by one field.
pub const FIELD_CHOICE_LIMIT: usize = 64;

/// Minimum input contexts one action must declare.
pub const ACTION_CONTEXT_MINIMUM: usize = 1;

/// Maximum input contexts one action may declare.
pub const ACTION_CONTEXT_LIMIT: usize = 32;

/// Maximum argument fields one action may declare.
pub const ACTION_ARGUMENT_LIMIT: usize = 128;

/// Shortest action timeout, in seconds.
pub const ACTION_TIMEOUT_SECONDS_MINIMUM: u32 = 1;

/// Longest action timeout, in seconds.
pub const ACTION_TIMEOUT_SECONDS_LIMIT: u32 = 600;

/// Minimum model kinds one panel must declare.
pub const PANEL_MODEL_KIND_MINIMUM: usize = 1;

/// Maximum data ports one panel may declare.
pub const PANEL_PORT_LIMIT: usize = 32;

/// Maximum activation fields one route may declare.
pub const ROUTE_ACTIVATION_FIELD_LIMIT: usize = 32;

/// Minimum screen identifiers one contribution must bind.
pub const SCREEN_ID_MINIMUM: usize = 1;

/// Maximum screen identifiers one contribution may bind.
pub const SCREEN_ID_LIMIT: usize = 32;

/// Maximum fields one configuration schema may declare.
pub const CONFIG_FIELD_LIMIT: usize = 128;

/// Lowest accepted configuration schema version.
pub const CONFIG_SCHEMA_VERSION_MINIMUM: u32 = 1;

/// The only manifest schema this executable reads.
pub const MANIFEST_SCHEMA: u32 = 1;

/// The only provider protocol this executable speaks.
pub const MANIFEST_PROTOCOL: u32 = 1;

/// Maximum bytes in one manifest or resource file.
pub const MANIFEST_BYTE_LIMIT: usize = 1_048_576;

/// Maximum bytes in a package display name.
pub const DISPLAY_NAME_BYTE_LIMIT: usize = 256;

/// Maximum actions one manifest may declare.
pub const ACTION_LIMIT: usize = 128;

/// Maximum panels one manifest may declare.
pub const PANEL_LIMIT: usize = 32;

/// Maximum routes one manifest may declare.
pub const ROUTE_LIMIT: usize = 32;

/// Maximum screen contributions one manifest may declare.
pub const SCREEN_CONTRIBUTION_LIMIT: usize = 32;
