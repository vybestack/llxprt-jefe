//! The typed result surface of one provider lifecycle (issue #390 CW-10).
//!
//! Everything the supervisor is allowed to hand back lives here: the terminal
//! outcome, the typed failures, the ordered lifecycle transcript, and the
//! complete result. Keeping them apart from the process plumbing is the point —
//! nothing in this module owns a child, a pipe, or a thread, so a caller can
//! read the whole contract of "what a provider run can tell you" without
//! reading how a process is spawned or reaped.
//!
//! No type here carries a secret value: the supervisor redacts every
//! provider-authored string against the resolved secrets before it is placed in
//! one of these values (CW10-14).

use super::environment::{EnvironmentError, Redactor};
use super::protocol::MessageKind;
use super::{dto, error};

/// The typed terminal result of a one-shot invocation.
#[derive(Debug, Clone)]
pub enum OneShotOutcome {
    /// The provider returned a successful outcome.
    Completed(dto::Outcome),
    /// The provider reported a typed error.
    ProviderError(dto::ErrorPayload),
    /// The host cancelled the invocation (first terminal from the session;
    /// the reducer already marks the request Cancelled, S17).
    Cancelled,
    /// A supervisor-level failure (spawn, protocol, timeout, crash, I/O).
    Failed(SupervisorFailure),
}

/// A supervisor-level failure. No variant carries a secret value.
#[derive(Debug, Clone)]
pub enum SupervisorFailure {
    /// The process could not be spawned.
    Spawn(String),
    /// Environment construction failed.
    Environment(EnvironmentError),
    /// A closed-protocol failure (`PLG-E502`).
    Protocol(error::ProviderError),
    /// A handshake stage did not complete in time.
    HandshakeTimeout,
    /// The invocation did not reach a terminal in time.
    InvocationTimeout,
    /// The provider did not shut down in time.
    ShutdownTimeout,
    /// A pipe I/O failure.
    Io(String),
    /// The provider exited or closed stdout before a terminal.
    Crashed {
        /// The observed exit code, if the process exited normally.
        exit: Option<i32>,
    },
}

impl SupervisorFailure {
    /// The stable operator code for this failure.
    ///
    /// Only a closed-protocol contract violation ([`Self::Protocol`]) carries
    /// `PLG-E502`. Every other variant is a runtime-unavailable condition
    /// (spawn, environment, I/O, timeout, crash) and carries `PLG-E503`, the
    /// existing unavailable contract, so an operator can distinguish a protocol
    /// violation from a runtime failure.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Protocol(_) => error::PROTOCOL_FAILURE_CODE,
            Self::Spawn(_)
            | Self::Environment(_)
            | Self::HandshakeTimeout
            | Self::InvocationTimeout
            | Self::ShutdownTimeout
            | Self::Io(_)
            | Self::Crashed { .. } => error::RUNTIME_UNAVAILABLE_CODE,
        }
    }
}

/// A bounded cleanup/lifecycle failure observed after the authoritative
/// terminal result.
///
/// The terminal outcome ([`OneShotOutcome`]) remains the request result and is
/// never replaced by a cleanup failure; this type is reported separately so the
/// full shutdown/ack/EOF/reap lifecycle is visible. No variant carries a secret
/// value: a shutdown-ack fault carries the typed protocol error (whose strings
/// are redacted before it leaves the supervisor), and the drain/reap failures
/// carry no operator text.
///
/// A clean cleanup requires the leader to be reaped **and** both pipes to close:
/// descendants are never assumed reaped merely because the leader reaped. A
/// surviving descendant that holds an inherited stdout/stderr pipe surfaces as
/// [`Self::DrainTimeout`] (the bounded final drains did not observe closure).
#[derive(Debug, Clone)]
pub enum CleanupFailure {
    /// The provider did not send a valid `shutdown-ack`: wrong kind, malformed
    /// line, wrong generation/order, missing (timeout), or EOF before the ack.
    /// Data buffered after a valid ack is also reported here (observed by the
    /// bounded final stdout drain, not by an unbounded wait).
    ShutdownAck(error::ProviderError),
    /// Provider bytes observed after the request's first terminal event. The
    /// original result remains authoritative; this protocol failure is emitted
    /// separately and the persistent generation is no longer reusable.
    PostTerminal(error::ProviderError),
    /// The stdout or stderr drain did not close within the final-drain bound, so
    /// the process tree could not be observed fully drained (a descendant likely
    /// holds an inherited pipe). The leader may have reaped while a descendant
    /// still holds the pipes.
    DrainTimeout,
    /// The process tree could not be observed reaped within the bound.
    NotReaped,
    /// A best-effort cleanup I/O step failed: a `shutdown` write/flush (the
    /// provider closed its stdin or exited) or a terminate/force-kill command.
    /// The reap still escalates and reaps; this is the runtime evidence.
    Io(String),
}

