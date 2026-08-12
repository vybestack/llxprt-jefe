//! Closed envelope and payload data-transfer objects for the action-provider
//! protocol (issue #390 CW-10, Slice A).
//!
//! Pure data types only: no framing, parsing, process, state, effect, or
//! persistence. Each struct mirrors exactly the closed field set its wire
//! object admits; the readers in [`super::payload_reader`] enforce those sets.

use std::collections::BTreeMap;
use std::fmt;

use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::Field;
use crate::domain::{CanonicalSemver, Id, TypedMap};

use super::identifiers::{EnvName, MessageKind, RequestId};

/// A `hello` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloPayload {
    /// Host API identifier.
    pub host_api: String,
    /// The plugin package being driven.
    pub plugin_id: Id,
    /// The plugin package version.
    pub plugin_version: CanonicalSemver,
}

/// A `hello-ack` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloAckPayload {
    /// Provider-declared name.
    pub provider_name: String,
}

/// A `configure` payload.
///
/// [`Debug`] is hand-written rather than derived; see the impl below.
#[derive(Clone, PartialEq, Eq)]
pub struct ConfigurePayload {
    /// Selected configuration version.
    pub config_version: u64,
    /// Resolved configuration values.
    pub config: TypedMap,
    /// Resolved secret values keyed by their owning environment binding.
    pub secrets: BTreeMap<EnvName, String>,
    /// Declared non-secret environment bindings.
    pub environment: BTreeMap<EnvName, String>,
}

/// [`Debug`] is written by hand so a resolved secret cannot reach a log.
///
/// The redactor scrubs provider-*authored* surfaces, but it never sees the
/// payload the host builds. This type is embedded in `ProviderMessage` and in
/// `PersistentCandidate`, so a single `{:?}` on either — in a log line, a
/// diagnostic, or a panic message — would print every secret in cleartext.
/// Binding names are kept because they are declarations, not values, and they
/// are what an operator needs in order to debug a missing secret (CW10-14).
impl fmt::Debug for ConfigurePayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigurePayload")
            .field("config_version", &self.config_version)
            .field("config", &self.config)
            .field("secrets", &RedactedSecrets(&self.secrets))
            .field("environment", &self.environment)
            .finish()
    }
}

/// Renders secret bindings without their values.
struct RedactedSecrets<'a>(&'a BTreeMap<EnvName, String>);

impl fmt::Debug for RedactedSecrets<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.0.keys().map(|binding| (binding, "<redacted>")))
            .finish()
    }
}

/// A `ready` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyPayload {
    /// Declared capabilities.
    pub capabilities: Vec<Capability>,
}

/// The `context` object inside `invoke-action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeContext {
    /// Screen the action was invoked from.
    pub screen_id: Id,
    /// Screen instance the action was invoked from.
    pub screen_instance: Id,
    /// Resource references currently in view.
    pub resource_refs: TypedMap,
}

/// The optional `continuation` object inside `invoke-action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    /// The host-issued confirmation id.
    pub confirmation_id: Id,
    /// Whether the operator approved.
    pub approved: bool,
    /// Continuation values.
    pub values: TypedMap,
}

/// An `invoke-action` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeActionPayload {
    /// One invocation identifier.
    pub invocation_id: Id,
    /// The action being invoked.
    pub action_id: ActionId,
    /// Collected arguments.
    pub arguments: TypedMap,
    /// Invocation context.
    pub context: InvokeContext,
    /// Continuation, present only for the second invocation of a confirmation.
    pub continuation: Option<Continuation>,
}

/// A `cancel` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelPayload {
    /// The in-flight request to cancel.
    pub target_request_id: RequestId,
}

/// A `progress` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressPayload {
    /// Monotonic sequence number.
    pub sequence: u16,
    /// Operator-facing progress text.
    pub message: String,
    /// Optional completed count.
    pub completed: Option<u64>,
    /// Optional total count.
    pub total: Option<u64>,
}

/// One field error inside an `error` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// Dotted path to the offending argument field.
    pub path: String,
    /// Why the field was rejected.
    pub message: String,
}

/// An `error` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPayload {
    /// Provider error code.
    pub code: String,
    /// Operator-facing message.
    pub message: String,
    /// Whether retrying might succeed.
    pub retryable: bool,
    /// Per-field errors, bounded at the CW10-06 limit.
    pub field_errors: Vec<FieldError>,
}

/// A `shutdown` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownPayload {
    /// Why the host is shutting the provider down.
    pub reason: ShutdownReason,
}

/// A provider-declared capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The provider contributes actions.
    Actions,
    /// The provider contributes panels.
    Panels,
}

impl Capability {
    /// Every capability, in declaration order.
    pub const ALL: [Self; 2] = [Self::Actions, Self::Panels];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Actions => "actions",
            Self::Panels => "panels",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == value)
    }
}

/// A notice severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informational.
    Info,
    /// Cautionary.
    Warning,
}

