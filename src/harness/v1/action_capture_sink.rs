//! Contained application-side writer for the private capture protocol
//! (issue #383 S8, D9).
//!
//! The sink is inert unless [`CAPTURE_PATH_ENV`] names a file, which only the
//! contained schema-1 runner does. When inert, `record` performs no I/O and no
//! allocation beyond the caller's own arguments, so production input routing
//! is unchanged. Records append as newline-delimited JSON so a partial run
//! still yields every completed observation.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::action_capture::{ActionCaptureRecord, CAPTURE_PATH_ENV, encode_record};

/// The configured artifact path, resolved once per process.
fn artifact_path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os(CAPTURE_PATH_ENV).map(PathBuf::from))
        .as_ref()
}

/// True when the contained harness asked for capture.
#[must_use]
pub fn is_active() -> bool {
    artifact_path().is_some()
}

/// Append one record when capture is active; otherwise do nothing.
///
/// Capture is diagnostic evidence, never a behavioral input: a write failure
/// is deliberately swallowed so an unwritable artifact can never change what
/// the application does with the user's keystroke.
pub fn record(build: impl FnOnce() -> ActionCaptureRecord) {
    let Some(path) = artifact_path() else {
        return;
    };
    let Ok(line) = encode_record(&build()) else {
        return;
    };
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path);
    if let Ok(mut file) = opened {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A monotonic frame counter for mouse activations.
#[must_use]
pub fn next_frame() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static FRAME: AtomicU64 = AtomicU64::new(0);
    FRAME.fetch_add(1, Ordering::Relaxed) + 1
}