impl CleanupFailure {
    /// The stable operator code for this cleanup failure. A shutdown-ack
    /// protocol fault is `PLG-E502`; drain/reap failures are runtime-unavailable
    /// (`PLG-E503`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ShutdownAck(_) | Self::PostTerminal(_) => error::PROTOCOL_FAILURE_CODE,
            Self::DrainTimeout | Self::NotReaped | Self::Io(_) => error::RUNTIME_UNAVAILABLE_CODE,
        }
    }
}

/// One observable lifecycle transcript entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEntry {
    /// A host-sent message kind.
    Sent(MessageKind),
    /// A provider-received message kind.
    Received(MessageKind),
    /// A progress event with its sequence.
    Progress(u16),
    /// The provider's stdout reached EOF.
    Eof,
    /// The process tree was reaped.
    Reaped,
}

/// The ordered observable lifecycle transcript.
///
/// [`Self::entries`] records the lifecycle shape (which kinds were exchanged,
/// in which order). [`Self::progress`] records what each progress event
/// actually said, because a sequence number alone is not progress an operator
/// can read: the reducer's progress model is message, completed and total
/// (CW10-07). The two are kept apart so the lifecycle record stays a record of
/// order rather than a carrier of payload data.
#[derive(Debug, Clone, Default)]
pub struct LifecycleTranscript {
    entries: Vec<TranscriptEntry>,
    progress: Vec<dto::ProgressPayload>,
}

impl LifecycleTranscript {
    pub(super) fn push(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
    }

    pub(super) fn push_progress(&mut self, payload: dto::ProgressPayload) {
        self.progress.push(payload);
    }

    /// The ordered transcript entries.
    #[must_use]
    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    /// The ordered progress payloads exactly as the provider sent them, with
    /// every resolved secret redacted from the message text.
    #[must_use]
    pub fn progress(&self) -> &[dto::ProgressPayload] {
        &self.progress
    }

    /// Redact every progress message against the resolved secrets.
    ///
    /// A provider can echo a configured secret into its own progress text, and
    /// progress reaches the UI, so it is redacted on the same terms as stderr.
    pub(super) fn redact_progress(&mut self, redactor: &Redactor) {
        for payload in &mut self.progress {
            payload.message = redactor.redact(&payload.message).into_owned();
        }
    }
}

/// The complete typed result of a one-shot invocation.
#[derive(Debug, Clone)]
pub struct OneShotResult {
    /// The terminal outcome (success, provider error, or supervisor failure).
    pub outcome: OneShotOutcome,
    /// The ordered lifecycle transcript.
    pub transcript: LifecycleTranscript,
    /// Retained stderr, redacted against resolved secrets, capped at the bound.
    pub retained_stderr: String,
    /// Whether retained stderr was truncated at the retention cap.
    pub stderr_truncated: bool,
    /// Whether the process tree was reaped.
    pub process_reaped: bool,
    /// The provider exit code, if observed.
    pub exit_code: Option<i32>,
    /// A bounded cleanup/lifecycle failure observed after the terminal result,
    /// if the full shutdown/ack/EOF/reap lifecycle did not complete cleanly.
    /// Never replaces [`Self::outcome`].
    pub cleanup_failure: Option<CleanupFailure>,
}

impl OneShotResult {
    /// Build a result for a failure that occurred before any process existed.
    pub(super) fn pre_spawn(failure: SupervisorFailure) -> Self {
        Self {
            outcome: OneShotOutcome::Failed(failure),
            transcript: LifecycleTranscript::default(),
            retained_stderr: String::new(),
            stderr_truncated: false,
            process_reaped: false,
            exit_code: None,
            cleanup_failure: None,
        }
    }

    /// A terminal-failed result for a failure outside the process lifecycle
    /// (pre-spawn, thread panic). Public so the runtime boundary can construct
    /// one without spawning a process. No process evidence is carried.
    #[must_use]
    pub fn without_process(failure: SupervisorFailure) -> Self {
        Self::pre_spawn(failure)
    }
}
