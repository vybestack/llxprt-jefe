//! Keyboard-transport contract tests for the attached agent PTY (issue #627).
//!
//! jefe's embedded terminal has to behave like a terminal towards the child it
//! hosts. Two obligations are exercised here: identity/mode queries must be
//! answered on the child's own input stream, and the Enter chord family must be
//! encoded so the child can tell `Ctrl+Enter` from `Ctrl+J`.

use super::*;

use std::sync::{Arc, Mutex};

/// A `Write` sink that records everything written to it.
struct CaptureWriter {
    written: Arc<Mutex<Vec<u8>>>,
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut written) = self.written.lock() {
            written.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ListenerHarness {
    term: Mutex<Term<RuntimeListener>>,
    parser: Processor<StdSyncHandler>,
    dirty: AtomicBool,
    written: Arc<Mutex<Vec<u8>>>,
}

impl ListenerHarness {
    fn new() -> Self {
        let written = Arc::new(Mutex::new(Vec::new()));
        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(CaptureWriter {
                written: Arc::clone(&written),
            })));
        let size = TermDimensions { cols: 80, rows: 24 };
        let term = Term::new(embedded_term_config(), &size, RuntimeListener::new(writer));
        Self {
            term: Mutex::new(term),
            parser: Processor::new(),
            dirty: AtomicBool::new(false),
            written,
        }
    }

    /// Feed bytes to the terminal model exactly as the reader thread does.
    fn feed(&mut self, bytes: &[u8]) {
        process_pty_read(bytes, &mut self.parser, &self.term, &self.dirty);
    }

    fn written(&self) -> Vec<u8> {
        self.written
            .lock()
            .map(|written| written.clone())
            .unwrap_or_default()
    }

    fn mode(&self) -> TermMode {
        self.term
            .lock()
            .map(|term| *term.mode())
            .unwrap_or_default()
    }
}

/// A1: the kitty keyboard-flags query is answered on the child's input stream.
/// Without a reply the child concludes the protocol is unsupported and can
/// never enable the disambiguation this issue depends on.
#[test]
fn kitty_keyboard_query_is_answered_on_the_child_input_stream() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[?u");

    let written = harness.written();
    let reply = String::from_utf8_lossy(&written).into_owned();
    assert!(
        reply.starts_with("\x1b[?") && reply.ends_with('u'),
        "kitty keyboard flags query must be answered with CSI ? <flags> u, got {reply:?}"
    );
}

/// A1: the primary device-attributes query is answered too. Children commonly
/// block their raw-mode setup until DA1 comes back, so dropping it stalls
/// startup even when nothing else is negotiated.
#[test]
fn device_attributes_query_is_answered_on_the_child_input_stream() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[c");

    let written = harness.written();
    assert!(
        !written.is_empty(),
        "primary device attributes query must be answered"
    );
    assert!(
        written.starts_with(b"\x1b[?"),
        "DA1 reply must be a CSI ? ... c report, got {:?}",
        String::from_utf8_lossy(&written)
    );
}

/// A1: replies keep their order and are not merged or reordered.
#[test]
fn successive_queries_are_answered_in_order() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[?u");
    let after_first = harness.written();
    harness.feed(b"\x1b[c");
    let after_second = harness.written();

    assert!(
        after_second.starts_with(&after_first),
        "the second reply must be appended after the first, not replace it"
    );
    assert!(
        after_second.len() > after_first.len(),
        "the second query must produce its own reply"
    );
}

/// A3: events that are not query replies never reach the child's input.
#[test]
fn non_query_terminal_events_write_nothing_to_the_child() {
    let mut harness = ListenerHarness::new();

    // A bell, a window title change, and ordinary text all raise terminal
    // events or state changes, none of which are input for the child.
    harness.feed(b"\x07\x1b]0;title\x07hello world");

    assert!(
        harness.written().is_empty(),
        "only query replies may be written to the child's input, got {:?}",
        String::from_utf8_lossy(&harness.written())
    );
}

/// A4: after the child pushes the kitty keyboard flags, the negotiated mode is
/// observable so the key encoder can switch to CSI-u.
#[test]
fn pushed_kitty_keyboard_flags_are_observable() {
    let mut harness = ListenerHarness::new();

    assert!(
        !harness.mode().contains(TermMode::DISAMBIGUATE_ESC_CODES),
        "disambiguation must be off until the child asks for it"
    );

    harness.feed(b"\x1b[>1u");

    assert!(
        harness.mode().contains(TermMode::DISAMBIGUATE_ESC_CODES),
        "pushing kitty flag 1 must enable escape-code disambiguation"
    );
}
