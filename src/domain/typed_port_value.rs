//! Pure schema-addressed values carried through screen relationships.

use super::{Id, TypedMap};

/// One closed resource value published by a screen control port.
///
/// The immutable workbench schema registry validates all four fields before a
/// relationship transition commits. Keeping this transport type pure lets the
/// same value cross built-in, local, and package definitions without carrying
/// runtime ownership or arbitrary JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedPortValue {
    /// Bare schema type identifier; the version is represented separately.
    pub type_id: Id,
    /// Exact resource schema version.
    pub schema_version: u64,
    /// Stable identity of the resource represented by `value`.
    pub semantic_key: String,
    /// Closed typed fields validated by the published resource schema.
    pub value: TypedMap,
}
