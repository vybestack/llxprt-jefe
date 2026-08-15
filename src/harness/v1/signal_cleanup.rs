//! Interruption tracking for the schema-1 real-PTY runner.
//!
//! The listener converts process signals into a runner-observed interruption.
//! Cleanup remains on the runner thread, where the run's exact process group
//! and contained application socket are available and reportable.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use super::error::HarnessError;

const HANDLED_SIGNALS: &[i32] = &[
    signal_hook::consts::SIGINT,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGHUP,
    signal_hook::consts::SIGQUIT,
];

/// Signal listener scoped to one schema-1 run.
#[derive(Debug)]
#[must_use]
pub struct SignalCleanupGuard {
    interrupted: Arc<AtomicI32>,
    handle: Option<signal_hook::iterator::Handle>,
}

impl SignalCleanupGuard {
    /// Install the process signal listener before scenario execution starts.
    ///
    /// # Errors
    ///
    /// Returns `HAR-E005` if registration or thread creation fails.
    pub fn new() -> Result<Self, HarnessError> {
        let mut signals = signal_hook::iterator::Signals::new(HANDLED_SIGNALS.iter().copied())
            .map_err(|err| HarnessError::process(format!("install signal cleanup: {err}")))?;
        let handle = signals.handle();
        let interrupted = Arc::new(AtomicI32::new(0));
        let thread_interrupted = Arc::clone(&interrupted);
        std::thread::Builder::new()
            .name("schema-1-signal-cleanup".to_string())
            .spawn(move || {
                if let Some(signal) = signals.forever().next() {
                    thread_interrupted.store(signal, Ordering::SeqCst);
                }
            })
            .map_err(|err| HarnessError::process(format!("spawn signal cleanup: {err}")))?;
        Ok(Self {
            interrupted,
            handle: Some(handle),
        })
    }

    /// Return the first signal received by this run, if any.
    #[must_use]
    pub fn interruption(&self) -> Option<HarnessError> {
        let signal = self.interrupted.load(Ordering::SeqCst);
        (signal != 0)
            .then(|| HarnessError::process(format!("interrupted by {}", signal_name(signal))))
    }
}

impl Drop for SignalCleanupGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.close();
        }
    }
}

fn signal_name(signal: i32) -> &'static str {
    match signal {
        signal_hook::consts::SIGINT => "SIGINT",
        signal_hook::consts::SIGTERM => "SIGTERM",
        signal_hook::consts::SIGHUP => "SIGHUP",
        signal_hook::consts::SIGQUIT => "SIGQUIT",
        _ => "unknown signal",
    }
}
