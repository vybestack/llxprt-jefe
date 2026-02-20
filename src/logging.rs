//! Application logging setup using `tracing` + `tracing-subscriber`.
//!
//! Controlled by two environment variables:
//! - `JEFE_LOG_FILE` — path to the log file. If unset, logging is disabled.
//! - `JEFE_LOG` — filter directive (e.g. `debug`, `jefe=trace`).
//!   Defaults to `info,jefe=debug` when omitted.

use std::fs::{self, OpenOptions};
use std::path::PathBuf;

use tracing_subscriber::EnvFilter;

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
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Warning: Could not open log file: {}", path.display());
        }
        return;
    };

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
