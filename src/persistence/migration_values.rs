//! Migration-local view of the canonical typed-value and identity helpers.
//!
//! The implementations live in [`crate::domain::canonical_values`] so the
//! durable-state projection in `state/` builds byte-identical schema-2 values
//! without depending on `persistence/`.

pub(super) use crate::domain::canonical_values::{
    canonical_remote_target, digest_parts, json_map_to_typed, normalize_remote_path,
    shipped_definition_hash, stable_id, type_id, typed_map_hash,
};
