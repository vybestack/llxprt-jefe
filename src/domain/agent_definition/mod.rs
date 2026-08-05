//! Closed four-agent definition contract (issue #382 CW-02).
//!
//! This module is the single source of truth for the agent-type registry,
//! probe spec, launch plan, and shipped four-agent definitions that replace
//! the closed `AgentTypeId` enum. Product tokens live only in [`shipped`].
//!
//! The public surface is the closed contract imported by tests and the
//! runtime/persistence/state/UI layers:
//! - [`AgentTypeId`], [`AgentDefinition`], [`ExecutableCandidate`]
//! - [`ProbeSpec`], [`AgentLaunchPlan`], [`Availability`], [`Support`]
//! - [`Operation`], [`Target`], [`OperationMatrix`], [`Preflight`]
//! - [`ProbeErrorCode`], [`RemoteTarget`], [`CandidateKind`]

pub mod bounded_json;
pub mod canonical;
pub mod definition;
pub mod diagnostics;
pub mod fields;
pub mod json_pointer;
pub mod limits;
pub mod normalize;
pub mod probe;
pub mod reader;
pub mod sha256;
pub mod shipped;
pub mod signature;
pub mod type_id;
pub mod types;
pub mod validation;

pub use definition::{AgentDefinition, DEFINITION_SCHEMA};
pub use diagnostics::{DefinitionError, FieldScope};
pub use fields::{Emitter, EmitterValidateError, Field, FieldKind, FieldValidateError, FieldValue};
pub use normalize::Normalize;
pub use probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeParseError, ProbeSpec, ProbeStream,
    ProbeValidateError,
};
pub use sha256::DefinitionSha256;
pub use signature::LaunchSignatureV1;
pub use type_id::{
    AgentTypeId, AgentTypeIdError, AgentTypeIdErrorReason, CandidateKind, CandidateValidateError,
    ExecutableCandidate,
};
pub use types::{
    AgentLaunchPlan, Availability, Operation, OperationMatrix, OperationSupport, Preflight,
    ProbeErrorCode, PromptShape, RemoteTarget, Support, Target, TargetMatrix, TargetSupport,
};
