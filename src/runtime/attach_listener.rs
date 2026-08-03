//! Terminal-model event sink for a hosted multiplexer client.
//!
//! Extracted from `attach.rs` to keep that file inside the source-file size
//! limit. The listener is how the embedded terminal model talks back: clipboard
//! stores go to the host clipboard boundary, and identity/mode query replies
//! are queued for the hosted client's own input stream.

use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::term::Config as TermConfig;
use tracing::{debug, trace, warn};

/// Terminal-query replies waiting to be written to the hosted client.
pub(super) type PendingReplies = Arc<Mutex<Vec<u8>>>;

/// Runtime event listener for alacritty_terminal.
///
/// Handles OSC52 clipboard-store events so an agent's copy propagates to the
/// host clipboard when running inside jefe's embedded PTY, and queues the
/// terminal-query replies the model produces.
///
/// The listener runs inside the terminal parser, which holds the terminal-model
/// lock, so it performs no PTY I/O of its own: a child that has stopped reading
/// its input would otherwise block the reader thread mid-parse and stall every
/// render behind the model lock. Replies are queued here and written once the
/// parser has released the model (issue #627).
pub struct RuntimeListener {
    pending: PendingReplies,
}

impl std::fmt::Debug for RuntimeListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RuntimeListener").finish()
    }
}

impl RuntimeListener {
    /// Build a listener that queues query replies into `pending`.
    #[must_use]
    pub fn new(pending: PendingReplies) -> Self {
        Self { pending }
    }
}

/// Terminal-model configuration for a hosted multiplexer client.
///
/// Jefe does not implement the kitty keyboard protocol, so the model is left
/// with alacritty's default configuration rather than advertising a protocol
/// whose flags jefe would not honour. Modified key chords survive the
/// multiplexer hop through the multiplexer's own extended-key support instead
/// (issue #627).
#[must_use]
pub fn embedded_term_config() -> TermConfig {
    TermConfig::default()
}

// ClipboardStore uses the same OSC 52 boundary as Jefe selections. This keeps
// provider policy centralized; unsupported outer terminals may ignore OSC 52.
pub(super) fn forward_clipboard_store<F>(text: &str, mut writer: F) -> std::io::Result<()>
where
    F: FnMut(&str) -> std::io::Result<()>,
{
    if text.is_empty() {
        return Ok(());
    }
    writer(text)
}

/// Queue a terminal query reply for the hosted client's input stream.
///
/// The embedded terminal model answers identity and mode queries (device
/// attributes, XTVERSION, cursor position, ...) by emitting
/// `TermEvent::PtyWrite`. Those replies are input for the hosted client, not
/// output for the host: a client that never receives them keeps identifying its
/// terminal until its own timeouts expire (issue #627).
pub(super) fn queue_pty_write(pending: &Mutex<Vec<u8>>, text: &str) {
    if let Ok(mut pending) = pending.lock() {
        pending.extend_from_slice(text.as_bytes());
    } else {
        warn!("dropping terminal query reply: pending-reply lock poisoned");
    }
}

impl EventListener for RuntimeListener {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::ClipboardStore(_, text) => {
                debug!(len = text.len(), "received OSC52 ClipboardStore event");
                if let Err(error) = forward_clipboard_store(&text, crate::clipboard::write_osc52) {
                    warn!(%error, "failed to forward OSC52 clipboard store");
                }
            }
            TermEvent::PtyWrite(text) => {
                trace!(len = text.len(), "queueing terminal query reply");
                queue_pty_write(&self.pending, &text);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "attach_listener_tests.rs"]
mod tests;
