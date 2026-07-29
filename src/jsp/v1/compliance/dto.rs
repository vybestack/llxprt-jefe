//! Closed typed DTOs for JSP/1 compliance artifacts.

use serde::Deserialize;
use serde_json::value::RawValue;

use crate::domain::observation::{EventRecord, HeartbeatRecord, ObservationIdentity};
use crate::jsp::Snapshot;
use crate::jsp::v1::error::JspError;

use super::projection::NormalizedProjection;

/// Exact nested JSON bytes retained until the authoritative JSP byte parser
/// enforces the per-document ingress bound and closed contract.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct DocumentWire(Box<RawValue>);

pub enum TypedDocument {
    Snapshot(Box<Snapshot>),
    Event(EventRecord),
    Heartbeat(HeartbeatRecord),
}

#[derive(Deserialize)]
struct DocumentKindProbe {
    kind: DocumentKindWire,
}

impl DocumentWire {
    pub fn from_raw(raw: Box<RawValue>) -> Self {
        Self(raw)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.get().as_bytes()
    }

    pub fn into_typed(self) -> Result<TypedDocument, JspError> {
        let bytes = self.as_bytes();
        if bytes.len() > crate::jsp::v1::limits::MAX_DOCUMENT_BYTES {
            return crate::jsp::v1::parse_snapshot(bytes)
                .map(|value| TypedDocument::Snapshot(Box::new(value)));
        }
        let probe: DocumentKindProbe = serde_json::from_slice(bytes)
            .map_err(|_| JspError::closed_shape("document: malformed or missing kind"))?;
        match probe.kind {
            DocumentKindWire::Snapshot => crate::jsp::v1::parse_snapshot(bytes)
                .map(Box::new)
                .map(TypedDocument::Snapshot),
            DocumentKindWire::Event => crate::jsp::v1::parse_event(bytes).map(TypedDocument::Event),
            DocumentKindWire::Heartbeat => {
                crate::jsp::v1::parse_heartbeat(bytes).map(TypedDocument::Heartbeat)
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerTraceWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: ProducerTraceKind,
    pub trace_artifact_version: String,
    pub adapter_version: String,
    pub challenge_nonce: u64,
    #[serde(rename = "description")]
    pub description: String,
    pub facts: Vec<ProducerFactWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerTraceKind {
    ProducerTrace,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerFactWire {
    pub fact: ProducerFactKind,
    #[serde(default)]
    pub now_ms: Option<u64>,
    #[serde(default)]
    pub document: Option<DocumentWire>,
    #[serde(default)]
    pub document_index: Option<usize>,
    #[serde(default)]
    pub forbidden_marker: Option<String>,
    #[serde(default)]
    pub source_handle: Option<String>,
    #[serde(default)]
    pub at_limit: Option<DocumentWire>,
    #[serde(default)]
    pub limit_plus_one: Option<DocumentWire>,
    #[serde(default)]
    pub sink: Option<SinkKind>,
    #[serde(default)]
    pub queue_capacity: Option<u64>,
    #[serde(default)]
    pub attempted: Option<u64>,
    #[serde(default)]
    pub accepted: Option<u64>,
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    #[serde(default)]
    pub operation_handle: Option<String>,
    #[serde(default)]
    pub operation_handles: Option<Vec<String>>,
    #[serde(default)]
    pub emitted_through: Option<u64>,
    #[serde(default)]
    pub dropped_start: Option<u64>,
    #[serde(default)]
    pub dropped_end: Option<u64>,
    #[serde(default)]
    pub next_emitted: Option<u64>,
    #[serde(default)]
    pub next_publication: Option<DocumentWire>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerFactKind {
    ClockSet,
    Document,
    RedactionChallenge,
    DraftChallenge,
    BoundChallenge,
    NonblockingChallenge,
    GapChallenge,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SinkKind {
    Blocked,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerTranscriptWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: ServerTranscriptKind,
    pub transcript_artifact_version: String,
    pub server_version: String,
    pub challenge_nonce: u64,
    #[serde(rename = "description")]
    pub description: String,
    pub interactions: Vec<ServerInteractionWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerTranscriptKind {
    ServerTranscript,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerInteractionWire {
    #[serde(rename = "name")]
    pub _name: String,
    pub request: ServerRequestWire,
    #[serde(default)]
    pub response: Option<ServerResponseWire>,
    #[serde(default)]
    pub stream: Option<Vec<StreamItemWire>>,
    #[serde(rename = "assert")]
    pub _assertion: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerRequestWire {
    pub route: RouteWire,
    pub credential_handle: String,
    pub principal_handle: String,
    pub method: MethodWire,
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
    #[serde(default)]
    pub body: Option<Box<RawValue>>,
}

impl ServerRequestWire {
    pub fn identity(&self) -> Result<ObservationIdentity, JspError> {
        crate::jsp::v1::validate::build_event_identity(
            &self.agent_id,
            self.lifecycle_generation,
            &self.source_epoch,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum RouteWire {
    #[serde(rename = "/jsp/1/register")]
    Register,
    #[serde(rename = "/jsp/1/publish")]
    Publish,
    #[serde(rename = "/jsp/1/heartbeat")]
    Heartbeat,
    #[serde(rename = "/jsp/1/observe")]
    Observe,
    #[serde(rename = "/jsp/1/control")]
    Control,
    #[serde(rename = "/jsp/1/internal/lease_expired")]
    LeaseExpired,
    #[serde(rename = "/jsp/1/internal/observation_digest")]
    ObservationDigest,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleWire {
    Publisher,
    Observer,
    Server,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MethodWire {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequestWire {
    pub now_ms: u64,
    pub lease_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerResponseWire {
    pub status: u16,
    pub kind: ResponseKindWire,
    #[serde(default)]
    pub body: Option<ResponseBodyWire>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKindWire {
    Registered,
    Accepted,
    DuplicateNoop,
    OutOfOrderNoop,
    GapRejectedFreshStreamRequired,
    FreshSnapshotAccepted,
    UnrelatedAgentRejected,
    StaleGenerationRejected,
    StaleEpochRejected,
    ForbiddenRole,
    UnknownAuthentication,
    ForbiddenBinding,
    CanonicalObservation,
    ObservationHealthStale,
    BoundExceeded,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ResponseBodyWire {
    Binding(BindingWire),
    Rejection(RejectionWire),
    Health(HealthWire),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingWire {
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
}

impl BindingWire {
    pub fn identity(&self) -> Result<ObservationIdentity, JspError> {
        crate::jsp::v1::validate::build_event_identity(
            &self.agent_id,
            self.lifecycle_generation,
            &self.source_epoch,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectionWire {
    pub reason: RejectionReasonWire,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReasonWire {
    SequenceGapFreshSnapshotRequired,
    FreshSnapshotRequired,
    UnrelatedAgent,
    StaleLifecycleGeneration,
    StaleSourceEpoch,
    PayloadExceedsBound,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthWire {
    pub observation_health: HealthValueWire,
    pub native_activity: ActivityValueWire,
    #[serde(default)]
    pub activity_availability: Option<super::projection::AvailabilityProjection>,
    #[serde(default)]
    pub activity_provenance: Option<super::projection::ProjectionProvenance>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthValueWire {
    Stale,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityValueWire {
    Idle,
    Thinking,
    Acting,
    Unsupported,
    Unknown,
    Degraded,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamItemWire {
    pub kind: DocumentKindWire,
    pub document: DocumentWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: ScenarioKind,
    pub id: String,
    pub name: String,
    #[serde(rename = "description")]
    pub _description: String,
    pub base_snapshot: DocumentWire,
    pub steps: Vec<ScenarioStepWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    Scenario,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioStepWire {
    pub kind: ScenarioStepKind,
    pub index: u64,
    #[serde(default)]
    pub document: Option<DocumentWire>,
    #[serde(default)]
    pub expected: Option<NormalizedProjection>,
    #[serde(default)]
    pub expected_gap_signal: Option<bool>,
    #[serde(default)]
    pub expected_rejected_identity: Option<bool>,
    #[serde(default)]
    pub expected_illegal_transition: Option<bool>,
    #[serde(default)]
    pub permanent: Option<bool>,
    #[serde(default)]
    pub expected_primary: Option<NormalizedProjection>,
    #[serde(default)]
    pub expected_secondary: Option<NormalizedProjection>,
    #[serde(default)]
    pub alive: Option<bool>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStepKind {
    Event,
    Heartbeat,
    EventDuplicate,
    EventLower,
    EventBeforeSnapshot,
    HeartbeatBeforeSnapshot,
    EventAfterFreshRequired,
    HeartbeatAfterFreshRequired,
    MalformedCapabilities,
    EventGap,
    EventEpochMismatch,
    EventAfterSessionEnded,
    ToolTerminalRegression,
    DraftExcluded,
    TransportDisconnect,
    FreshSnapshot,
    ParallelSnapshot,
    ProcessLiveness,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioManifestWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: ScenarioManifestKind,
    pub scenario_artifact_version: String,
    #[serde(rename = "description")]
    pub _description: String,
    pub scenarios: Vec<ScenarioManifestEntryWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioManifestKind {
    ScenarioManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioManifestEntryWire {
    pub id: String,
    pub name: String,
    pub file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaManifestWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: SchemaManifestKind,
    pub schema_artifact_version: String,
    #[serde(rename = "description")]
    pub description: String,
    pub schemas: Vec<SchemaEntryWire>,
    pub cases: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaManifestKind {
    SchemaManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaEntryWire {
    pub kind: DocumentKindWire,
    pub file: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKindWire {
    Snapshot,
    Event,
    Heartbeat,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaCasesWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub _kind: SchemaCasesKind,
    pub cases: Vec<SchemaCaseWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaCasesKind {
    SchemaCases,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaCaseWire {
    pub name: String,
    pub document_kind: DocumentKindWire,
    pub event_type: Option<EventTypeWire>,
    pub file: String,
    pub expected: CaseExpectedWire,
    pub expected_code: Option<ExpectedCodeWire>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseExpectedWire {
    Ok,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ExpectedCodeWire {
    #[serde(rename = "JSP-E001")]
    E001,
    #[serde(rename = "JSP-E002")]
    E002,
    #[serde(rename = "JSP-E003")]
    E003,
    #[serde(rename = "JSP-E004")]
    E004,
    #[serde(rename = "JSP-E005")]
    E005,
    #[serde(rename = "JSP-E006")]
    E006,
}

impl ExpectedCodeWire {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::E001 => "JSP-E001",
            Self::E002 => "JSP-E002",
            Self::E003 => "JSP-E003",
            Self::E004 => "JSP-E004",
            Self::E005 => "JSP-E005",
            Self::E006 => "JSP-E006",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EventTypeWire {
    #[serde(rename = "activity.changed")]
    ActivityChanged,
    #[serde(rename = "wait.opened")]
    WaitOpened,
    #[serde(rename = "wait.resolved")]
    WaitResolved,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.ended")]
    TurnEnded,
    #[serde(rename = "todos.replaced")]
    TodosReplaced,
    #[serde(rename = "tool_call.created")]
    ToolCallCreated,
    #[serde(rename = "tool_call.phase_changed")]
    ToolCallPhaseChanged,
    #[serde(rename = "assistant_message.displayed")]
    AssistantMessageDisplayed,
    #[serde(rename = "source.error")]
    SourceError,
    #[serde(rename = "session.ended")]
    SessionEnded,
}

// ---------------------------------------------------------------------------
// Top-level compliance manifest DTOs
// ---------------------------------------------------------------------------

/// The top-level compliance manifest. Pins the compliance artifact version,
/// schema/scenario/trace paths, scenario count, and profile inventory.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComplianceManifestWire {
    pub schema: u64,
    #[serde(rename = "kind")]
    pub kind: ComplianceManifestKind,
    pub compliance_artifact_version: String,
    #[serde(rename = "description")]
    pub description: String,
    pub schemas: SchemasSectionWire,
    pub scenarios: ScenariosSectionWire,
    pub traces: TracesSectionWire,
    pub profiles: Vec<ProfileWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComplianceManifestKind {
    ComplianceManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemasSectionWire {
    pub index: String,
    pub documents: SchemasDocumentsWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemasDocumentsWire {
    pub snapshot: String,
    pub event: String,
    pub heartbeat: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenariosSectionWire {
    pub index: String,
    pub count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TracesSectionWire {
    pub producer: TraceEntryWire,
    pub server: TraceEntryWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEntryWire {
    pub contract: String,
    pub trace: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProfileWire {
    #[serde(rename = "schema")]
    Schema,
    #[serde(rename = "reducer")]
    Reducer,
    #[serde(rename = "producer")]
    Producer,
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "all")]
    All,
}
