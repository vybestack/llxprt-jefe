//! Privacy-conscious user identity for private multiplexer namespaces.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hash opaque identity bytes into a psmux-compatible namespace without
/// exposing the identity material.
#[must_use]
pub fn namespace_for_identity(identity: &[u8]) -> String {
    let mut hash = FNV_OFFSET;
    for byte in identity {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("jefe-{hash:016x}")
}

/// Return a collision-resistant namespace for isolated automation runs.
#[must_use]
pub fn unique_namespace_for_identity(identity: &[u8]) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!(
        "{}-{:x}-{nanos:x}-{counter:x}",
        namespace_for_identity(identity),
        std::process::id()
    )
}

/// Reduce a state path to the identity material that names its installation.
///
/// Keyed on the resolved state path rather than hostname plus account
/// (issue #547). Machine identity is global to the box, so every jefe on it
/// collapsed into a single namespace, and renaming the machine or changing the
/// account casing silently orphaned every running session. The state path is
/// something jefe controls and persists, so it separates genuinely distinct
/// installations while surviving machine renames.
///
/// Normalization is deliberately lexical rather than `std::fs::canonicalize`:
/// the state file does not exist before the first save, and canonicalization
/// emits `\\?\` verbatim prefixes that would not match the same location
/// spelled normally.
fn state_path_identity_material(state_path: &Path) -> String {
    let unified: String = state_path
        .to_string_lossy()
        .chars()
        .map(|character| if character == '\\' { '/' } else { character })
        .collect();
    let trimmed = unified.trim_end_matches('/');
    let normalized = if trimmed.is_empty() {
        &unified
    } else {
        trimmed
    };
    normalized.to_ascii_lowercase()
}

/// Stable, privacy-safe namespace for the installation rooted at `state_path`.
#[must_use]
pub fn namespace_for_state_path(state_path: &Path) -> String {
    namespace_for_identity(state_path_identity_material(state_path).as_bytes())
}

/// Isolated namespace for one run of the installation rooted at `state_path`.
#[must_use]
pub fn unique_namespace_for_state_path(state_path: &Path) -> String {
    unique_namespace_for_identity(state_path_identity_material(state_path).as_bytes())
}
