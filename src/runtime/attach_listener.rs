//! Terminal-model event sink for a hosted agent PTY.
//!
//! Extracted from `attach.rs` to keep that file inside the source-file size
//! limit. The listener is how the embedded terminal model talks back: clipboard
//! stores go to the host clipboard boundary, and identity/mode query replies go
//! onto the hosted child's own input stream.

use std::io::Write;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::term::Config as TermConfig;
use tracing::{debug, trace, warn};

/// Runtime event listener for alacritty_terminal.
///
/// Handles OSC52 clipboard-store events so an agent's copy propagates to the
/// host clipboard when running inside jefe's embedded PTY, and answers the
/// child's terminal queries on the child's own input.
pub struct RuntimeListener {
    /// Write end of the hosted child's PTY.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl std::fmt::Debug for RuntimeListener {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("RuntimeListener").finish()
    }
}

impl RuntimeListener {
    /// Build a listener that can answer the hosted child on its own input.
    #[must_use]
    pub fn new(writer: Arc<Mutex<Box<dyn Write + Send>>>) -> Self {
        Self { writer }
    }
}

/// Terminal-model configuration for a hosted agent PTY.
///
/// The kitty keyboard protocol is switched on so the model answers the child's
/// `CSI ? u` query and tracks the modes it pushes. Without it the model
/// silently ignores every keyboard-protocol control sequence, the child
/// concludes the terminal cannot disambiguate key chords, and chords such as
/// `Ctrl+Enter` become indistinguishable from their legacy control-byte
/// aliases (issue #627).
#[must_use]
pub fn embedded_term_config() -> TermConfig {
    TermConfig {
        kitty_keyboard: true,
        ..TermConfig::default()
    }
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

/// Write a terminal query reply back onto the hosted child's input stream.
///
/// The embedded terminal model answers identity and mode queries (DA1, kitty
/// keyboard flags, XTVERSION, `modifyOtherKeys`, DSR) by emitting
/// `TermEvent::PtyWrite`. Those replies are input for the child, not output for
/// the host: a child that never receives them concludes the terminal supports
/// nothing it asked about, and many block their raw-mode setup until the
/// device-attributes reply arrives (issue #627).
fn forward_pty_write(writer: &Mutex<Box<dyn Write + Send>>, text: &str) -> std::io::Result<()> {
    let mut writer = writer
        .lock()
        .map_err(|_| std::io::Error::other("pty writer lock poisoned"))?;
    writer.write_all(text.as_bytes())?;
    writer.flush()
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
                trace!(len = text.len(), "answering terminal query on child input");
                if let Err(error) = forward_pty_write(&self.writer, &text) {
                    warn!(%error, "failed to answer terminal query on child input");
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "attach_listener_tests.rs"]
mod tests;
