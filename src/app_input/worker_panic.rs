//! Panic containment for recoverable background workers (issue #437).
//!
//! A panic inside a background GitHub worker is recoverable: the request is
//! abandoned, but the application keeps running. The default panic hook writes
//! the payload and backtrace to stderr, which is the same terminal the TUI is
//! drawing on, so the report is torn across the interface and lost on the next
//! render.
//!
//! [`contain`] runs a closure with the payload captured instead of printed, so
//! the caller can route it to the errors screen. Containment is scoped to the
//! calling thread for the duration of that closure: panics anywhere else keep
//! the previously installed hook and stay loud.

use std::cell::{Cell, RefCell};
use std::sync::Once;

thread_local! {
    /// Whether this thread is currently inside a [`contain`] boundary.
    static CONTAINED: Cell<bool> = const { Cell::new(false) };
    /// Source location recorded by the hook for the active containment.
    static LOCATION: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Install the containment-aware panic hook exactly once per process.
///
/// The hook delegates to the previously installed hook for every panic that is
/// not inside a [`contain`] boundary, so uncontained panics keep their normal
/// diagnostics.
fn install_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if CONTAINED.with(Cell::get) {
                LOCATION.with(|slot| {
                    *slot.borrow_mut() = info.location().map(ToString::to_string);
                });
                return;
            }
            previous(info);
        }));
    });
}

/// Run `work`, returning its panic payload as an error instead of letting the
/// default hook print it over the terminal UI.
///
/// The returned message includes the panic's source location when the runtime
/// reports one, so the errors screen shows a copyable report.
pub(super) fn contain<T>(work: impl FnOnce() -> T) -> Result<T, String> {
    install_hook();
    let restore = CONTAINED.with(|flag| flag.replace(true));
    // Drop any location left by an inner boundary or by a panic the work
    // caught itself, so a stale site is never attributed to this payload.
    LOCATION.with(|slot| slot.borrow_mut().take());
    // `AssertUnwindSafe` is sound here because `work` is consumed by this call
    // and nothing it captures is observable afterwards: on unwind the captures
    // are dropped with the closure, the payload is converted to a `String`, and
    // no borrow crosses the boundary. Callers pass owned request data, so there
    // is no shared mutable state that could be left half-updated.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    CONTAINED.with(|flag| flag.set(restore));
    let location = LOCATION.with(|slot| slot.borrow_mut().take());
    result.map_err(|payload| describe(&*payload, location))
}

