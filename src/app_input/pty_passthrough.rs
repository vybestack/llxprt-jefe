//! Forwarding keystrokes to the attached agent PTY.
//!
//! Extracted from `app_shell` so the key handler stays under the source-file
//! size limit. These functions encode a key event to PTY bytes and write them
//! to the currently-attached agent terminal, plus the `Ctrl-C` interrupt
//! passthrough (issue #200) that forwards `Ctrl-C` to the agent terminal
//! regardless of pane focus.

use iocraft::hooks::State as HookState;
use iocraft::prelude::KeyEvent;
use tracing::{debug, trace, warn};

use crate::pty_encoding::{
    PasteEnterSuppression, key_to_bytes, should_arm_paste_enter_suppression,
    should_disarm_paste_enter_suppression, should_suppress_synthetic_enter,
};
use jefe::input::{InputMode, is_bare_ctrl_c};
use jefe::runtime::{RuntimeError, RuntimeManager};

use std::sync::Arc;
use std::time::Instant;

/// Shared ctx handle type (mirrors `app_shell::CtxArc`).
pub type CtxArc = Arc<std::sync::Mutex<crate::AppContext>>;

/// Encode a key event to PTY bytes and write it to the attached agent terminal.
///
/// Mirrors the previous in-`app_shell` implementation: keys that cannot be
/// encoded are ignored and clear the paste-Enter suppression arm.
pub fn forward_key_to_pty(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
) {
    let encoded = key_to_bytes(key_event, false);

    trace!(
        code = ?key_event.code,
        modifiers = ?key_event.modifiers,
        encoded_len = encoded.as_ref().map_or(0, std::vec::Vec::len),
        "forwarding key to PTY"
    );

    let unmapped = encoded.is_none();
    if let Some(bytes) = encoded
        && let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        if let Err(e) = ctx_guard.runtime.write_input(&bytes)
            && !matches!(e, RuntimeError::WriteFailed(_))
        {
            warn!(error = %e, "runtime.write_input failed");
        }
    } else if unmapped {
        // Unmapped key: ignore immediately and clear suppression arm.
        suppress_next_enter.set(PasteEnterSuppression::new());
    }
}

/// Tri-state probe for the attached agent terminal (issue #333).
///
/// `Busy` means the ctx mutex is held elsewhere (e.g. async attach). The
/// interrupt path must still forward `Ctrl-C` because [`forward_key_to_pty`]
/// uses a blocking `lock()` and can wait for the attach to finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AttachedTerminalProbe {
    Attached,
    Absent,
    Busy,
}

/// Classify from a non-blocking lock attempt and attached-agent presence.
#[must_use]
pub(super) fn attached_terminal_probe_from_lock(
    try_lock_ok: bool,
    has_attached_agent: bool,
) -> AttachedTerminalProbe {
    if !try_lock_ok {
        AttachedTerminalProbe::Busy
    } else if has_attached_agent {
        AttachedTerminalProbe::Attached
    } else {
        AttachedTerminalProbe::Absent
    }
}

#[must_use]
pub(super) fn ctrl_c_passthrough_may_forward(probe: AttachedTerminalProbe) -> bool {
    matches!(
        probe,
        AttachedTerminalProbe::Attached | AttachedTerminalProbe::Busy
    )
}

/// Non-blocking probe used on the key path before `Ctrl-C` passthrough.
#[must_use]
pub(super) fn probe_attached_terminal(ctx: Option<&CtxArc>) -> AttachedTerminalProbe {
    let Some(ctx_arc) = ctx else {
        return AttachedTerminalProbe::Absent;
    };
    match ctx_arc.try_lock() {
        Ok(guard) => {
            attached_terminal_probe_from_lock(true, guard.runtime.attached_agent().is_some())
        }
        Err(_) => AttachedTerminalProbe::Busy,
    }
}

/// Forward `Ctrl-C` to the attached agent terminal regardless of pane focus
/// (#200).
///
/// `Ctrl-C`'s only sensible meaning when an agent terminal is attached is
/// "interrupt the agent's foreground shell / cancel the run". Routing it to the
/// agent terminal even when the terminal pane is not in dedicated capture mode
/// makes the interrupt reliable and side-steps the F12 toggle trap: creating or
/// selecting an agent auto-focuses the terminal, so a user pressing F12
/// (advertised as "terminal focus") can inadvertently *unfocus* it, after which
/// a `TerminalCapture`-gated forward would silently drop `Ctrl-C`.
///
/// Returns `true` (caller returns) only when an agent terminal is attached, the
/// plain dashboard owns the key (`Normal` mode — no modal/form/search), and the
/// key is a bare `Ctrl-C`. Returns `false` otherwise so the normal key path
/// proceeds.
pub fn try_ctrl_c_interrupt_passthrough(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    input_mode: InputMode,
    key_event: &KeyEvent,
) -> bool {
    if input_mode != InputMode::Normal
        || !is_bare_ctrl_c(key_event)
        || !ctrl_c_passthrough_may_forward(probe_attached_terminal(ctx))
    {
        return false;
    }
    forward_key_to_pty(ctx, suppress_next_enter, key_event);
    true
}

/// Check paste-Enter suppression. Returns `true` when the key was a synthetic
/// Enter that should be swallowed (caller returns) (issue #286).
pub fn try_suppress_synthetic_enter(
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
    now: Instant,
) -> bool {
    if should_suppress_synthetic_enter(suppress_next_enter.get(), key_event, now) {
        debug!("suppressing synthetic Enter preceding paste");
        suppress_next_enter.set(PasteEnterSuppression::new());
        true
    } else {
        false
    }
}

/// Arm or clear the paste-Enter suppression based on the current key, input
/// mode, and current time (issue #286).
pub fn update_paste_enter_suppression(
    input_mode: InputMode,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: &KeyEvent,
    now: Instant,
) {
    if should_arm_paste_enter_suppression(key_event, input_mode) {
        let mut next = suppress_next_enter.get();
        next.arm(now);
        suppress_next_enter.set(next);
    } else if should_disarm_paste_enter_suppression(suppress_next_enter.get(), key_event, now) {
        suppress_next_enter.set(PasteEnterSuppression::new());
    }
}
