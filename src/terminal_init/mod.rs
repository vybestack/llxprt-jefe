//! Windows console output preparation for Unicode TUI rendering (issue #434).
//!
//! On Windows, the console output code page defaults to a legacy OEM page
//! (e.g. 437/850), so the UTF-8 byte sequences jefe emits for box-drawing
//! borders, separators, and caret glyphs are decoded with the wrong code page
//! and render as `?`. This module sets the output code page to UTF-8 (65001)
//! and enables `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on the console output
//! handle immediately before jefe enters its render loop, then restores the
//! original code page when the returned guard is dropped.
//!
//! VT processing is intentionally left enabled after jefe exits: it is a
//! progressive enhancement that modern Windows Terminal enables by default,
//! and restoring the original mode would require capturing the full mode
//! bitmask (which cannot be safely round-tripped through the `ConsolePolicy`
//! trait without reconstructing `winsafe` bitflags from `u32`, requiring
//! `unsafe`). Only the code page is restored because that is the value users
//! notice if left modified.
//!
//! The guard restores state on normal return and during panic unwinding. It
//! cannot restore state after a hard abort (`std::process::abort`,
//! `std::process::exit`, forced process termination, or a `panic = "abort"`
//! panic) because those paths do not run destructors.
//!
//! The module separates a deterministic, platform-agnostic state machine
//! (`ConsolePolicy` trait + `prepare_console` + `ConsoleGuard`) from the
//! Windows-specific adapter (`WinsafePolicy`) so the full setup/restore
//! contract is unit-testable without a real console.

#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

/// Windows console output code page identifier for UTF-8.
const UTF8_CODE_PAGE: u32 = 65001;

/// `ENABLE_VIRTUAL_TERMINAL_PROCESSING` flag bit.
///
/// On Windows this is `0x0004`. The flag instructs the console to interpret
/// ANSI/VT escape sequences in the output stream. Modern Windows Terminal
/// enables it by default, but legacy `cmd.exe` on older Windows 10 builds may
/// not, so the adapter ORs it in as a belt-and-braces measure.
#[cfg_attr(not(test), allow(dead_code))]
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

/// Abstracts the console output operations used by the preparation state
/// machine.
///
/// This trait exists so the deterministic setup/restore contract can be
/// unit-tested with a recording fake, and so the Windows adapter is the single
/// owner of every `winsafe` / `win32console` call. The methods use `u32` for
/// code page values so the trait stays platform-agnostic. Mode manipulation is
/// abstracted at the semantic level (`enable_virtual_terminal_processing`,
/// `has_virtual_terminal_processing`) so the Windows adapter can use safe
/// `winsafe` bitflag types without reconstructing them from raw `u32` values
/// (which would require `unsafe`).
pub trait ConsolePolicy {
    /// Whether stdout is attached to a terminal (TTY).
    fn is_stdout_terminal(&self) -> bool;

    /// Reads the current console output code page.
    fn current_output_code_page(&self) -> io::Result<u32>;

    /// Sets the console output code page to the given value.
    fn set_output_code_page(&mut self, code_page: u32) -> io::Result<()>;

    /// Returns `true` if the VT flag is already set in the current output mode.
    fn has_virtual_terminal_processing(&self) -> io::Result<bool>;

    /// Enables virtual-terminal processing by OR-ing the VT flag into the
    /// current output mode.
    fn enable_virtual_terminal_processing(&mut self) -> io::Result<()>;
}

/// RAII guard that restores the original console output code page when
/// dropped.
///
/// VT processing is intentionally left enabled after restore because it is a
/// progressive enhancement that does not affect non-TUI shell usage.
///
/// Restoration is best-effort: if the code-page restore fails, a structured
/// `tracing::warn!` event is emitted so diagnostics surface without panicking.
pub struct PolicyGuard<P: ConsolePolicy> {
    policy: P,
    original_code_page: u32,
}

impl<P: ConsolePolicy> Drop for PolicyGuard<P> {
    fn drop(&mut self) {
        restore_code_page(&mut self.policy, self.original_code_page);
    }
}

