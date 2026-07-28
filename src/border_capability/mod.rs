//! Rounded-border font-capability detection (issue #497).
//!
//! On Windows, some console host/font combinations have glyphs for the
//! `BorderStyle::Double` box-drawing set (`╔ ╗ ╚ ╝`, U+2550–2557) but lack the
//! `BorderStyle::Round` set (`╭ ╮ ╰ ╯`, U+256D–2570). When that happens, every
//! unfocused pane border (resolved via `if focused { Double } else { Round }`)
//! renders as `?`, because the UTF-8 code page (set by `terminal_init` for
//! issue #434) only tells the console how to *decode* bytes — it cannot give
//! the font a glyph it lacks.
//!
//! This module detects whether the console can represent the rounded-corner
//! glyphs and, if not, exposes a fallback so the unfocused border style
//! becomes `Single` (`┌ ┐ └ ┘`, U+250C–2510) — a sub-range even raster fonts
//! typically cover. The detection runs once at startup, after
//! `terminal_init::prepare_console_for_unicode` has set the output code page
//! to UTF-8.
//!
//! The module separates a deterministic, platform-agnostic state machine
//! (`BorderCapabilityPolicy` trait + `detect_capability` + the global
//! `OnceLock<Capability>`) from the Windows-specific adapter so the full
//! detect/fallback contract is unit-testable without a real console.
//!
//! Fail-safe policy: every uncertain path (non-TTY, probe error, non-Windows)
//! yields `Capability::RoundSupported`, so a Unicode-rich terminal is never
//! regressed into uglier borders by a misdetection.

use iocraft::prelude::BorderStyle;
use std::sync::atomic::{AtomicU8, Ordering};

/// A rounded-corner glyph actually emitted by iocraft's `BorderStyle::Round`
/// (`vendor/iocraft/src/components/box.rs:83-92`). The probe round-trips this
/// codepoint to detect font coverage of the U+256D–2570 sub-range.
pub(crate) const ROUND_CORNER_SAMPLE: char = '╭';

/// Whether the console font can represent the rounded-corner glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The font has the rounded-corner glyphs; `Round` renders correctly.
    RoundSupported,
    /// The font lacks the rounded-corner glyphs; fall back to `Single`.
    RoundUnsupported,
}

impl Capability {
    /// The unfocused border style for this capability: `Round` when supported,
    /// `Single` when not.
    #[must_use]
    pub const fn unfocused_border_style(self) -> BorderStyle {
        match self {
            Self::RoundSupported => BorderStyle::Round,
            Self::RoundUnsupported => BorderStyle::Single,
        }
    }

    /// Encode the capability as an atomic discriminant for lock-free storage.
    const fn to_u8(self) -> u8 {
        match self {
            Self::RoundSupported => 0,
            Self::RoundUnsupported => 1,
        }
    }

    /// Decode the atomic discriminant back into a `Capability`.
    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::RoundUnsupported,
            _ => Self::RoundSupported,
        }
    }
}

/// Abstracts the console operations used by the detection state machine.
///
/// The trait exists so the deterministic detect/fallback contract is
/// unit-testable with a recording fake, and so the Windows adapter is the
/// single owner of every `win32console`/`winsafe` call.
pub(crate) trait BorderCapabilityPolicy {
    /// Whether stdout is attached to a terminal (TTY).
    fn is_stdout_terminal(&self) -> bool;

    /// Probe whether the console font can represent the rounded-corner glyph
    /// [`ROUND_CORNER_SAMPLE`].
    ///
    /// Returns `Ok(true)` when the glyph is supported, `Ok(false)` when it is
    /// not, and `Err` when the probe itself could not run (e.g. the console
    /// handle is unavailable). The caller treats `Err` as fail-safe
    /// "supported" so a transient probe failure never regresses the border
    /// style.
    fn probe_round_corner(&self) -> std::io::Result<bool>;
}

/// Detect the console's rounded-corner capability using the given policy.
///
/// Deterministic state machine entry point:
///
/// 1. Non-TTY → `RoundSupported` without probing (piped/redirected output is
///    irrelevant; the flag is irrelevant outside the TUI and must not mutate a
///    console that is not a terminal).
/// 2. TTY → probe once. `Ok(true)` → `RoundSupported`; `Ok(false)` →
///    `RoundUnsupported`; `Err` → `RoundSupported` (fail safe).
pub(crate) fn detect_capability(policy: &dyn BorderCapabilityPolicy) -> Capability {
    if !policy.is_stdout_terminal() {
        return Capability::RoundSupported;
    }
    match policy.probe_round_corner() {
        Ok(supported) => {
            if supported {
                Capability::RoundSupported
            } else {
                Capability::RoundUnsupported
            }
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "rounded-corner font probe failed; assuming supported (Round borders)"
            );
            Capability::RoundSupported
        }
    }
}

// ---------------------------------------------------------------------------
// Global capability flag and public resolution helper
// ---------------------------------------------------------------------------

/// Process-global capability flag, stored as an atomic discriminant so it can
/// be (re)set by tests. Defaults to `RoundSupported` (the historical rendering
/// on Unix and on Unicode-rich Windows Terminal hosts) before [`initialize`]
/// runs.
static CAPABILITY: AtomicU8 = AtomicU8::new(0);

