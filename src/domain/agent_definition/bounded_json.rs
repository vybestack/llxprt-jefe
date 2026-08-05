//! Definition-schema binding for the shared bounded JSON reader (issue #382).
//!
//! The parsing itself lives in [`crate::domain::bounded_json`], which every
//! closed contract in this crate shares. This module supplies only what is
//! specific to the agent definition: its inclusive bounds, its integer-only
//! number policy, and the mapping from the reader's neutral error into the
//! `AGT-E201` diagnostic taxonomy.
//!
//! The bounds (depth 16, map 256, array 1024, string 4096, artifact 1 MiB) are
//! the exact limits issue #382 mandates.

use super::diagnostics::DefinitionError;
use super::limits::{
    ARRAY_LIMIT, ARTIFACT_LIMIT, DATA_DEPTH_LIMIT, MAP_LIMIT, STRING_VALUE_BYTE_LIMIT,
};
use crate::domain::bounded_json::{BoundedJsonError, BoundedJsonLimits, NumberPolicy};

pub use crate::domain::bounded_json::BoundedJson;

/// The closed definition schema's bounds.
///
/// Numbers are integer-only: the definition contract admits no fraction or
/// exponent, so a fractional literal is a schema error rather than a value.
const DEFINITION_LIMITS: BoundedJsonLimits = BoundedJsonLimits {
    document_bytes: ARTIFACT_LIMIT,
    depth: DATA_DEPTH_LIMIT,
    object_members: MAP_LIMIT,
    array_elements: ARRAY_LIMIT,
    string_bytes: STRING_VALUE_BYTE_LIMIT,
    numbers: NumberPolicy::IntegerOnly,
};

/// Parse a complete JSON document with all closed-contract bounds enforced.
///
/// # Errors
///
/// Returns a [`DefinitionError`] (always prefixed `AGT-E201`) for syntax,
/// duplicate-key, non-integer-number, non-UTF-8, or exceeded-bound failures.
pub fn parse_definition_json(input: &[u8]) -> Result<BoundedJson, DefinitionError> {
    crate::domain::bounded_json::parse(input, &DEFINITION_LIMITS).map_err(definition_error)
}

/// Lower a reader failure into the definition diagnostic taxonomy.
///
/// A duplicate key has its own diagnostic because the definition contract
/// reports it alongside unknown fields; every other reader failure is a
/// malformed artifact, which the taxonomy already carries as a described
/// `UnknownField`.
fn definition_error(error: BoundedJsonError) -> DefinitionError {
    match error {
        BoundedJsonError::DuplicateKey { key } => {
            DefinitionError::DuplicateJsonField { field: key }
        }
        other => DefinitionError::UnknownField {
            field: other.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "bounded_json_tests.rs"]
mod tests;
