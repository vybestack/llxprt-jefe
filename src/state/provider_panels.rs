//! Pure deterministic provider-panel lifecycle reducer (issue #391).
//!
//! Sole owner of panel instance identity, lifecycle, activation generation,
//! accepted snapshot model, revision, per-generation rate state, and bounded
//! host-local presentation state. No process handle, pipe, clock read, I/O, or
//! persistence lives here: every monotonic timestamp is injected by the caller
//! so the reducer stays deterministic. The returned effect descriptors are
//! plain values staged for post-commit delivery by the runtime.
//!
//! Every protocol/model failure is observable as [`PanelError`] carrying the
//! `PLG-E502` code without echoing the offending value.

use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::Field;
use crate::domain::{Id, TypedMap};
use crate::runtime::provider::protocol::{
    BodyKind, DeactivateReason, HostLocal, PanelEvent, PanelSnapshot,
};
use crate::workbench::PanelId;
pub use crate::workbench::PanelInstanceId;

#[path = "provider_panels_canonical.rs"]
mod canonical;
#[path = "provider_panels_event.rs"]
mod event_validation;

use canonical::host_local_canonical_bytes;
use event_validation::{matching_declaration, validate_event_against_snapshot};

// ---------------------------------------------------------------------------
// Inclusive bounds and fixed-point rate constants
// ---------------------------------------------------------------------------

/// The single panel model schema version this reducer accepts.
pub const MODEL_SCHEMA: u64 = 1;

/// Maximum inclusive byte count of a snapshot payload (original UTF-8 JSON).
pub const SNAPSHOT_MAX_BYTES: u64 = 524_288;

/// Maximum inclusive canonical-JSON byte count of host-local state.
pub const HOST_LOCAL_MAX_BYTES: usize = 65_536;

/// Token-bucket burst capacity and initial credit, in whole tokens.
pub const TOKEN_CAPACITY: u64 = 40;

/// Token-bucket steady-state refill rate, in whole tokens per second.
pub const TOKEN_REFILL_PER_SEC: u64 = 20;

/// One whole token, in milli-tokens.
const MILLI_PER_TOKEN: u64 = 1_000;

/// Bucket capacity, in milli-tokens.
const CAPACITY_MILLI: u64 = TOKEN_CAPACITY * MILLI_PER_TOKEN;

/// Refill rate, in milli-tokens per millisecond (20 tokens/s = 20 milli/ms).
const REFILL_MILLI_PER_MS: u64 = 20;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// The exact seven-state panel lifecycle.
///
/// Panel lifecycle is in-memory only and never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelLifecycle {
    /// Declared; not yet activated.
    Declared,
    /// Activated; awaiting the first snapshot.
    Activating,
    /// At least one snapshot accepted.
    Active,
    /// Logically unsubscribed; may resume with a fresh generation.
    Suspended,
    /// The last candidate was invalid; the prior model is retained as stale.
    Failed,
    /// Disposing (transient host intent).
    Disposing,
    /// Permanently gone; never mutates again.
    Disposed,
}

impl PanelLifecycle {
    /// Whether a snapshot may be received in this state.
    const fn receives_snapshot(self) -> bool {
        matches!(self, Self::Activating | Self::Active | Self::Failed)
    }

    /// Whether suspend is a legal transition from this state.
    const fn can_suspend(self) -> bool {
        matches!(self, Self::Activating | Self::Active | Self::Failed)
    }

    /// Whether host-local updates are accepted in this state.
    const fn is_live_or_suspended(self) -> bool {
        matches!(
            self,
            Self::Activating | Self::Active | Self::Suspended | Self::Failed
        )
    }

    /// Whether a host-driven dispose should stage a deactivate effect.
    const fn dispose_sends_effect(self) -> bool {
        matches!(
            self,
            Self::Activating | Self::Active | Self::Suspended | Self::Failed
        )
    }
}

// ---------------------------------------------------------------------------
// Event declaration schema (D1 cutover)
// ---------------------------------------------------------------------------