/// Attempts to restore the code page, logging a warning on failure.
fn restore_code_page<P: ConsolePolicy>(policy: &mut P, code_page: u32) {
    if let Err(error) = policy.set_output_code_page(code_page) {
        tracing::warn!(
            error = %error,
            code_page,
            "failed to restore console output code page; the shell may retain UTF-8"
        );
    }
}

/// Prepares the console for Unicode output using the given policy.
///
/// This is the deterministic state machine entry point. It:
///
/// 1. Skips all operations if stdout is not a terminal (returns `None`).
/// 2. Reads the original code page and mode. On read failure, logs a warning
///    and returns `None` without mutation.
/// 3. If the code page is already UTF-8 and VT processing is already enabled,
///    returns `None` (nothing to do — avoids an unnecessary restore).
/// 4. Sets the code page to UTF-8. On failure, logs a warning and returns
///    `None`.
/// 5. Enables VT processing. On failure, rolls back the code page to its
///    original value (logging a separate warning if the rollback also fails),
///    then returns `None`.
/// 6. Returns a guard that restores the original code page on drop.
fn prepare_console<P: ConsolePolicy>(mut policy: P) -> Option<PolicyGuard<P>> {
    if !policy.is_stdout_terminal() {
        return None;
    }

    let original_cp = match policy.current_output_code_page() {
        Ok(cp) => cp,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to read console output code page; continuing without UTF-8 console setup"
            );
            return None;
        }
    };

    let original_has_vt = match policy.has_virtual_terminal_processing() {
        Ok(vt) => vt,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to read console output mode; continuing without VT setup"
            );
            return None;
        }
    };

    let needs_cp_change = original_cp != UTF8_CODE_PAGE;
    let needs_vt_change = !original_has_vt;

    if !needs_cp_change && !needs_vt_change {
        return None;
    }

    if needs_cp_change {
        if let Err(error) = policy.set_output_code_page(UTF8_CODE_PAGE) {
            tracing::warn!(
                error = %error,
                "failed to set console output code page to UTF-8; Unicode glyphs may render incorrectly"
            );
            return None;
        }
    }

    if needs_vt_change {
        if let Err(error) = policy.enable_virtual_terminal_processing() {
            tracing::warn!(
                error = %error,
                "failed to enable virtual terminal processing; attempting code page rollback"
            );
            if needs_cp_change {
                restore_code_page(&mut policy, original_cp);
            }
            return None;
        }
    }

    Some(PolicyGuard {
        policy,
        original_code_page: original_cp,
    })
}

// ---------------------------------------------------------------------------
// Platform-specific adapter and public entry point
// ---------------------------------------------------------------------------

/// Placeholder guard type for non-Windows platforms.
///
/// On Unix, terminal output is natively UTF-8, so no console preparation is
/// needed. The function returns `None` and this type exists only to provide a
/// uniform call-site signature. It implements `Drop` as a no-op so the public
/// `ConsoleGuard` alias has a consistent `Drop` bound on every platform.
#[cfg(not(windows))]
#[derive(Debug)]
pub struct NoOpGuard;

#[cfg(not(windows))]
impl Drop for NoOpGuard {
    fn drop(&mut self) {}
}

/// Unified console-preparation guard type returned by
/// [`prepare_console_for_unicode`].
///
/// On Windows this is the real code-page-restoring guard; on other platforms
/// it is the no-op [`NoOpGuard`]. Either way it implements `Drop`, so callers
/// can hold the value across the render loop without a platform-specific
/// `impl Drop` return bound (which cannot be satisfied when the non-Windows
/// stub returns a unit struct).
pub type ConsoleGuard = GuardInner;

#[cfg(windows)]
pub type GuardInner = PolicyGuard<self::windows_adapter::WinsafePolicy>;

#[cfg(not(windows))]
pub type GuardInner = NoOpGuard;

