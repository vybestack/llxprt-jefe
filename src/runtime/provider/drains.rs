//! Continuous stdout/stderr drains for the provider supervisor
//! (issue #390 CW-10, CW10-11).
//!
//! Two dedicated reader threads keep a provider's stdout and stderr draining for
//! the entire lifetime of one invocation. The stdout drain splits the byte
//! stream into complete JSONL frames through [`super::line_reader::LineBuffer`]
//! and forwards each frame (or a typed oversize fault) to the supervisor driver.
//! The stderr drain retains up to [`STDERR_RETENTION_MAX`] bytes so a
//! misbehaving provider cannot exhaust the host; the supervisor redacts the
//! retained bytes against resolved secrets before exposing them.
//!
//! No application state, effect, or persistence lives here.

use std::io::{self, Read};
use std::process::{ChildStderr, ChildStdout};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::error::ProviderError;
use super::line_reader::LineBuffer;

/// The inclusive maximum number of stderr bytes retained from one provider.
pub const STDERR_RETENTION_MAX: usize = 262_144;

/// The read-buffer size used by each drain.
const READ_CHUNK: usize = 8_192;

/// One event from the stdout drain.
#[derive(Debug)]
pub(super) enum StdoutEvent {
    /// One complete JSONL frame, terminator included.
    Frame(Vec<u8>),
    /// A run of bytes with no terminator exceeded the line byte bound.
    Oversize(ProviderError),
    /// The underlying read failed (the pipe broke, etc.).
    ReadError,
}

/// A stdout drain feeding complete frames to the supervisor driver.
///
/// The drain thread is detached: the supervisor observes completion through the
/// bounded channel (or the process reaping that closes the pipe), never through
/// an unbounded `join`.
pub(super) struct StdoutDrain {
    pub receiver: mpsc::Receiver<StdoutEvent>,
}

impl StdoutDrain {
    /// Spawn the drain for the given stdout stream.
    ///
    /// # Errors
    ///
    /// Returns the underlying thread-spawn error instead of discarding it. A
    /// spawn failure is propagated by the supervisor as a typed failure rather
    /// than surfacing later as an ambiguous EOF.
    pub(super) fn spawn(stream: ChildStdout) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<StdoutEvent>();
        thread::Builder::new()
            .name("jefe-provider-stdout".to_owned())
            .spawn(move || drive_stdout(stream, &sender))?;
        Ok(Self { receiver })
    }
}

fn drive_stdout(mut stream: ChildStdout, sender: &mpsc::Sender<StdoutEvent>) {
    let mut buffer = LineBuffer::new();
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => match buffer.push(&chunk[..read]) {
                Ok(frames) => {
                    for frame in frames {
                        if sender.send(StdoutEvent::Frame(frame)).is_err() {
                            return;
                        }
                    }
                }
                Err(error) => {
                    let _ = sender.send(StdoutEvent::Oversize(error.into_provider_error()));
                    return;
                }
            },
            Err(_) => {
                let _ = sender.send(StdoutEvent::ReadError);
                return;
            }
        }
    }
}

/// One event from the stderr drain.
#[derive(Debug)]
pub(super) enum StderrOutcome {
    /// The retained stderr bytes, capped at [`STDERR_RETENTION_MAX`].
    Retained {
        /// The retained bytes.
        bytes: Vec<u8>,
        /// Whether stderr exceeded the retention cap.
        truncated: bool,
    },
}

/// A stderr drain retaining a bounded window of provider output.
///
/// The drain thread is detached; completion is observed through the bounded
/// channel, never an unbounded `join`.
pub(super) struct StderrDrain {
    pub receiver: mpsc::Receiver<StderrOutcome>,
}

impl StderrDrain {
    /// Spawn the drain for the given stderr stream.
    ///
    /// # Errors
    ///
    /// Returns the underlying thread-spawn error instead of discarding it.
    pub(super) fn spawn(stream: ChildStderr) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<StderrOutcome>();
        thread::Builder::new()
            .name("jefe-provider-stderr".to_owned())
            .spawn(move || drive_stderr(stream, &sender))?;
        Ok(Self { receiver })
    }
}

/// The bounded stderr retention policy, separated from the read loop.
///
/// Kept as a value rather than inline flags so the "was anything actually
/// dropped" question can be asked directly. Conflating "the buffer is full"
/// with "bytes were lost" reports a complete capture as truncated, which sends
/// an operator looking for output that was never missing (CW10-14).
pub(super) struct StderrRetention {
    bytes: Vec<u8>,
    truncated: bool,
}

impl StderrRetention {
    pub(super) const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            truncated: false,
        }
    }

    /// Retain what still fits, and record truncation only when something was
    /// discarded.
    pub(super) fn push(&mut self, chunk: &[u8]) {
        let room = STDERR_RETENTION_MAX.saturating_sub(self.bytes.len());
        let take = room.min(chunk.len());
        self.bytes.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            self.truncated = true;
        }
    }

    /// The retained bytes and whether any byte was dropped.
    pub(super) fn finish(self) -> (Vec<u8>, bool) {
        (self.bytes, self.truncated)
    }
}

fn drive_stderr(mut stream: ChildStderr, sender: &mpsc::Sender<StderrOutcome>) {
    let mut retention = StderrRetention::new();
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => retention.push(&chunk[..read]),
        }
    }
    let (bytes, truncated) = retention.finish();
    let _ = sender.send(StderrOutcome::Retained { bytes, truncated });
}

/// The outcome of the bounded final stdout drain performed after process
/// exit/kill. Clean cleanup requires an observed stdout EOF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FinalStdoutOutcome {
    /// The stdout channel disconnected: EOF was actually observed.
    Eof,
    /// A frame arrived after the lifecycle completed (data-after-ack).
    DataAfterAck,
    /// A non-frame fault (oversize/read error) remained in the channel.
    Fault,
    /// No EOF was observed within the bound (a descendant likely holds the pipe).
    Timeout,
}

/// After process exit/kill, perform a bounded final stdout drain that observes
/// whether stdout actually reached EOF.
///
/// One bounded `recv_timeout(bound)` examines the next stdout channel event: a
/// channel disconnection ([`mpsc::RecvTimeoutError::Disconnected`]) is the EOF
/// the full lifecycle requires; a remaining frame is data-after-ack; an oversize
/// or read fault is a non-frame protocol/pipe fault. If no event arrives within
/// `bound`, [`FinalStdoutOutcome::Timeout`] is returned.
///
/// This never blocks unbounded: `recv_timeout` is the completion signal and the
/// drain handle is detached rather than joined.
pub(super) fn final_stdout_drain(
    receiver: &mpsc::Receiver<StdoutEvent>,
    bound: Duration,
) -> FinalStdoutOutcome {
    match receiver.recv_timeout(bound) {
        Ok(StdoutEvent::Frame(_)) => FinalStdoutOutcome::DataAfterAck,
        Ok(StdoutEvent::Oversize(_) | StdoutEvent::ReadError) => FinalStdoutOutcome::Fault,
        Err(mpsc::RecvTimeoutError::Disconnected) => FinalStdoutOutcome::Eof,
        Err(mpsc::RecvTimeoutError::Timeout) => FinalStdoutOutcome::Timeout,
    }
}