/// The closed set of semantic panel event kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// An item was selected.
    Selected,
    /// An item was activated.
    Activated,
    /// An affordance action was triggered.
    Action,
    /// A form field value changed.
    FieldChanged,
    /// The form was submitted.
    Submit,
    /// A new page was requested.
    PageRequested,
    /// Retry was requested.
    Retry,
    /// Cancel was requested.
    Cancel,
    /// A link was selected.
    LinkSelected,
    /// A tree node's expansion state changed.
    ExpansionChanged,
}

/// One manifest-declared allowed event with its argument field grammar.
///
/// The full event schema is caller-supplied because the manifest
/// `event_schema` cutover is wired in a later slice; the reducer validates
/// events only against the declarations it receives here.
#[derive(Debug, Clone)]
pub struct EventDeclaration {
    /// The declared event kind.
    pub kind: EventKind,
    /// Declared argument fields (may be empty).
    pub arguments: Vec<Field>,
}

// ---------------------------------------------------------------------------
// Effect descriptors (pure values returned from the reducer)
// ---------------------------------------------------------------------------

/// A pure `activate-panel` effect descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateEffect {
    /// The exact owning plugin.
    pub owner: Id,
    /// The panel instance being activated.
    pub panel_instance: PanelInstanceId,
    /// The owning screen instance.
    pub screen_instance: u64,
    /// The owner-declared panel type.
    pub panel_type: Id,
    /// Activation parameters.
    pub activation: TypedMap,
    /// Prior bounded host-local state, if resuming.
    pub prior_host_local: Option<HostLocal>,
    /// The fresh panel activation generation.
    pub generation: u64,
}

/// A pure `deactivate-panel` effect descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeactivateEffect {
    /// The exact owning plugin.
    pub owner: Id,
    /// The panel instance being deactivated.
    pub panel_instance: PanelInstanceId,
    /// The panel activation generation.
    pub generation: u64,
    /// Why the panel is being deactivated.
    pub reason: DeactivateReason,
}

/// A pure `panel-event` effect descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelEventEffect {
    /// The owning plugin.
    pub owner: Id,
    /// The panel instance the event targets.
    pub panel_instance: PanelInstanceId,
    /// The panel activation generation.
    pub generation: u64,
    /// The snapshot revision the event was raised against.
    pub revision: u64,
    /// The validated semantic event.
    pub event: PanelEvent,
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// Result of declaring a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclareOutcome {
    /// The allocated instance identity.
    pub instance: PanelInstanceId,
}

/// Result of activating/resuming/retrying a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivateOutcome {
    /// The activate effect to deliver.
    pub effect: ActivateEffect,
}

/// Result of a host-driven dispose/replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeactivateOutcome {
    /// A deactivate effect was staged for delivery.
    Sent(DeactivateEffect),
    /// No effect was staged (the panel had no provider binding).
    None,
}

/// Result of accepting a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptOutcome {
    /// The accepted revision.
    pub revision: u64,
}

/// Result of submitting a semantic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    /// A typed panel-event effect was staged.
    Event(PanelEventEffect),
    /// A retry from `Failed` staged a fresh activate.
    Activate(ActivateEffect),
    /// The event was rejected (zero effect, zero mutation).
    None,
}

// ---------------------------------------------------------------------------
// Command inputs
// ---------------------------------------------------------------------------

/// Input for declaring a panel.
#[derive(Debug, Clone, Copy)]
pub struct DeclareInput<'a> {
    /// The host-side owner plugin.
    pub owner: &'a Id,
    /// Descriptor identity of this panel within its screen.
    pub panel_id: &'a PanelId,
    /// The owning screen instance.
    pub screen_instance_id: u64,
    /// The owner-declared panel type.
    pub panel_type: &'a Id,
    /// Activation parameters.
    pub activation: &'a TypedMap,
    /// Snapshot body kinds declared by the exact selected manifest.
    pub allowed_model_kinds: &'a [BodyKind],
    /// Semantic events declared by the exact selected manifest.
    pub allowed_events: &'a [EventDeclaration],
    /// Owner-declared action ids that snapshot affordances may reference.
    pub action_authority: &'a [ActionId],
    /// The fixed provider-process generation.
    pub process_generation: u64,
}