impl Severity {
    /// Every severity, in declaration order.
    pub const ALL: [Self; 2] = [Self::Info, Self::Warning];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|severity| severity.as_str() == value)
    }
}

/// A shutdown reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
    /// Normal completion.
    Completed,
    /// Operator cancellation.
    Cancelled,
    /// The host is exiting.
    HostExit,
    /// The provider failed.
    Failure,
}

impl ShutdownReason {
    /// Every reason, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Completed,
        Self::Cancelled,
        Self::HostExit,
        Self::Failure,
    ];

    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::HostExit => "host-exit",
            Self::Failure => "failure",
        }
    }

    /// Resolve a wire name, exactly and case-sensitively.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == value)
    }
}

/// The four-kind outcome a provider may return for a one-shot action.
///
/// Panel snapshots, panel lifecycle, and configuration migration are no longer
/// carried as outcomes: they are direct, typed messages (issue #391). The
/// `ReplacePanel`, `ClosePanel`, and `MigratedConfig` placeholders have been
/// removed; the panel/migration paths are the sole paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Navigate to a declared route.
    Navigate {
        /// The declared route id.
        route_id: Id,
        /// Activation parameters.
        activation: TypedMap,
    },
    /// Refresh the current resource.
    Refresh {
        /// The resource reference.
        resource_ref: TypedMap,
    },
    /// Show a notice.
    Notice {
        /// Notice severity.
        severity: Severity,
        /// Notice text.
        message: String,
    },
    /// Ask the host to confirm a continuation.
    RequestHostConfirmation {
        /// The single-use confirmation id.
        confirmation_id: Id,
        /// Modal title.
        title: String,
        /// Modal body.
        body: String,
        /// Confirm button label.
        confirm_label: String,
        /// Whether the confirmed action is destructive.
        destructive: bool,
        /// Fields to collect on confirmation.
        continuation_schema: Vec<Field>,
    },
}

/// The fully parsed message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    /// The validated request id.
    pub request_id: RequestId,
    /// The fixed positive generation.
    pub generation: u64,
    /// Exact UTF-8 source bytes occupied by the payload JSON value.
    pub payload_byte_count: usize,
    /// The typed message body.
    pub message: ProviderMessage,
}

impl ParsedMessage {
    /// The message kind.
    #[must_use]
    pub fn kind(&self) -> MessageKind {
        self.message.kind()
    }
}

/// The seventeen closed message bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderMessage {
    /// `hello`.
    Hello(HelloPayload),
    /// `hello-ack`.
    HelloAck(HelloAckPayload),
    /// `configure`.
    Configure(ConfigurePayload),
    /// `ready`.
    Ready(ReadyPayload),
    /// `invoke-action`.
    InvokeAction(InvokeActionPayload),
    /// `cancel`.
    Cancel(CancelPayload),
    /// `progress`.
    Progress(ProgressPayload),
    /// `outcome`.
    Outcome(Outcome),
    /// `error`.
    Error(ErrorPayload),
    /// `shutdown`.
    Shutdown(ShutdownPayload),
    /// `shutdown-ack`.
    ShutdownAck,
    /// `activate-panel` (issue #391).
    ActivatePanel(super::panel_model::ActivatePanelPayload),
    /// `deactivate-panel` (issue #391).
    DeactivatePanel(super::panel_model::DeactivatePanelPayload),
    /// `panel-event` (issue #391).
    PanelEvent(super::panel_model::PanelEventPayload),
    /// `panel-snapshot` (issue #391).
    PanelSnapshot(super::panel_model::PanelSnapshot),
    /// `migrate-config` (issue #391).
    MigrateConfig(super::panel_model::MigrateConfigPayload),
    /// `migrated-config` (issue #391).
    MigratedConfig(super::panel_model::MigratedConfigPayload),
}

impl ProviderMessage {
    /// The message kind.
    #[must_use]
    pub fn kind(&self) -> MessageKind {
        match self {
            Self::Hello(_) => MessageKind::Hello,
            Self::HelloAck(_) => MessageKind::HelloAck,
            Self::Configure(_) => MessageKind::Configure,
            Self::Ready(_) => MessageKind::Ready,
            Self::InvokeAction(_) => MessageKind::InvokeAction,
            Self::Cancel(_) => MessageKind::Cancel,
            Self::Progress(_) => MessageKind::Progress,
            Self::Outcome(_) => MessageKind::Outcome,
            Self::Error(_) => MessageKind::Error,
            Self::Shutdown(_) => MessageKind::Shutdown,
            Self::ShutdownAck => MessageKind::ShutdownAck,
            Self::ActivatePanel(_) => MessageKind::ActivatePanel,
            Self::DeactivatePanel(_) => MessageKind::DeactivatePanel,
            Self::PanelEvent(_) => MessageKind::PanelEvent,
            Self::PanelSnapshot(_) => MessageKind::PanelSnapshot,
            Self::MigrateConfig(_) => MessageKind::MigrateConfig,
            Self::MigratedConfig(_) => MessageKind::MigratedConfig,
        }
    }
}
