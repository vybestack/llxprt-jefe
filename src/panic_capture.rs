//! Process-wide panic diagnostics for the terminal application.
//!
//! The hook never touches terminal streams or application state. It records a
//! bounded diagnostic for the executor-owned render loop to drain.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, Once};

use jefe::domain::{ERROR_STORE_CAPACITY, ErrorSource};
use jefe::messages::ErrorsMessage;

static INSTALL: Once = Once::new();
static REPORTS: Mutex<VecDeque<PanicReport>> = Mutex::new(VecDeque::new());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicReport {
    pub message: String,
    pub location: Option<String>,
    pub thread: String,
    timestamp: String,
}

pub fn install_panic_hook() {
    INSTALL.call_once(|| {
        std::panic::set_hook(Box::new(|info| {
            let message = info
                .payload()
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    info.payload()
                        .downcast_ref::<&'static str>()
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| "unknown panic".to_owned());
            capture_panic(message, info.location().map(ToString::to_string));
        }));
    });
}

fn capture_panic(message: String, location: Option<String>) {
    let thread = thread_label();
    let timestamp = timestamp();
    tracing::error!(
        target: "jefe::panic",
        panic_message = %message,
        panic_location = ?location,
        panic_thread = %thread,
        "thread panicked"
    );
    // A panic is exactly the moment the tail of the log matters most, and the
    // process may not survive long enough to write it later (issue #662).
    jefe::logging::flush();
    let mut reports = reports_guard();
    if reports.len() == ERROR_STORE_CAPACITY {
        reports.pop_front();
    }
    reports.push_back(PanicReport {
        message,
        location,
        thread,
        timestamp,
    });
}

fn thread_label() -> String {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    format!("{name} ({:?})", thread.id())
}

fn timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "0".to_owned(),
            |duration| duration.as_secs().to_string(),
        )
}

fn reports_guard() -> MutexGuard<'static, VecDeque<PanicReport>> {
    match REPORTS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn drain_panic_reports() -> Vec<PanicReport> {
    std::mem::take(&mut *reports_guard()).into()
}

impl PanicReport {
    pub fn into_errors_message(self) -> ErrorsMessage {
        let detail = report_detail(&self);
        ErrorsMessage::CaptureSilent {
            title: format!("Panic on {}", self.thread),
            detail,
            source: ErrorSource::Panic,
            timestamp: self.timestamp,
        }
    }
}

fn report_detail(report: &PanicReport) -> String {
    report.location.as_ref().map_or_else(
        || format!("{}\nThread: {}", report.message, report.thread),
        |location| {
            format!(
                "{}\nThread: {}\nLocation: {location}",
                report.message, report.thread
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHILD_ENV: &str = "JEFE_PANIC_CAPTURE_CHILD";
    const PANIC_MARKER: &str = "issue-496-blocking-panic";

    #[test]
    fn blocking_pool_panic_is_logged_and_captured_without_stderr() {
        if std::env::var_os(CHILD_ENV).is_some() {
            run_blocking_pool_child();
            return;
        }

        let executable = std::env::current_exe()
            .unwrap_or_else(|error| panic!("test executable path must resolve: {error}"));
        let log_path = std::env::temp_dir().join(format!(
            "jefe-panic-capture-{}-{}.log",
            std::process::id(),
            unique_suffix()
        ));
        let output = std::process::Command::new(executable)
            .args([
                "--exact",
                "panic_capture::tests::blocking_pool_panic_is_logged_and_captured_without_stderr",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, "1")
            .env("JEFE_LOG_FILE", &log_path)
            .env("JEFE_LOG", "error")
            .output()
            .unwrap_or_else(|error| panic!("panic-capture child must start: {error}"));

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "child assertions failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            !stdout.contains(PANIC_MARKER),
            "global hook must not print panic payload to stdout: {stdout}"
        );
        assert!(
            !stderr.contains(PANIC_MARKER),
            "global hook must not print panic payload to stderr: {stderr}"
        );
        let log = std::fs::read_to_string(&log_path)
            .unwrap_or_else(|error| panic!("configured panic log must be readable: {error}"));
        assert!(
            log.contains(PANIC_MARKER),
            "configured log must contain the panic record: {log}"
        );
        let _ = std::fs::remove_file(log_path);
    }

    fn run_blocking_pool_child() {
        crate::init_diagnostics();
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            smol::block_on(smol::unblock(|| panic!("{PANIC_MARKER}")))
        }));
        assert!(unwind.is_err(), "the blocking worker must still unwind");

        let reports = drain_panic_reports();
        assert_eq!(reports.len(), 1, "the hook must enqueue exactly one report");
        let report = &reports[0];
        assert_eq!(report.message, PANIC_MARKER);
        assert!(
            !report.thread.is_empty(),
            "worker identity must be recorded"
        );
        assert!(report.location.is_some(), "panic location must be recorded");

        for index in 0..ERROR_STORE_CAPACITY + 5 {
            capture_panic(format!("queue-{index}"), None);
        }
        let reports = drain_panic_reports();
        assert_eq!(reports.len(), ERROR_STORE_CAPACITY);
        assert_eq!(
            reports.first().map(|report| report.message.as_str()),
            Some("queue-5")
        );
        assert_eq!(
            reports.last().map(|report| report.message.as_str()),
            Some("queue-54")
        );
        assert!(
            drain_panic_reports().is_empty(),
            "reports must drain exactly once"
        );

        let unknown = std::panic::catch_unwind(|| std::panic::panic_any(42_u8));
        assert!(unknown.is_err(), "non-string payload must still unwind");
        let reports = drain_panic_reports();
        assert_eq!(
            reports.first().map(|report| report.message.as_str()),
            Some("unknown panic")
        );
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    }
}
