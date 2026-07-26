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
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
    CONTAINED.with(|flag| flag.set(restore));
    result.map_err(|payload| describe(&payload, LOCATION.with(|slot| slot.borrow_mut().take())))
}

/// Render a panic payload and its recorded location as one diagnostic line.
fn describe(payload: &Box<dyn std::any::Any + Send>, location: Option<String>) -> String {
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
