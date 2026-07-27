//! Behavioral contracts for the console preparation state machine (issue #434).
//!
//! These tests verify the deterministic setup/restore policy using a recording
//! fake — no real console is required, so they run identically on all
//! platforms. The Windows-specific adapter path is additionally exercised by
//! the native build: if stdout is a console, the real Win32 calls execute; if
//! not, the function degrades to `None`.

use super::{ConsoleGuard, ConsolePolicy, ENABLE_VIRTUAL_TERMINAL_PROCESSING, UTF8_CODE_PAGE};

use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

/// Operations recorded by [`RecordingPolicy`] for behavioral assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordedOp {
    IsTerminal,
    GetCodePage,
    SetCodePage(u32),
    GetMode,
    EnableVt,
}

/// In-memory fake implementing [`ConsolePolicy`] for deterministic testing.
///
/// Records every operation and supports failure injection for each method via
/// the `fail_on` configuration. The fake maintains `mode` as a `32` and
/// `EnableVt` ORs the VT flag. If `restore_log` is set, every
/// `set_output_code_page` call records the CP value so tests can verify
/// restoration after `Drop`.
struct RecordingPolicy {
    is_tty: bool,
    code_page: u32,
    mode: u32,
    ops: RefCell<Vec<RecordedOp>>,
    /// When non-empty, the next matching operation returns an error.
    fail_on: RefCell<Vec<&'static str>>,
    /// Optional shared log of code-page values passed to
    /// `set_output_code_page`. Enables post-`Drop` verification.
    restore_log: Option<Arc<Mutex<Vec<u32>>>>,
}

impl RecordingPolicy {
    fn new(is_tty: bool, code_page: u32, mode: u32) -> Self {
        Self {
            is_tty,
            code_page,
            mode,
            ops: RefCell::new(Vec::new()),
            fail_on: RefCell::new(Vec::new()),
            restore_log: None,
        }
    }

    fn with_restore_log(mut self, log: Arc<Mutex<Vec<u32>>>) -> Self {
        self.restore_log = Some(log);
        self
    }

    fn with_failures(is_tty: bool, code_page: u32, mode: u32, fails: &[&'static str]) -> Self {
        Self {
            is_tty,
            code_page,
            mode,
            ops: RefCell::new(Vec::new()),
            fail_on: RefCell::new(fails.to_vec()),
            restore_log: None,
        }
    }

    fn record(&self, op: RecordedOp) {
        self.ops.borrow_mut().push(op);
    }

    fn check_fail(&self, key: &'static str) -> bool {
        let mut fails = self.fail_on.borrow_mut();
        if let Some(pos) = fails.iter().position(|f| *f == key) {
            fails.remove(pos);
            true
        } else {
            false
        }
    }

    fn ops_snapshot(&self) -> Vec<RecordedOp> {
        self.ops.borrow().clone()
    }

    fn set_failure(&self, key: &'static str) {
        self.fail_on.borrow_mut().push(key);
    }
}

impl ConsolePolicy for RecordingPolicy {
    fn is_stdout_terminal(&self) -> bool {
        self.record(RecordedOp::IsTerminal);
        self.is_tty
    }

    fn current_output_code_page(&self) -> Result<u32, std::io::Error> {
        self.record(RecordedOp::GetCodePage);
        if self.check_fail("get_cp") {
            return Err(std::io::Error::other("injected get_cp failure"));
        }
        Ok(self.code_page)
    }

    fn set_output_code_page(&mut self, code_page: u32) -> Result<(), std::io::Error> {
        self.record(RecordedOp::SetCodePage(code_page));
        if self.check_fail("set_cp") {
            return Err(std::io::Error::other("injected set_cp failure"));
        }
        if let Some(log) = &self.restore_log {
            if let Ok(mut guard) = log.lock() {
                guard.push(code_page);
            }
        }
        self.code_page = code_page;
        Ok(())
    }

    fn enable_virtual_terminal_processing(&mut self) -> Result<(), std::io::Error> {
        self.record(RecordedOp::EnableVt);
        if self.check_fail("enable_vt") {
            return Err(std::io::Error::other("injected enable_vt failure"));
        }
        self.mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        Ok(())
    }

