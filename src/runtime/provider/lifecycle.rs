//! Pure handshake and operation order validator with fixed-generation binding
//! (issue #390 CW-10, CW10-06 order).
//!
//! The handshake is exactly `hello`, `hello-ack`, `configure`, `ready`. After
//! `ready` the process is steady until `shutdown`, after which exactly
//! `shutdown-ack` terminates it. The generation is fixed by the first observed
//! message and any later change is fatal. No process, state, effect, or
//! persistence lives here.

use super::error::ProviderError;
use super::identifiers::MessageKind;

/// One handshake/operation lifecycle phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    /// Awaiting the host `hello`.
    AwaitHello,
    /// Awaiting the provider `hello-ack`.
    AwaitHelloAck,
    /// Awaiting the host `configure`.
    AwaitConfigure,
    /// Awaiting the provider `ready`.
    AwaitReady,
    /// Steady state: invoke, cancel, progress, outcome, error, shutdown.
    Ready,
    /// Awaiting the provider `shutdown-ack`.
    AwaitShutdownAck,
    /// Terminated; any further message is fatal.
    Terminated,
}

impl LifecyclePhase {
    /// A stable, operator-readable name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitHello => "await-hello",
            Self::AwaitHelloAck => "await-hello-ack",
            Self::AwaitConfigure => "await-configure",
            Self::AwaitReady => "await-ready",
            Self::Ready => "ready",
            Self::AwaitShutdownAck => "await-shutdown-ack",
            Self::Terminated => "terminated",
        }
    }
}

/// Pure handshake and operation order validator with fixed-generation binding.
#[derive(Debug, Clone)]
pub struct LifecycleOrder {
    phase: LifecyclePhase,
    generation: Option<u64>,
}

impl LifecycleOrder {
    /// Construct a validator at the start of a handshake.
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: LifecyclePhase::AwaitHello,
            generation: None,
        }
    }

    /// The current lifecycle phase.
    #[must_use]
    pub fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Validate and absorb one ordered message.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for a generation change or an out-of-order
    /// message.
    pub fn observe(&mut self, kind: MessageKind, generation: u64) -> Result<(), ProviderError> {
        match self.generation {
            None => self.generation = Some(generation),
            Some(fixed) if fixed == generation => {}
            Some(_) => {
                return Err(ProviderError::InvalidGeneration { value: generation });
            }
        }
        let next = match (self.phase, kind) {
            (LifecyclePhase::AwaitHello, MessageKind::Hello) => LifecyclePhase::AwaitHelloAck,
            (LifecyclePhase::AwaitHelloAck, MessageKind::HelloAck) => {
                LifecyclePhase::AwaitConfigure
            }
            (LifecyclePhase::AwaitConfigure, MessageKind::Configure) => LifecyclePhase::AwaitReady,
            // The handshake completes on `ready` and steady-state messages keep
            // the process in the `ready` phase; both stay at [`LifecyclePhase::Ready`].
            (LifecyclePhase::AwaitReady, MessageKind::Ready)
            | (
                LifecyclePhase::Ready,
                MessageKind::InvokeAction
                | MessageKind::Cancel
                | MessageKind::Progress
                | MessageKind::Outcome
                | MessageKind::Error,
            ) => LifecyclePhase::Ready,
            (LifecyclePhase::Ready, MessageKind::Shutdown) => LifecyclePhase::AwaitShutdownAck,
            (LifecyclePhase::AwaitShutdownAck, MessageKind::ShutdownAck) => {
                LifecyclePhase::Terminated
            }
            (phase, _) => {
                return Err(ProviderError::OutOfOrder {
                    phase: phase.as_str().to_owned(),
                    kind: kind.as_str().to_owned(),
                });
            }
        };
        self.phase = next;
        Ok(())
    }
}

impl Default for LifecycleOrder {
    fn default() -> Self {
        Self::new()
    }
}
