//! Terminal-query contract tests for the attached agent PTY (issue #627).
//!
//! Jefe's embedded terminal has to behave like a terminal towards the process
//! it hosts. That process is a multiplexer client, and it identifies its
//! terminal at startup before it will drive the pane; a terminal that never
//! answers leaves it identifying nothing until its own timeouts expire.

use super::*;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use crate::runtime::key_pacing::ENTER_INPUT_GAP;

/// Read a shared buffer, keeping a poisoned lock visible rather than reporting
/// an empty buffer: several assertions below are satisfied by emptiness, so
/// swallowing poison would turn a panicking sibling test into a vacuous pass.
fn snapshot(buffer: &Mutex<Vec<u8>>) -> Vec<u8> {
    buffer
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// A `Write` sink that records everything written to it.
struct CaptureWriter {
    written: Arc<Mutex<Vec<u8>>>,
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.written
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
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
    pending: PendingReplies,
    input: Arc<Mutex<PacedPtyInput>>,
    written: Arc<Mutex<Vec<u8>>>,
}

impl ListenerHarness {
    fn new() -> Self {
        let written = Arc::new(Mutex::new(Vec::new()));
        let input = Arc::new(Mutex::new(PacedPtyInput::new(Box::new(CaptureWriter {
            written: Arc::clone(&written),
        }))));
        let pending: PendingReplies = Arc::new(Mutex::new(Vec::new()));
        let size = TermDimensions { cols: 80, rows: 24 };
        let term = Term::new(
            embedded_term_config(),
            &size,
            RuntimeListener::new(Arc::clone(&pending)),
        );
        Self {
            term: Mutex::new(term),
            parser: Processor::new(),
            dirty: AtomicBool::new(false),
            pending,
            input,
            written,
        }
    }

    /// Feed bytes to the terminal model exactly as the reader thread does,
    /// including the reply flush that follows once the model is released.
    fn feed(&mut self, bytes: &[u8]) {
        process_pty_read(bytes, &mut self.parser, &self.term, &self.dirty);
        flush_pending_replies(&self.pending, &self.input);
    }

    /// Feed bytes without flushing, so the queue itself can be inspected.
    fn parse_only(&mut self, bytes: &[u8]) {
        process_pty_read(bytes, &mut self.parser, &self.term, &self.dirty);
    }

    fn queued(&self) -> Vec<u8> {
        snapshot(&self.pending)
    }

    fn written(&self) -> Vec<u8> {
        snapshot(&self.written)
    }
}

/// A1: the primary device-attributes query is answered. A multiplexer client
/// sends this at startup and waits for it before it trusts anything else it
/// asked about its terminal.
#[test]
fn device_attributes_query_is_answered_on_the_client_input_stream() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[c");

    let written = harness.written();
    assert!(
        written.starts_with(b"\x1b[?"),
        "DA1 must be answered with a CSI ? ... c report, got {:?}",
        String::from_utf8_lossy(&written)
    );
    assert!(
        written.ends_with(b"c"),
        "DA1 reply must terminate with 'c', got {:?}",
        String::from_utf8_lossy(&written)
    );
}

/// A1: the secondary device-attributes query is answered too.
#[test]
fn secondary_device_attributes_query_is_answered() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[>c");

    let written = harness.written();
    assert!(
        !written.is_empty(),
        "secondary device attributes must be answered"
    );
    assert!(
        written.ends_with(b"c"),
        "DA2 reply must terminate with 'c', got {:?}",
        String::from_utf8_lossy(&written)
    );
}

/// A1: a cursor-position report is answered, so a client that measures the
/// cursor before drawing is not left waiting.
#[test]
fn cursor_position_report_is_answered() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[6n");

    let written = harness.written();
    assert!(
        written.starts_with(b"\x1b[") && written.ends_with(b"R"),
        "DSR 6 must be answered with a CSI ... R report, got {:?}",
        String::from_utf8_lossy(&written)
    );
}

/// A1: replies keep their order and are not merged or replaced.
#[test]
fn successive_queries_are_answered_in_order() {
    let mut harness = ListenerHarness::new();

    harness.feed(b"\x1b[c");
    let after_first = harness.written();
    harness.feed(b"\x1b[6n");
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

/// A1: the parser never writes to the child itself. It only queues, so a child
/// that has stopped reading its input cannot block the reader thread while it
/// holds the terminal-model lock every render needs.
#[test]
fn replies_are_queued_by_the_parser_and_written_only_afterwards() {
    let mut harness = ListenerHarness::new();

    harness.parse_only(b"\x1b[c");

    assert!(
        !harness.queued().is_empty(),
        "the parser must queue the reply"
    );
    assert!(
        harness.written().is_empty(),
        "the parser must not write to the child while it holds the model"
    );

    flush_pending_replies(&harness.pending, &harness.input);

    assert!(
        !harness.written().is_empty(),
        "the queued reply must be written once the model is released"
    );
    assert!(
        harness.queued().is_empty(),
        "a flushed reply must not be written twice"
    );
}

/// A3: events that are not query replies never reach the client's input.
#[test]
fn non_query_terminal_events_write_nothing_to_the_client() {
    let mut harness = ListenerHarness::new();

    // A bell, a window title change, and ordinary text all raise terminal
    // events or state changes, none of which are input for the client.
    harness.feed(b"\x07\x1b]0;title\x07hello world");

    assert!(
        harness.written().is_empty(),
        "only query replies may be written to the client's input, got {:?}",
        String::from_utf8_lossy(&harness.written())
    );
}

/// A10: an Enter written straight after another write really is held back on
/// the wire, not merely calculated to be.
#[test]
fn an_enter_write_is_separated_from_the_write_before_it() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut input = PacedPtyInput::new(Box::new(CaptureWriter {
        written: Arc::clone(&written),
    }));

    let start = Instant::now();
    let first = input.write(b"x", PtyInputKind::Other);
    let ordinary_elapsed = start.elapsed();
    let second = input.write(b"\r", PtyInputKind::Enter);
    let total_elapsed = start.elapsed();

    assert!(first.is_ok() && second.is_ok(), "writes should succeed");
    assert!(
        ordinary_elapsed < ENTER_INPUT_GAP,
        "an ordinary write must not be delayed, took {ordinary_elapsed:?}"
    );
    assert!(
        total_elapsed >= ENTER_INPUT_GAP,
        "the Enter must be held back by the guard interval, total was {total_elapsed:?}"
    );
    assert_eq!(
        snapshot(&written),
        b"x\r".to_vec(),
        "both writes must reach the child, in order"
    );
}

/// A11/A12: a query reply arriving between two keystrokes counts as a write,
/// so the Enter is separated from the reply rather than from the older key.
#[test]
fn a_query_reply_between_keystrokes_still_separates_the_enter() {
    let written = Arc::new(Mutex::new(Vec::new()));
    let mut input = PacedPtyInput::new(Box::new(CaptureWriter {
        written: Arc::clone(&written),
    }));

    assert!(input.write(b"x", PtyInputKind::Other).is_ok());
    std::thread::sleep(ENTER_INPUT_GAP);
    // A terminal-query reply goes out on the same paced writer.
    assert!(input.write(b"\x1b[?6c", PtyInputKind::Other).is_ok());

    let start = Instant::now();
    assert!(input.write(b"\r", PtyInputKind::Enter).is_ok());

    assert!(
        start.elapsed() >= ENTER_INPUT_GAP,
        "the Enter must be separated from the reply, not from the older keystroke"
    );
}