/// Input for accepting a provider snapshot.
#[derive(Debug, Clone, Copy)]
pub struct AcceptSnapshot<'a> {
    /// The host-side owner plugin.
    pub owner: &'a Id,
    /// The provider-process generation carried by the snapshot.
    pub received_process_generation: u64,
    /// The exact original payload JSON byte count.
    pub payload_byte_count: u64,
    /// Monotonic elapsed milliseconds injected by the caller.
    pub elapsed_ms: u64,
    /// The typed panel snapshot.
    pub snapshot: &'a PanelSnapshot,
}

/// Input for submitting a semantic panel event.
#[derive(Debug, Clone)]
struct SubmitEvent<'a> {
    /// The panel instance the event targets.
    pub panel: PanelInstanceId,
    /// The host-side owner plugin.
    pub owner: &'a Id,
    /// The provider-process generation carried by the event.
    pub received_process_generation: u64,
    /// The panel activation generation.
    pub generation: u64,
    /// The snapshot revision the event was raised against.
    pub revision: u64,
    /// The semantic event.
    pub event: PanelEvent,
    /// The manifest-declared allowed events for this panel.
    pub allowed_events: &'a [EventDeclaration],
}

// ---------------------------------------------------------------------------
// Errors (redaction-safe, PLG-E502)
// ---------------------------------------------------------------------------

/// Rejected provider-panel reducer transition.
///
/// Every protocol/model variant carries `PLG-E502` in its display and never
/// echoes the offending value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelError {
    /// No panel matched the supplied identity.
    UnknownPanel,
    /// The panel instance is permanently disposed.
    Disposed,
    /// The lifecycle transition is illegal in the current state.
    InvalidLifecycle,
    /// The owner plugin does not match the panel.
    OwnerMismatch,
    /// The provider-process generation does not match.
    ProcessGenerationMismatch,
    /// The panel activation generation does not match.
    GenerationMismatch,
    /// The snapshot revision does not match the expected sequence.
    RevisionMismatch,
    /// The panel model schema does not match.
    ModelMismatch,
    /// The well-formed snapshot exceeded the per-generation rate limit.
    RateLimited,
    /// The injected monotonic clock moved backwards.
    ClockRegression,
    /// The host-local state exceeds the canonical byte limit.
    HostLocalTooLarge,
    /// The candidate snapshot failed model validation; the prior model is stale.
    SnapshotInvalid,
    /// The panel generation counter exhausted.
    GenerationExhausted,
    /// The process-global panel instance identity space is exhausted.
    InstanceIdentityExhausted,
}

impl std::fmt::Display for PanelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPanel => formatter.write_str("unknown panel instance (PLG-E502)"),
            Self::Disposed => formatter.write_str("panel instance is disposed (PLG-E502)"),
            Self::InvalidLifecycle => {
                formatter.write_str("panel lifecycle transition is invalid (PLG-E502)")
            }
            Self::OwnerMismatch => formatter.write_str("owner does not match the panel (PLG-E502)"),
            Self::ProcessGenerationMismatch => {
                formatter.write_str("provider process generation mismatch (PLG-E502)")
            }
            Self::GenerationMismatch => {
                formatter.write_str("panel activation generation mismatch (PLG-E502)")
            }
            Self::RevisionMismatch => formatter.write_str("snapshot revision mismatch (PLG-E502)"),
            Self::ModelMismatch => formatter.write_str("panel model schema mismatch (PLG-E502)"),
            Self::RateLimited => {
                formatter.write_str("panel snapshot exceeded rate limit (PLG-E502)")
            }
            Self::ClockRegression => {
                formatter.write_str("monotonic clock regression detected (PLG-E502)")
            }
            Self::HostLocalTooLarge => {
                formatter.write_str("host-local state exceeds size limit (PLG-E502)")
            }
            Self::SnapshotInvalid => {
                formatter.write_str("panel snapshot failed model validation (PLG-E502)")
            }
            Self::GenerationExhausted => {
                formatter.write_str("PLG-E502: panel generation counter exhausted")
            }
            Self::InstanceIdentityExhausted => {
                formatter.write_str("PLG-E502: panel instance identity space exhausted")
            }
        }
    }
}

