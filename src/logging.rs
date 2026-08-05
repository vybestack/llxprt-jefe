//! Application logging setup using `tracing` + `tracing-subscriber`.
//!
//! Controlled by two environment variables:
//! - `JEFE_LOG_FILE` — path to the log file. If unset, logging is disabled.
//! - `JEFE_LOG` — filter directive (e.g. `debug`, `jefe=trace`).
//!   Defaults to `info,jefe=debug` when omitted.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tracing_subscriber::EnvFilter;

static LOG_FILE: OnceLock<Arc<File>> = OnceLock::new();

/// Returns the configured log file path, if any.
pub fn log_file_path() -> Option<PathBuf> {
    std::env::var("JEFE_LOG_FILE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("JEFE_DEBUG_LOG").ok().map(PathBuf::from))
}

/// Initialize the global tracing subscriber.
///
/// Call once at the start of `main()`. If `JEFE_LOG_FILE` is not set,
/// this is a no-op and no subscriber is installed.
pub fn init() {
    let Some(path) = log_file_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        write_log_open_warning(&path);
        return;
    };

    let file = Arc::new(file);
    let _ = LOG_FILE.set(Arc::clone(&file));

    let filter = std::env::var("JEFE_LOG")
        .ok()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .or_else(|| EnvFilter::try_new("info,jefe=debug").ok())
        .unwrap_or_else(|| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .try_init();
}

/// Push every buffered log byte to durable storage.
///
/// The subscriber writes straight to the file handle, so a record is already in
/// the file by the time the event returns. This adds the operating-system half
/// of that guarantee: a run that is about to end — cleanly, by panic, or
/// because something outside it said so — calls this so its final records
/// survive a death that never reaches a normal exit path.
///
/// A run with logging disabled, or one whose log file could not be opened, has
/// nothing to flush and does nothing here.
pub fn flush() {
    let Some(file) = LOG_FILE.get() else {
        return;
    };
    let mut handle: &File = file.as_ref();
    let _ = handle.flush();
    let _ = handle.sync_all();
}

fn write_log_open_warning(path: &std::path::Path) {
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    let _ = writeln!(
        handle,
        "Warning: Could not open log file: {}",
        path.display()
    );
}
