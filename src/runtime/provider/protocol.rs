//! Public DTO and state-machine API for the closed action-provider protocol
//! (issue #390 CW-10, Slice A).
//!
//! This module is the single public surface of the protocol wire boundary
//! shared by the (later) handle-free request reducer and the (later) process
//! supervisor. It maps the shared bounded reader's ordered tree onto a closed
//! set of strongly typed message DTOs: every object denies unknown fields and
//! (via the reader) duplicate keys at every nesting level, and request ids,
//! stream direction, and the fixed positive generation are validated by hand.
//! It also exposes the two pure validators the reducer and supervisor drive
//! later: the progress sequence/count/total monotonicity check and the
//! handshake lifecycle order check.
//!
//! The implementation is split across cohesive private sibling modules under
//! [`super`] and re-exported here so the public path
//! (`crate::runtime::provider::protocol::`) is stable:
//!
//! - [`identifiers`] — stream direction, closed message kinds, request ids, and
//!   environment-variable names.
//! - [`dto`] — the closed envelope and payload data-transfer objects.
//! - [`payload_reader`] — the [`parse_message`] entry point and payload
//!   decoding.
//! - [`progress`] — the progress monotonicity validator.
//! - [`lifecycle`] — the handshake and operation order validator.
//!
//! No process, application state, effect, or persistence lives in this layer.
//! The only JSON architecture is the shared bounded reader.
//!
//! [`identifiers`]: super::identifiers
//! [`dto`]: super::dto
//! [`payload_reader`]: super::payload_reader
//! [`progress`]: super::progress
//! [`lifecycle`]: super::lifecycle

// Re-export the domain value types this layer surfaces so consumers (and tests)
// reach them through the protocol module without depending on their origin path.
pub use crate::domain::{Id, TypedMap};

// Identifiers and validated strings.
pub use super::identifiers::{
    Direction, EnvName, EnvNameError, INITIAL_PROCESS_GENERATION, MessageKind, RequestId,
    RequestIdError, RequestOrigin,
};

// Closed envelope and payload DTOs.
pub use super::dto::{
    CancelPayload, Capability, ConfigurePayload, Continuation, ErrorPayload, FieldError,
    HelloAckPayload, HelloPayload, InvokeActionPayload, InvokeContext, Outcome, ParsedMessage,
    ProgressPayload, ProviderMessage, ReadyPayload, Severity, ShutdownPayload, ShutdownReason,
};

// Closed panel model and direct panel/migration DTOs (issue #391).
pub use super::panel_model::{
    ActivatePanelPayload, Affordance, BodyKind, DeactivatePanelPayload, DeactivateReason,
    DetailBody, DetailMetadata, DiffLineOrigin, EmptyBody, ErrorBody, FormBody, FormFieldError,
    HostLocal, ListBody, ListItem, MigrateConfigPayload, MigratedConfigPayload, PanelBody,
    PanelEvent, PanelEventPayload, PanelSnapshot, ProgressBody, StatusBody, StatusRow,
    StatusRowState, StructuredDiffBody, StructuredDiffFile, StructuredDiffHunk, StructuredDiffLine,
    StructuredDiffPath, TreeBody, TreeNode,
};

// Pure validators.
pub use super::lifecycle::{LifecycleOrder, LifecyclePhase};
pub use super::progress::ProgressTracker;

// Wire entry point.
pub use super::payload_reader::parse_message;