/// Prepares the console for Unicode TUI output.
///
/// On Windows, this sets the output code page to UTF-8 and enables VT
/// processing, returning a guard that restores the original state on drop.
/// On all other platforms, this is a no-op returning `None`.
///
/// The returned guard (if any) must be held alive for the duration of the
/// terminal render loop. Dropping it restores the console to its prior state.
#[cfg(not(windows))]
#[must_use]
pub fn prepare_console_for_unicode() -> Option<ConsoleGuard> {
    None
}

#[cfg(windows)]
mod windows_adapter {
    use super::{ConsolePolicy, UTF8_CODE_PAGE};

    use std::io;

    use win32console::console::WinConsole;
    use winsafe::{HSTD, co};

    /// Windows adapter implementing [`ConsolePolicy`] via safe `winsafe` and
    /// `win32console` wrappers.
    ///
    /// All Win32 FFI stays inside the safe wrappers; jefe source contains no
    /// `unsafe`.
    pub struct WinsafePolicy;

    impl WinsafePolicy {
        /// Opens the standard output console handle for mode queries/sets.
        ///
        /// The handle is leaked from its `CloseHandleGuard` so the real stdout
        /// handle is never closed by the guard's `Drop`.
        fn stdout_handle() -> io::Result<HSTD> {
            let mut guard = HSTD::GetStdHandle(co::STD_HANDLE::OUTPUT).map_err(sysresult_to_io)?;
            // Leak the handle to prevent CloseHandleGuard from closing the real
            // stdout handle when this temporary guard drops.
            let handle = guard.leak();
            if handle.ptr().is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "console output handle is null",
                ));
            }
            Ok(handle)
        }
    }

    impl ConsolePolicy for WinsafePolicy {
        fn is_stdout_terminal(&self) -> bool {
            use std::io::IsTerminal;
            std::io::stdout().is_terminal()
        }

        fn current_output_code_page(&self) -> io::Result<u32> {
            WinConsole::get_output_code_page()
        }

        fn set_output_code_page(&mut self, code_page: u32) -> io::Result<()> {
            WinConsole::set_output_code(code_page)
        }

        fn enable_virtual_terminal_processing(&mut self) -> io::Result<()> {
            let handle = Self::stdout_handle()?;
            let mode = handle.GetConsoleMode().map_err(sysresult_to_io)?;
            let new_mode = mode | co::CONSOLE::ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            handle.SetConsoleMode(new_mode).map_err(sysresult_to_io)
        }

        fn has_virtual_terminal_processing(&self) -> io::Result<bool> {
            let handle = Self::stdout_handle()?;
            let mode = handle.GetConsoleMode().map_err(sysresult_to_io)?;
            Ok((mode.raw() & co::CONSOLE::ENABLE_VIRTUAL_TERMINAL_PROCESSING.raw()) != 0)
        }
    }

    /// Converts a `winsafe` `co::ERROR` into an `io::Error`.
    ///
    /// Windows system error codes are small positive `u32` values that always
    /// fit in `i32`. If an unexpected out-of-range value appears, fall back to
    /// a generic error rather than wrapping.
    fn sysresult_to_io(error: co::ERROR) -> io::Error {
        match i32::try_from(error.raw()) {
            Ok(code) => io::Error::from_raw_os_error(code),
            Err(_) => io::Error::other(format!("winsafe error code: {}", error.raw())),
        }
    }

    /// Prepares the console for Unicode TUI output on Windows.
    ///
    /// Sets the output code page to UTF-8 (65001) and enables
    /// `ENABLE_VIRTUAL_TERMINAL_PROCESSING`, returning a guard that restores
    /// the original code page on drop (VT processing is left enabled). If
    /// stdout is not a terminal or setup fails, returns `None` and jefe
    /// continues without console modification.
    #[must_use]
    pub fn prepare_console_for_unicode() -> Option<super::ConsoleGuard> {
        super::prepare_console(WinsafePolicy)
    }

    // Keep the constant referenced on Windows so dead-code analysis is happy
    // even if the optimizer inlines its only use.
    const _: u32 = UTF8_CODE_PAGE;
}

#[cfg(windows)]
pub use windows_adapter::prepare_console_for_unicode;

#[cfg(test)]
mod tests;
