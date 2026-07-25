//! Inclusive resource bounds for the schema-1 harness contract (issue #380).
//!
//! Every bound is inclusive: a value at the limit is valid; limit plus one
//! fails with `HAR-E002` before any launch.

/// Maximum bytes for scenario input, the report, each materialized file, and
/// each captured stream.
pub const MAX_BYTES: usize = 1_048_576;
/// Maximum bytes for any single JSON string value.
pub const MAX_STRING_BYTES: usize = 262_144;
/// Maximum JSON nesting depth.
pub const MAX_DEPTH: usize = 16;
/// Maximum members per JSON object.
pub const MAX_OBJECT_MEMBERS: usize = 256;
/// Maximum elements per JSON array.
pub const MAX_ARRAY_ELEMENTS: usize = 1_024;
/// Maximum frames recorded in a report.
pub const MAX_FRAMES: usize = 2_048;
/// Maximum registered captures.
pub const MAX_CAPTURES: usize = 256;
/// Maximum recorded processes per capture.
pub const MAX_PROCESSES_PER_CAPTURE: usize = 32;
/// Steps per scenario: 1..=1024.
pub const MAX_STEPS: usize = 1_024;
/// Secrets per scenario: 0..=64.
pub const MAX_SECRETS: usize = 64;
/// Maximum launch argv elements.
pub const MAX_ARGV: usize = 64;
/// Maximum env entries per env list.
pub const MAX_ENV: usize = 256;
/// Maximum dirs/files per workspace.
pub const MAX_WORKSPACE_ENTRIES: usize = 256;
/// Maximum bytes in a relative path.
pub const MAX_PATH_BYTES: usize = 4_096;
/// Maximum repeat count after the leading env-name character
/// (`[A-Z_][A-Z0-9_]{0,127}` ⇒ 128 bytes total).
pub const MAX_ENV_NAME_LEN: usize = 127;
/// Terminal column range.
pub const COLS_RANGE: (u64, u64) = (1, 500);
/// Terminal row range.
pub const ROWS_RANGE: (u64, u64) = (1, 200);
/// Wait timeout range in milliseconds.
pub const TIMEOUT_MS_RANGE: (u64, u64) = (1, 30_000);
/// Modifier list bound per key step.
pub const MAX_MODIFIERS: usize = 3;