/// Render a panic payload and its recorded location as one diagnostic line.
fn describe(payload: &dyn std::any::Any, location: Option<String>) -> String {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
        .unwrap_or("unknown panic");
    match location {
        Some(location) => format!("{message} (at {location})"),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    #[test]
    fn successful_work_returns_its_value() {
        let result = contain(|| 7_u32);
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn panic_payload_is_returned_with_its_source_location() {
        let error = contain(|| panic!("worker exploded"))
            .expect_err("a panicking worker must report an error");
        assert!(
            error.starts_with("worker exploded (at "),
            "message and location must be reported: {error}"
        );
        assert!(
            error.contains("worker_panic.rs"),
            "location must identify the panic site: {error}"
        );
    }

    #[test]
    fn static_str_payloads_are_reported() {
        let Err(error) = contain(|| -> () { std::panic::panic_any("static payload") }) else {
            panic!("a panicking worker must report an error");
        };
        assert!(
            error.starts_with("static payload"),
            "static payloads must be reported: {error}"
        );
    }

    #[test]
    fn unknown_payloads_report_a_placeholder() {
        let Err(error) = contain(|| -> () { std::panic::panic_any(42_u8) }) else {
            panic!("a panicking worker must report an error");
        };
        assert!(
            error.starts_with("unknown panic"),
            "unrecognized payloads must still report: {error}"
        );
    }

    /// Marks the re-executed child of the hook-delegation test.
    const HOOK_CHILD_VAR: &str = "JEFE_WORKER_PANIC_HOOK_CHILD";

    /// The point of the hook is that a contained panic never reaches the
    /// terminal while an uncontained one keeps its normal reporting.
    ///
    /// The hook is process-global and installed once, so this runs in a fresh
    /// child process where a sentinel can be installed as the delegate before
    /// any containment exists. Observing delegation is the only way to prove
    /// the panic text is actually withheld from the terminal.
    #[test]
    fn contained_panics_are_silent_and_uncontained_panics_still_report() {
        if std::env::var_os(HOOK_CHILD_VAR).is_some() {
            run_hook_delegation_child();
            return;
        }

        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => panic!("the test executable path must resolve: {error}"),
        };
        let output = std::process::Command::new(executable)
            .args([
                "--exact",
                "app_input::worker_panic::tests::contained_panics_are_silent_and_uncontained_panics_still_report",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HOOK_CHILD_VAR, "1")
            .output();
        let Ok(output) = output else {
            panic!("the child test process must start");
        };
        assert!(
            output.status.success(),
            "child hook assertions failed:
stdout:
{}
stderr:
{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Child half of [`contained_panics_are_silent_and_uncontained_panics_still_report`].
    fn run_hook_delegation_child() {
        static SENTINEL_CALLS: AtomicUsize = AtomicUsize::new(0);

        // Install the sentinel first so our wrapper adopts it as the delegate.
        std::panic::set_hook(Box::new(|_info| {
            SENTINEL_CALLS.fetch_add(1, Ordering::SeqCst);
        }));
        install_hook();

        let contained = contain(|| panic!("contained"));
        let after_contained = SENTINEL_CALLS.load(Ordering::SeqCst);

        let uncontained =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panic!("uncontained")));
        let after_uncontained = SENTINEL_CALLS.load(Ordering::SeqCst);

        assert!(contained.is_err(), "the contained panic must be captured");
        assert_eq!(
            after_contained, 0,
            "a contained panic must not reach the previous hook"
        );
        assert!(
            uncontained.is_err(),
            "the uncontained panic must still unwind"
        );
        assert_eq!(
            after_uncontained, 1,
            "an uncontained panic must still be reported by the previous hook"
        );
    }

    /// Each worker thread records its own location, so simultaneous panics
    /// cannot be attributed to each other's source site.
    #[test]
    fn concurrent_containment_keeps_locations_independent() {
        let barrier = Arc::new(Barrier::new(2));
        let first = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                contain(|| panic!("first thread"))
            })
        };
        let second = {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                contain(|| panic!("second thread"))
            })
        };

        let (Ok(Err(first)), Ok(Err(second))) = (first.join(), second.join()) else {
            panic!("both threads must report a contained panic");
        };
        assert!(
            first.starts_with("first thread (at ") && first.contains("worker_panic.rs"),
            "the first thread must keep its own message and location: {first}"
        );
        assert!(
            second.starts_with("second thread (at ") && second.contains("worker_panic.rs"),
            "the second thread must keep its own message and location: {second}"
        );
    }

    /// A panic the work catches itself must not leak its location onto a later
    /// boundary. A resumed payload never re-enters the hook, so without
    /// clearing the slot the earlier site would be reported as this one's.
    #[test]
    fn a_location_from_earlier_work_is_not_reused() {
        let Err(payload) = std::panic::catch_unwind(|| panic!("swallowed by earlier work"));

        let error = contain(|| std::panic::resume_unwind(payload))
            .expect_err("the resumed payload must still be contained");
        assert_eq!(
            error, "swallowed by earlier work",
            "a resumed payload must not inherit an earlier location: {error}"
        );
    }

    /// When work swallows a panic and then panics again, the reported location
    /// must be the site that actually escaped.
    #[test]
    fn the_reported_location_is_the_escaping_panic() {
        let error = contain(|| {
            let _ = std::panic::catch_unwind(|| panic!("swallowed inside the worker"));
            panic!("escaped the worker")
        })
        .expect_err("the escaping payload must be contained");
        assert!(
            error.starts_with("escaped the worker (at "),
            "the escaping panic must be reported: {error}"
        );
    }

    /// Containment must not leak to later work on the same thread, otherwise a
    /// subsequent unrelated panic would be silently swallowed.
    #[test]
    fn containment_is_scoped_to_the_closure() {
        let _ = contain(|| panic!("first"));
        assert!(
            !CONTAINED.with(Cell::get),
            "containment must be cleared after the closure returns"
        );
    }

    #[test]
    fn nested_containment_restores_the_outer_boundary() {
        let outer = contain(|| {
            let inner = contain(|| panic!("inner"));
            assert!(inner.is_err(), "inner panic must be contained");
            assert!(
                CONTAINED.with(Cell::get),
                "the outer boundary must still be active"
            );
            "outer completed"
        });
        assert_eq!(outer, Ok("outer completed"));
    }
}