/// The default capability assumed before [`initialize`] runs, or on platforms
/// where detection is unavailable. `RoundSupported` preserves the historical
/// rendering on Unix and on Unicode-rich Windows Terminal hosts.
///
/// Referenced by the non-Windows `detect_and_initialize` stub; on Windows the
/// real adapter determines the capability at runtime, so the constant is only
/// used off that path.
#[cfg_attr(windows, allow(dead_code))]
const DEFAULT_CAPABILITY: Capability = Capability::RoundSupported;

/// Set the process-global capability flag. Called once at startup after
/// `terminal_init::prepare_console_for_unicode`.
pub fn initialize(capability: Capability) {
    CAPABILITY.store(capability.to_u8(), Ordering::Relaxed);
}

/// Read the process-global capability flag, defaulting to
/// [`DEFAULT_CAPABILITY`] when [`initialize`] has not run.
#[must_use]
pub fn capability() -> Capability {
    Capability::from_u8(CAPABILITY.load(Ordering::Relaxed))
}

/// The unfocused pane border style, honoring the detected font capability.
///
/// All border-resolution sites (`selectable_list`, `terminal_view`, `preview`,
/// `detail_pane`) call this instead of hard-coding `BorderStyle::Round`, so the
/// fallback is applied uniformly when the console font lacks rounded corners.
#[must_use]
pub fn resolve_unfocused_border_style() -> BorderStyle {
    capability().unfocused_border_style()
}

// ---------------------------------------------------------------------------
// Platform-specific detection entry point
// ---------------------------------------------------------------------------

/// Non-Windows: terminal output is natively UTF-8 with a font that ships the
/// full box-drawing range, so the capability is always `RoundSupported`. The
/// function still records it via [`initialize`] so the flag is in a known
/// state regardless of platform.
#[cfg(not(windows))]
pub fn detect_and_initialize() {
    initialize(DEFAULT_CAPABILITY);
}

#[cfg(windows)]
mod windows_adapter {
    use super::{BorderCapabilityPolicy, ROUND_CORNER_SAMPLE, detect_capability};

    use std::io;

    use win32console::console::WinConsole;
    use win32console::structs::char_info::CharInfo;
    use win32console::structs::coord::Coord;
    use win32console::structs::small_rect::SmallRect;

    /// Windows adapter implementing [`BorderCapabilityPolicy`].
    ///
    /// The probe writes [`ROUND_CORNER_SAMPLE`] to a 1×1 scratch cell at the
    /// bottom-right of the visible window via `WriteConsoleOutputW`, reads it
    /// back via `ReadConsoleOutputW`, and compares the stored `char`. If the
    /// console/font cannot represent the codepoint, the round-trip differs and
    /// the probe reports unsupported. All Win32 FFI stays inside the safe
    /// `win32console` wrappers; jefe source contains no `unsafe`.
    struct Win32Policy;

    impl BorderCapabilityPolicy for Win32Policy {
        fn is_stdout_terminal(&self) -> bool {
            use std::io::IsTerminal;
            std::io::stdout().is_terminal()
        }

        fn probe_round_corner(&self) -> io::Result<bool> {
            let console = WinConsole::current_output();
            // Read the original content of the scratch cell so it can be
            // restored after the probe (avoid leaving a stray glyph on screen).
            let info = console.get_screen_buffer_info()?;
            let scratch_x = info.window.right;
            let scratch_y = info.window.bottom;
            let mut restore_region = SmallRect {
                left: scratch_x,
                top: scratch_y,
                right: scratch_x,
                bottom: scratch_y,
            };
            let original = console.read_output(
                Coord { x: 1, y: 1 },
                Coord { x: 0, y: 0 },
                &mut restore_region,
            )?;

            // Write the sample glyph to the scratch cell.
            let probe_cell = CharInfo {
                char_value: ROUND_CORNER_SAMPLE,
                attributes: 0,
            };
            let write_region = SmallRect {
                left: scratch_x,
                top: scratch_y,
                right: scratch_x,
                bottom: scratch_y,
            };
            console.write_output(
                &[probe_cell],
                Coord { x: 1, y: 1 },
                Coord { x: 0, y: 0 },
                write_region,
            )?;

            // Read it back and compare.
            let mut read_region = SmallRect {
                left: scratch_x,
                top: scratch_y,
                right: scratch_x,
                bottom: scratch_y,
            };
            let read_back = console.read_output(
                Coord { x: 1, y: 1 },
                Coord { x: 0, y: 0 },
                &mut read_region,
            )?;

            // Restore the original cell content (best-effort).
            if let Some(cell) = original.first() {
                let _ = console.write_output(
                    &[CharInfo {
                        char_value: cell.char_value,
                        attributes: cell.attributes,
                    }],
                    Coord { x: 1, y: 1 },
                    Coord { x: 0, y: 0 },
                    write_region,
                );
            }

            let supported = read_back
                .first()
                .is_some_and(|cell| cell.char_value == ROUND_CORNER_SAMPLE);
            Ok(supported)
        }
    }

    /// Detect the Windows console's rounded-corner capability.
    ///
    /// Records the result in the global flag. Called once at startup after
    /// `terminal_init::prepare_console_for_unicode` has set the output code
    /// page to UTF-8. Fail-safe: any probe error defaults to `RoundSupported`.
    pub fn detect_and_initialize() {
        let capability = detect_capability(&Win32Policy);
        super::initialize(capability);
    }
}

#[cfg(windows)]
pub use windows_adapter::detect_and_initialize;

/// Test-only setter for the global capability flag. Used by unit tests to
/// exercise `resolve_unfocused_border_style` across both states.
#[cfg(test)]
pub(crate) fn set_capability(capability: Capability) {
    CAPABILITY.store(capability.to_u8(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