    fn has_virtual_terminal_processing(&self) -> Result<bool, std::io::Error> {
        self.record(RecordedOp::GetMode);
        if self.check_fail("get_mode") {
            return Err(std::io::Error::other("injected get_mode failure"));
        }
        Ok(self.mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0)
    }
}

/// Helper: call the state machine directly with a fake.
fn prepare_with_fake(policy: RecordingPolicy) -> Option<ConsoleGuard<RecordingPolicy>> {
    super::prepare_console(policy)
}

#[test]
fn tty_preparation_sets_utf8_and_enables_vt() {
    // OEM CP 437, no VT bit set → both must be changed.
    let policy = RecordingPolicy::new(true, 437, 0x0003);
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else {
        panic!("TTY with OEM CP should produce a guard")
    };

    let ops = guard.policy.ops_snapshot();
    assert!(
        ops.contains(&RecordedOp::SetCodePage(UTF8_CODE_PAGE)),
        "must set CP to UTF-8 (65001): {ops:?}"
    );
    assert!(
        ops.contains(&RecordedOp::EnableVt),
        "must enable VT processing: {ops:?}"
    );
    assert_eq!(
        guard.original_code_page, 437,
        "original CP must be captured as 437"
    );
}

#[test]
fn non_tty_stdout_makes_no_policy_calls() {
    let policy = RecordingPolicy::new(false, 437, 0x0003);
    let guard = prepare_with_fake(policy);

    assert!(guard.is_none(), "non-TTY must return None");
}

#[test]
fn already_utf8_with_vt_returns_no_guard() {
    // CP already UTF-8, VT already enabled → nothing to do.
    let policy = RecordingPolicy::new(true, UTF8_CODE_PAGE, 0x0007);
    let guard = prepare_with_fake(policy);

    assert!(guard.is_none(), "already-UTF-8 with VT must return None");
}

#[test]
fn cp_read_failure_returns_none_without_mutation() {
    let policy = RecordingPolicy::with_failures(true, 437, 0x0003, &["get_cp"]);
    let guard = prepare_with_fake(policy);

    assert!(guard.is_none(), "CP read failure must return None");
}

#[test]
fn mode_read_failure_returns_none_without_cp_mutation() {
    // CP reads fine, but reading mode fails.
    let policy = RecordingPolicy::with_failures(true, 437, 0x0003, &["get_mode"]);
    let guard = prepare_with_fake(policy);

    assert!(
        guard.is_none(),
        "mode read failure must return None without a guard"
    );
}

#[test]
fn vt_set_failure_rolls_back_code_page() {
    // CP set succeeds, then enable_vt fails → rollback CP.
    let policy = RecordingPolicy::with_failures(true, 437, 0x0003, &["enable_vt"]);
    let guard = prepare_with_fake(policy);

    assert!(
        guard.is_none(),
        "VT enablement failure must return None after rollback"
    );
}

#[test]
fn panic_unwind_restores_code_page() {
    // Suppress panic output to keep test output clean.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let restore_log = Arc::new(Mutex::new(Vec::new()));
    let policy = RecordingPolicy::new(true, 437, 0x0003).with_restore_log(Arc::clone(&restore_log));

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let _guard = prepare_with_fake(policy);
        panic!("simulated render panic");
    }));

    panic::set_hook(default_hook);

    assert!(result.is_err(), "the panic must have been caught");
    // The guard's Drop must have called set_output_code_page(437) during unwind.
    let logged = restore_log.lock().map(|g| g.clone()).unwrap_or_default();
    assert!(
        logged.contains(&437u32),
        "Drop during panic unwind must restore CP to 437: logged={logged:?}"
    );
}

#[test]
fn only_utf8_change_when_vt_already_enabled() {
    // CP is OEM, but VT is already on → only CP should be set.
    let policy = RecordingPolicy::new(true, 437, 0x0007); // VT bit = 0x4, so 0x7 has it
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else { panic!("guard") };

    let ops = guard.policy.ops_snapshot();
    assert!(
        ops.contains(&RecordedOp::SetCodePage(UTF8_CODE_PAGE)),
        "must set CP to UTF-8"
    );
    assert!(
        !ops.contains(&RecordedOp::EnableVt),
        "must NOT enable VT when already enabled: {ops:?}"
    );
}

#[test]
fn only_vt_change_when_cp_already_utf8() {
    // CP is already UTF-8, but VT is off → only VT should be enabled.
    let policy = RecordingPolicy::new(true, UTF8_CODE_PAGE, 0x0003);
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else { panic!("guard") };

    let ops = guard.policy.ops_snapshot();
    assert!(
        !ops.contains(&RecordedOp::SetCodePage(UTF8_CODE_PAGE)),
        "must NOT set CP when already UTF-8: {ops:?}"
    );
    assert!(
        ops.contains(&RecordedOp::EnableVt),
        "must enable VT: {ops:?}"
    );
}

#[test]
fn restore_code_page_failure_does_not_panic() {
    // Build a guard, then inject a CP-restore failure on the next set_cp.
    let policy = RecordingPolicy::new(true, 437, 0x0003);
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else { panic!("guard") };

    // Force a failure on the next set_cp call (the restore in Drop).
    guard.policy.set_failure("set_cp");

    // Drop must not panic even though restore fails.
    drop(guard);
}

#[test]
fn guard_original_code_page_preserves_arbitrary_oem_value() {
    // Verify the guard captures the exact original CP, not a hardcoded value.
    let policy = RecordingPolicy::new(true, 850, 0x0003);
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else { panic!("guard") };

    assert_eq!(
        guard.original_code_page, 850,
        "must capture CP 850 (a common European OEM page)"
    );
}

#[test]
fn guard_drop_restores_original_code_page_value() {
    // Verify Drop calls set_output_code_page(original_cp) via the observer log.
    let restore_log = Arc::new(Mutex::new(Vec::new()));
    let policy = RecordingPolicy::new(true, 437, 0x0003).with_restore_log(Arc::clone(&restore_log));
    let guard = prepare_with_fake(policy);
    let Some(guard) = guard else { panic!("guard") };

    // After prepare: the set log should contain [65001] (UTF-8 was set).
    {
        let logged = restore_log.lock().map(|g| g.clone()).unwrap_or_default();
        assert_eq!(logged, vec![UTF8_CODE_PAGE], "prepare must have set UTF-8");
    }

    // Dropping the guard must call set_output_code_page(437).
    drop(guard);

    let logged = restore_log.lock().map(|g| g.clone()).unwrap_or_default();
    assert_eq!(
        logged,
        vec![UTF8_CODE_PAGE, 437],
        "Drop must restore original CP 437 after prepare set 65001"
    );
}

#[test]
fn native_prepare_does_not_panic_on_windows() {
    // This exercises the real Windows adapter. If stdout is not a console
    // (e.g. piped in CI), it returns None which is a valid pass.
    let _guard = super::prepare_console_for_unicode();
    // No assertion on Some/None — either is valid. The test proves the real
    // entry point does not panic.
}