impl std::error::Error for PanelError {}

// ---------------------------------------------------------------------------
// Per-generation token bucket (fixed-point, no wall clock)
// ---------------------------------------------------------------------------

/// Deterministic token bucket carrying fractional milli-token credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TokenBucket {
    credit_milli: u64,
    last_elapsed_ms: Option<u64>,
}

impl TokenBucket {
    /// A full bucket at capacity, with no prior observation.
    const fn fresh() -> Self {
        Self {
            credit_milli: CAPACITY_MILLI,
            last_elapsed_ms: None,
        }
    }

    /// Consume one token after applying the injected-elapsed refill.
    ///
    /// Returns the typed error without mutating `last_elapsed_ms` on clock
    /// regression; on rate-limit the observation time advances so later
    /// snapshots refill from the correct instant.
    fn consume(&mut self, elapsed_ms: u64) -> Result<(), PanelError> {
        if let Some(prev) = self.last_elapsed_ms {
            if elapsed_ms < prev {
                return Err(PanelError::ClockRegression);
            }
            let refill = (elapsed_ms - prev).saturating_mul(REFILL_MILLI_PER_MS);
            self.credit_milli = self.credit_milli.saturating_add(refill).min(CAPACITY_MILLI);
        }
        self.last_elapsed_ms = Some(elapsed_ms);
        if self.credit_milli < MILLI_PER_TOKEN {
            return Err(PanelError::RateLimited);
        }
        self.credit_milli -= MILLI_PER_TOKEN;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Accepted model
// ---------------------------------------------------------------------------

/// The complete accepted snapshot for one generation, with revision and staleness.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AcceptedModel {
    snapshot: PanelSnapshot,
    revision: u64,
    stale: bool,
}

// ---------------------------------------------------------------------------
// Panel record
// ---------------------------------------------------------------------------

/// One panel's in-memory state.
#[derive(Debug, Clone)]
struct PanelRecord {
    id: PanelInstanceId,
    owner: Id,
    panel_id: PanelId,
    screen_instance_id: u64,
    panel_type: Id,
    activation: TypedMap,
    allowed_model_kinds: Vec<BodyKind>,
    allowed_events: Vec<EventDeclaration>,
    action_authority: Vec<ActionId>,
    process_generation: u64,
    lifecycle: PanelLifecycle,
    generation: u64,
    expected_revision: u64,
    accepted: Option<AcceptedModel>,
    host_local: Option<HostLocal>,
    bucket: TokenBucket,
}

impl PanelRecord {
    /// Construct a fresh declared panel.
    fn new(id: PanelInstanceId, command: DeclareInput) -> Self {
        Self {
            id,
            owner: command.owner.clone(),
            panel_id: *command.panel_id,
            screen_instance_id: command.screen_instance_id,
            panel_type: command.panel_type.clone(),
            activation: command.activation.clone(),
            allowed_model_kinds: command.allowed_model_kinds.to_vec(),
            allowed_events: command.allowed_events.to_vec(),
            action_authority: command.action_authority.to_vec(),
            process_generation: command.process_generation,
            lifecycle: PanelLifecycle::Declared,
            generation: 0,
            expected_revision: 1,
            accepted: None,
            host_local: None,
            bucket: TokenBucket::fresh(),
        }
    }
}

// ---------------------------------------------------------------------------
// Reducer
// ---------------------------------------------------------------------------

/// Pure, in-memory provider-panel lifecycle reducer.
///
/// Owns bounded panel identity, lifecycle, generation, accepted model,
/// revision, rate state, and host-local presentation state. No process handle,
/// pipe, clock, or persisted field lives here.
#[derive(Debug, Clone)]
pub struct ProviderPanelState {
    panels: Vec<PanelRecord>,
}

impl Default for ProviderPanelState {
    fn default() -> Self {
        Self::new()
    }
}

include!("provider_panels_ops.rs");
