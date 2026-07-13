//! Privacy-conscious user identity for private multiplexer namespaces.

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

#[cfg(windows)]
fn current_identity_material() -> Vec<u8> {
    let account = whoami::username().unwrap_or_else(|_| "local-user".to_owned());
    let host = whoami::hostname().unwrap_or_else(|_| "local".to_owned());
    format!("{host}\0{account}").into_bytes()
}

#[cfg(not(windows))]
fn current_identity_material() -> Vec<u8> {
    std::env::var_os("USER")
        .or_else(|| std::env::var_os("LOGNAME"))
        .map_or_else(
            || b"local-user".to_vec(),
            |value| value.as_encoded_bytes().to_vec(),
        )
}

/// Stable, privacy-safe namespace for the current user.
#[must_use]
pub fn stable_current_user_namespace() -> String {
    namespace_for_identity(&current_identity_material())
}

/// Isolated namespace for the current test/run.
#[must_use]
pub fn unique_current_user_namespace() -> String {
    unique_namespace_for_identity(&current_identity_material())
}
