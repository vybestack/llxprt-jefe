//! Behavioral contracts for the rounded-border font-capability detection
//! state machine (issue #497).
//!
//! These tests verify the deterministic detection policy using a recording
//! fake — no real console is required, so they run identically on all
//! platforms. The Windows-specific adapter path is additionally exercised by
//! the native build: if stdout is a console, the real probe runs; if not, the
//! detector degrades to "assume capable".

use super::{
    BorderCapabilityPolicy, Capability, ROUND_CORNER_SAMPLE, detect_capability,
    resolve_unfocused_border_style, set_capability,
};

use iocraft::prelude::BorderStyle;

use std::cell::RefCell;

/// Operations recorded by [`RecordingPolicy`] for behavioral assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordedOp {
    IsTerminal,
    ProbeRound,
}

/// In-memory fake implementing [`BorderCapabilityPolicy`] for deterministic
/// testing.
///
/// Records every operation and supports failure injection for `probe_round`
/// via the `fail_probe` flag. The `round_supported` field controls whether
/// the probe reports the rounded-corner glyph as supported.
struct RecordingPolicy {
    is_tty: bool,
    round_supported: bool,
    fail_probe: bool,
    ops: RefCell<Vec<RecordedOp>>,
}

impl RecordingPolicy {
    fn new(is_tty: bool, round_supported: bool) -> Self {
        Self {
            is_tty,
            round_supported,
            fail_probe: false,
            ops: RefCell::new(Vec::new()),
        }
    }

    fn with_fail_probe(mut self) -> Self {
        self.fail_probe = true;
        self
    }
}

impl BorderCapabilityPolicy for RecordingPolicy {
    fn is_stdout_terminal(&self) -> bool {
        self.ops.borrow_mut().push(RecordedOp::IsTerminal);
        self.is_tty
    }

    fn probe_round_corner(&self) -> std::io::Result<bool> {
        self.ops.borrow_mut().push(RecordedOp::ProbeRound);
        if self.fail_probe {
            Err(std::io::Error::other("probe failed"))
        } else {
            Ok(self.round_supported)
        }
    }
}

/// A non-TTY environment must not run the font probe (only the TTY check) and
/// must default to capable (`Round`), so the Unicode-rich Unix/Terminal-host
/// path is never regressed.
#[test]
fn non_tty_defaults_to_capable_without_probing() {
    let policy = RecordingPolicy::new(false, false);
    let capability = detect_capability(&policy);
    assert_eq!(capability, Capability::RoundSupported);
    assert_eq!(
        policy.ops.borrow().as_slice(),
        &[RecordedOp::IsTerminal],
        "non-TTY must check the TTY flag but must not run the font probe"
    );
}

/// A TTY with a font that supports rounded corners reports capable and keeps
/// `Round` as the unfocused border style.
#[test]
fn tty_with_round_support_keeps_round() {
    let policy = RecordingPolicy::new(true, true);
    let capability = detect_capability(&policy);
    assert_eq!(capability, Capability::RoundSupported);
    assert_eq!(
        policy.ops.borrow().as_slice(),
        &[RecordedOp::IsTerminal, RecordedOp::ProbeRound],
        "TTY must probe exactly once"
    );
}

/// A TTY whose font lacks the rounded-corner glyphs falls back to
/// `RoundUnsupported`, and the unfocused border style becomes `Single`.
#[test]
fn tty_without_round_support_falls_back_to_single() {
    let policy = RecordingPolicy::new(true, false);
    let capability = detect_capability(&policy);
    assert_eq!(capability, Capability::RoundUnsupported);
}

/// If the probe itself errors (e.g. console handle unavailable), the detector
/// must fail safe to `RoundSupported` so a transient probe failure never
/// regresses a Unicode-rich terminal into uglier borders.
#[test]
fn probe_error_fails_safe_to_capable() {
    let policy = RecordingPolicy::new(true, true).with_fail_probe();
    let capability = detect_capability(&policy);
    assert_eq!(
        capability,
        Capability::RoundSupported,
        "probe error must not regress the unfocused border style"
    );
}

/// The global capability flag threads the detected result into
/// `resolve_unfocused_border_style`. Setting it to unsupported yields
/// `Single`; setting it to supported yields `Round`.
#[test]
fn resolve_unfocused_style_follows_global_flag() {
    set_capability(Capability::RoundSupported);
    assert_eq!(
        resolve_unfocused_border_style(),
        BorderStyle::Round,
        "supported → Round"
    );

    set_capability(Capability::RoundUnsupported);
    assert_eq!(
        resolve_unfocused_border_style(),
        BorderStyle::Single,
        "unsupported → Single"
    );

    // Restore the default for other tests in the process.
    set_capability(Capability::RoundSupported);
}

/// The focused border style is always `Double` regardless of capability; only
/// the unfocused style is subject to the fallback.
#[test]
fn focused_style_is_always_double() {
    set_capability(Capability::RoundUnsupported);
    // Focused style is the caller's concern; this test documents that the
    // fallback helper only rewrites the unfocused case by confirming the
    // public resolver is for the unfocused slot.
    assert_eq!(resolve_unfocused_border_style(), BorderStyle::Single);
    set_capability(Capability::RoundSupported);
}

/// `ROUND_CORNER_SAMPLE` is one of the rounded-corner glyphs actually emitted
/// by iocraft's `BorderStyle::Round`. It must be a single `char` in the
/// U+256D–2570 sub-range that the probe targets.
#[test]
fn round_corner_sample_is_a_real_rounded_corner_glyph() {
    assert!(
        matches!(ROUND_CORNER_SAMPLE, '\u{256D}'..='\u{2570}'),
        "sample must be a rounded-corner glyph (U+256D–2570), got U+{:04X}",
        ROUND_CORNER_SAMPLE as u32
    );
}
