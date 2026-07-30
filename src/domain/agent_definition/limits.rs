//! Closed-contract bounds for the four-agent definition (issue #382 CW-02).
//!
//! Every limit named here is the exact value the issue's "Deterministic
//! algorithms and limits" section mandates. S1 is a pure validation contract;
//! these constants are the single source of truth for every N/N+1 boundary
//! test in the focused unit modules.

/// Maximum agent-type-id byte length.
pub const ID_BYTE_LIMIT: usize = 128;
/// Maximum executable candidates per definition.
pub const CANDIDATE_LIMIT: usize = 8;
/// Maximum probe argv elements.
pub const PROBE_ARGV_LIMIT: usize = 8;
/// Maximum required capabilities per probe.
pub const CAPABILITY_LIMIT: usize = 32;
/// Maximum fields per scope (repository/agent).
pub const FIELD_SCOPE_LIMIT: usize = 64;
/// Maximum total form fields (repository + agent).
pub const FORM_FIELD_LIMIT: usize = 128;
/// Maximum emitters per definition.
pub const EMITTER_LIMIT: usize = 128;
/// Maximum enum choices per field.
pub const CHOICE_LIMIT: usize = 64;
/// Maximum probe stream bytes (stdout/stderr).
pub const PROBE_STREAM_LIMIT: usize = 65_536;
/// Local probe timeout per child process (milliseconds).
///
/// A definition probe may run identity and capability commands sequentially.
/// Each process receives this independently authored bound, so a two-process
/// probe has an explicit finite combined ceiling of twice this value.
pub const LOCAL_PROBE_TIMEOUT_MS: u64 = 10_000;
/// Remote probe timeout (milliseconds).
pub const REMOTE_PROBE_TIMEOUT_MS: u64 = 20_000;
/// Maximum artifact bytes.
pub const ARTIFACT_LIMIT: usize = 1_048_576;
/// Maximum JSON data depth.
pub const DATA_DEPTH_LIMIT: usize = 16;
/// Maximum JSON map entries.
pub const MAP_LIMIT: usize = 256;
/// Maximum JSON array entries.
pub const ARRAY_LIMIT: usize = 1_024;
/// Maximum path bytes.
pub const PATH_LIMIT: usize = 4_096;
/// Maximum field-id byte length.
pub const FIELD_ID_BYTE_LIMIT: usize = 128;
/// Maximum display-name byte length.
pub const DISPLAY_NAME_BYTE_LIMIT: usize = 256;
/// Maximum string-value byte length.
pub const STRING_VALUE_BYTE_LIMIT: usize = 4_096;
/// Closed definition schema version.
pub const DEFINITION_SCHEMA: u16 = 1;
