//! Definition-driven immutable LOCAL `AgentLaunchPlan` generation
//! (issue #382 CW02-06 / S7).
//!
//! This is a **pure** planner: no I/O, no process spawn, no filesystem access,
//! no environment reads, and no side effects. It consumes a validated
//! [`AgentDefinition`], typed field values, a chosen [`Operation`], a local
//! canonical [`Target`], and compatible current probe evidence/generations,
//! and produces exactly one immutable [`AgentLaunchPlan`] — or zero effects
//! when the operation or target is unsupported.
//!
//! # Architectural boundaries
//!
//! The planner resolves operation and target support **before** any effect.
//! An unsupported operation or target returns [`PlanOutcome::Unsupported`]
//! with the exact declared reason; no argv, env, or signature is constructed.
//!
//! Argv is emitted element-by-element in declaration order from typed
//! emitters only. Each emitter contributes zero or more [`OsString`] elements,
//! preserving the typed value byte-for-byte. There is no shell template,
//! token splitting, or raw-argument field.
//!
//! The environment starts empty and receives only typed
//! [`Emitter::Environment`] pairs whose field value is present. tmux and
//! unrelated ambient variables are never consulted: this module does not read
//! the process environment.
//!
//! Product knowledge lives only in the shipped definition data (emitters,
//! capability tokens, operation/target declarations). This module contains no
//! product tokens and performs no product matching. The runtime may execute
//! a produced plan eventually, but it must not create a parallel authority or
//! product match.
//!
//! # Slice scope (S7)
//!
//! This slice implements local plan generation only. It does not implement
//! remote serialization, execution, stale recheck, preflight process effects,
//! fresh send orchestration, persistence, migration, or package-cache
//! generalization.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::agent_candidate_path::AgentWrapperKind;
use crate::domain::agent_definition::fields::{Emitter, FieldValue};
use crate::domain::agent_definition::sha256::DefinitionSha256;
use crate::domain::agent_definition::signature::LaunchSignatureV1;
use crate::domain::agent_definition::{
    AgentDefinition, AgentLaunchPlan, Availability, Operation, Preflight, ProbeErrorCode, Target,
};
use crate::domain::canonical_values::launch_target_fingerprint;

// ---------------------------------------------------------------------------
// Field value carrier
// ---------------------------------------------------------------------------

/// Form-field scope: repository-wide or per-agent.
///
/// This mirrors [`crate::domain::agent_definition::FieldScope`] but is defined
/// locally so the planner does not depend on the state layer's form model.
/// The integration layer maps its generated-form values into this carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldScope {
    /// Repository-scope field.
    Repository,
    /// Agent-scope field.
    Agent,
}

/// Typed field values keyed by `(scope, field-id)`.
///
/// A thin, owned carrier that the planner reads without side effects. Values
/// not present here fall back to the definition's declared defaults during
/// emission. Values referencing unknown field ids are rejected as typed errors
/// before any argv element is constructed.
#[derive(Debug, Clone, Default)]
pub struct LaunchFieldValues {
    repository: Vec<(String, FieldValue)>,
    agent: Vec<(String, FieldValue)>,
}

impl LaunchFieldValues {
    /// Construct an empty value set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a repository-scope field value.
    pub fn set_repository(&mut self, id: impl Into<String>, value: FieldValue) {
        let id = id.into();
        if let Some(slot) = self
            .repository
            .iter_mut()
            .find(|(existing, _)| *existing == id)
        {
            slot.1 = value;
        } else {
            self.repository.push((id, value));
        }
    }

    /// Set an agent-scope field value.
    pub fn set_agent(&mut self, id: impl Into<String>, value: FieldValue) {
        let id = id.into();
        if let Some(slot) = self.agent.iter_mut().find(|(existing, _)| *existing == id) {
            slot.1 = value;
        } else {
            self.agent.push((id, value));
        }
    }

    /// Look up a repository-scope field value by id.
    #[must_use]
    pub fn repository(&self, id: &str) -> Option<&FieldValue> {
        self.repository
            .iter()
            .find(|(existing, _)| existing == id)
            .map(|(_, value)| value)
    }

    /// Look up an agent-scope field value by id.
    #[must_use]
    pub fn agent(&self, id: &str) -> Option<&FieldValue> {
        self.agent
            .iter()
            .find(|(existing, _)| existing == id)
            .map(|(_, value)| value)
    }

    /// Whether a value was explicitly provided for the given `(scope, id)`.
    #[must_use]
    pub fn contains(&self, scope: FieldScope, id: &str) -> bool {
        match scope {
            FieldScope::Repository => self.repository(id).is_some(),
            FieldScope::Agent => self.agent(id).is_some(),
        }
    }

    /// All explicitly-provided `(scope, id)` keys, for unknown-value checks.
    fn provided_keys(&self) -> Vec<(FieldScope, &str)> {
        self.repository
            .iter()
            .map(|(id, _)| (FieldScope::Repository, id.as_str()))
            .chain(
                self.agent
                    .iter()
                    .map(|(id, _)| (FieldScope::Agent, id.as_str())),
            )
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Request / outcome / error
// ---------------------------------------------------------------------------

/// Immutable inputs to the local launch planner.
///
/// Every field is borrowed; the planner performs no allocation beyond the
/// produced plan's owned `argv`/`env`/`cwd`. The caller guarantees the
/// `definition` is validated and the `executable` is the resolved candidate
/// path (resolution belongs to the candidate resolver, not the planner).
#[derive(Debug, Clone)]
pub struct PlanRequest<'a> {
    /// Validated agent definition (authority for emitters, operations, targets).
    pub definition: &'a AgentDefinition,
    /// Chosen closed operation.
    pub operation: Operation,
    /// Chosen execution target (must be local for this planner).
    pub target: Target,
    /// Resolved executable path from the candidate resolver.
    pub executable: PathBuf,
    /// Full physical executable fingerprint captured before planning.
    pub executable_fingerprint: CandidateFingerprint,
    /// Platform launch strategy captured by candidate resolution.
    pub executable_wrapper: AgentWrapperKind,
    /// Structural package-runner prefix finalized before immutable planning.
    pub argv_prefix: Vec<OsString>,
    /// Current probe availability evidence.
    pub probe: Availability,
    /// Probe generation stamp to stamp onto the plan.
    pub probe_generation: u64,
    /// Target generation stamp to stamp onto the plan.
    pub target_generation: u64,
    /// Activation generation stamp compared by the execution authorization
    /// guard (issue #382 CW02-12 / S8). Defaulted to the probe generation so
    /// a single-generation compatible plan authorizes on the first attempt.
    pub activation_generation: u64,
    /// Typed field values for argv/env emission.
    pub values: &'a LaunchFieldValues,
    /// Sandbox preflight contract stamped onto the plan.
    pub preflight: Preflight,
}

/// The outcome of local plan generation.
#[derive(Debug)]
pub enum PlanOutcome {
    /// Exactly one immutable plan was produced.
    ///
    /// Boxed so the enum stays small regardless of the plan's argv/env size
    /// (the plan is ~440 bytes; the other variants are tens of bytes).
    Supported(Box<AgentLaunchPlan>),
    /// The operation or target is unsupported; zero effects.
    Unsupported {
        /// The exact declared reason shown to the user.
        reason: String,
    },
    /// A typed validation failure occurred before any effect.
    Error(AgentPlanError),
}

/// Typed planner error. Never panics; never performs side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPlanError {
    /// A field value references an id not declared by the definition.
    UnknownFieldValue {
        /// The unknown field id.
        field: String,
    },
    /// A field value's kind does not match the declared field kind.
    FieldKindMismatch {
        /// The field id.
        field: String,
    },
    /// The probe result is NotFound (no executable resolved).
    ProbeNotFound,
    /// The probe result is InstalledIncompatible.
    ProbeIncompatible {
        /// The probe's declared reason.
        reason: String,
    },
    /// The probe result is a ProbeError.
    ProbeError {
        /// The closed probe error code.
        code: ProbeErrorCode,
        /// The probe's declared reason.
        reason: String,
    },
    /// The requested probe generation does not match the availability stamp.
    ProbeGenerationMismatch {
        /// The generation the caller requested.
        plan: u64,
        /// The generation the probe evidence carries.
        probe: u64,
    },
    /// A non-local target was passed to the local planner.
    NotLocalTarget,
    /// An emitter references a field that resolved to no value and no default.
    MissingRequiredValue {
        /// The field id.
        field: String,
    },
}

impl std::fmt::Display for AgentPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFieldValue { field } => {
                write!(f, "value provided for undeclared field `{field}`")
            }
            Self::FieldKindMismatch { field } => {
                write!(f, "value kind does not match declared field `{field}`")
            }
            Self::ProbeNotFound => f.write_str("probe returned NotFound; no executable resolved"),
            Self::ProbeIncompatible { reason } => {
                write!(f, "probe incompatible: {reason}")
            }
            Self::ProbeError { code, reason } => {
                write!(f, "probe error {}: {reason}", code.as_str())
            }
            Self::ProbeGenerationMismatch { plan, probe } => {
                write!(f, "probe generation mismatch: plan={plan}, probe={probe}")
            }
            Self::NotLocalTarget => f.write_str("local planner received a non-local target"),
            Self::MissingRequiredValue { field } => {
                write!(f, "required field `{field}` has no value or default")
            }
        }
    }
}

impl std::error::Error for AgentPlanError {}

// ---------------------------------------------------------------------------
// Pure planner entry point
// ---------------------------------------------------------------------------

/// Produce one immutable local `AgentLaunchPlan`, or zero effects.
///
/// Resolution order (per the issue's deterministic algorithm #4):
///   1. target must be local;
///   2. operation support — unsupported returns the declared reason;
///   3. target support — unsupported returns the declared reason;
///   4. probe evidence must be `InstalledCompatible`;
///   5. probe generation must match the requested stamp;
///   6. typed field values validated against the definition;
///   7. argv/env emitted element-by-element in declaration order;
///   8. signature stamped.
///
/// # Errors
///
/// Returns [`PlanOutcome::Error`] for any typed validation failure, or
/// [`PlanOutcome::Unsupported`] when the operation or target is declared
/// unsupported. Both produce zero effects.
#[must_use]
pub fn plan_local_launch(request: &PlanRequest<'_>) -> PlanOutcome {
    match &request.target {
        Target::Local { .. } => plan_launch(request),
        Target::Remote(_) => PlanOutcome::Error(AgentPlanError::NotLocalTarget),
    }
}

/// Build a plan for the target carried by `request`.
///
/// Target-specific boundary modules perform local/remote shape checks before
/// calling this shared, side-effect-free planner.
pub(crate) fn plan_launch(request: &PlanRequest<'_>) -> PlanOutcome {
    let definition = request.definition;
    if let Some(reason) = support_reason(definition, request.operation, &request.target) {
        return PlanOutcome::Unsupported { reason };
    }
    if let Err(error) = validate_probe_evidence(request) {
        return PlanOutcome::Error(error);
    }
    if let Err(error) = validate_provided_values(definition, request.values) {
        return PlanOutcome::Error(error);
    }
    let emitted = match emit_argv_env(definition, request.values) {
        Ok(parts) => parts,
        Err(error) => return PlanOutcome::Error(error),
    };
    PlanOutcome::Supported(Box::new(assemble_plan(request, emitted)))
}

/// The exact unsupported reason for an operation/target pair, if any.
fn support_reason(
    definition: &AgentDefinition,
    operation: Operation,
    target: &Target,
) -> Option<String> {
    let operation_support = &definition.operations.support_for(operation).supported;
    if operation_support.is_unsupported() {
        return operation_support.reason().map_or_else(
            || Some("operation not supported".to_string()),
            |reason| Some(reason.to_string()),
        );
    }
    let target_support = match target {
        Target::Local { .. } => &definition.targets.local.supported,
        Target::Remote(_) => &definition.targets.remote.supported,
    };
    target_support.is_unsupported().then(|| {
        target_support
            .reason()
            .unwrap_or("target not supported")
            .to_string()
    })
}

/// Validate the probe evidence (steps 4-5): compatibility + generation match.
///
/// Returns `Ok(())` only when the probe is `InstalledCompatible` and its
/// generation stamp matches the requested stamp. Any other state yields the
/// matching typed error with zero effects.
fn validate_probe_evidence(request: &PlanRequest<'_>) -> Result<(), AgentPlanError> {
    match &request.probe {
        Availability::NotFound => Err(AgentPlanError::ProbeNotFound),
        Availability::InstalledIncompatible { reason, generation } => {
            check_generation(*generation, request.probe_generation)?;
            Err(AgentPlanError::ProbeIncompatible {
                reason: reason.clone(),
            })
        }
        Availability::ProbeError {
            code,
            reason,
            generation,
        } => {
            check_generation(*generation, request.probe_generation)?;
            Err(AgentPlanError::ProbeError {
                code: *code,
                reason: reason.clone(),
            })
        }
        Availability::InstalledCompatible { generation, .. } => {
            check_generation(*generation, request.probe_generation)
        }
    }
}

/// Assemble the immutable plan from validated inputs and emitted argv/env.
fn assemble_plan(request: &PlanRequest<'_>, mut emitted: EmittedEffects) -> AgentLaunchPlan {
    let definition = request.definition;
    let typed_value_hash = compute_typed_value_hash(definition, request.values);
    let target_fingerprint = launch_target_fingerprint(&request.target);
    let signature =
        LaunchSignatureV1::v1(definition.sha256(), typed_value_hash, target_fingerprint);
    let mut argv = request.argv_prefix.clone();
    argv.append(&mut emitted.argv);
    AgentLaunchPlan {
        type_id: definition.id.clone(),
        operation: request.operation,
        definition_sha256: definition.sha256(),
        executable: request.executable.clone(),
        executable_fingerprint: request.executable_fingerprint.clone(),
        executable_wrapper: request.executable_wrapper,
        argv,
        env: emitted.env,
        cwd: request.target.canonical_cwd().to_path_buf(),
        target: request.target.clone(),
        probe_generation: request.probe_generation,
        target_generation: request.target_generation,
        activation_generation: request.activation_generation,
        preflight: request.preflight.clone(),
        signature,
    }
}

/// Check that the probe evidence generation matches the requested stamp.
fn check_generation(probe_gen: u64, requested_gen: u64) -> Result<(), AgentPlanError> {
    if probe_gen != requested_gen {
        return Err(AgentPlanError::ProbeGenerationMismatch {
            plan: requested_gen,
            probe: probe_gen,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Provided-value validation
// ---------------------------------------------------------------------------

fn validate_provided_values(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
) -> Result<(), AgentPlanError> {
    let repo_ids: Vec<&str> = definition
        .repository_fields
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    let agent_ids: Vec<&str> = definition
        .agent_fields
        .iter()
        .map(|f| f.id.as_str())
        .collect();
    for (scope, id) in values.provided_keys() {
        let declared = match scope {
            FieldScope::Repository => &repo_ids,
            FieldScope::Agent => &agent_ids,
        };
        if !declared.contains(&id) {
            return Err(AgentPlanError::UnknownFieldValue {
                field: id.to_string(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Argv / env emission
// ---------------------------------------------------------------------------

/// The emitted argv and env effects produced in declaration order.
///
/// A small typed carrier so emission helpers avoid returning a complex tuple.
#[derive(Debug, Clone, Default)]
struct EmittedEffects {
    /// Ordered argv elements, each preserved byte-wise as [`OsString`].
    argv: Vec<OsString>,
    /// Ordered `(name, value)` env pairs from typed env emitters only.
    env: Vec<(OsString, OsString)>,
}

impl EmittedEffects {
    /// Push a single argv element.
    fn push_arg(&mut self, element: impl Into<OsString>) {
        self.argv.push(element.into());
    }

    /// Push a `(name, value)` env pair.
    fn push_env(&mut self, name: impl Into<OsString>, value: impl Into<OsString>) {
        self.env.push((name.into(), value.into()));
    }
}

/// Emit `(argv, env)` from the definition's emitters in declaration order.
///
/// Each emitter contributes zero or more argv/env elements; no shell template,
/// token splitting, or raw-argument field is used. Env starts empty and
/// receives only typed [`Emitter::Environment`] pairs.
fn emit_argv_env(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
) -> Result<EmittedEffects, AgentPlanError> {
    let mut effects = EmittedEffects::default();
    for emitter in &definition.emitters {
        match emitter {
            Emitter::Fixed { value } => {
                effects.push_arg(OsString::from(value));
            }
            Emitter::Option { name, field } => {
                if let Some(string_value) = resolve_string_value(definition, values, field)? {
                    effects.push_arg(OsString::from(name));
                    effects.push_arg(OsString::from(string_value));
                }
            }
            Emitter::BooleanOption {
                name,
                field,
                true_value,
                false_value,
            } => {
                if let Some(bool_value) = resolve_bool_value(definition, values, field)? {
                    effects.push_arg(OsString::from(name));
                    let emitted = if bool_value {
                        true_value.clone()
                    } else {
                        false_value.clone().unwrap_or_default()
                    };
                    if !emitted.is_empty() {
                        effects.push_arg(OsString::from(emitted));
                    }
                }
            }
            Emitter::RepeatedOption { name, field } => {
                if let Some(list) = resolve_list_value(definition, values, field)? {
                    for element in &list {
                        effects.push_arg(OsString::from(name));
                        effects.push_arg(OsString::from(element));
                    }
                }
            }
            Emitter::Positional { field } => {
                if let Some(string_value) = resolve_string_value(definition, values, field)? {
                    effects.push_arg(OsString::from(string_value));
                }
            }
            Emitter::Flag { name, field } => {
                if resolve_bool_value(definition, values, field)? == Some(true) {
                    effects.push_arg(OsString::from(name));
                }
            }
            Emitter::Environment { name, field } => {
                if let Some(string_value) = resolve_string_value(definition, values, field)? {
                    effects.push_env(OsString::from(name), OsString::from(string_value));
                }
            }
        }
    }
    Ok(effects)
}

/// Resolve a field's effective value: explicit value, then declared default.
fn effective_value<'a>(
    definition: &'a AgentDefinition,
    values: &'a LaunchFieldValues,
    field: &str,
) -> Option<&'a FieldValue> {
    if let Some(value) = values.repository(field) {
        return Some(value);
    }
    if let Some(value) = values.agent(field) {
        return Some(value);
    }
    definition
        .repository_fields
        .iter()
        .find(|f| f.id == field)
        .and_then(|f| f.default.as_ref())
        .or_else(|| {
            definition
                .agent_fields
                .iter()
                .find(|f| f.id == field)
                .and_then(|f| f.default.as_ref())
        })
}

/// Resolve a non-empty string representation for Option/Positional emitters.
fn resolve_string_value(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
    field: &str,
) -> Result<Option<String>, AgentPlanError> {
    let Some(value) = effective_value(definition, values, field) else {
        return Ok(None);
    };
    match value {
        FieldValue::String(s) | FieldValue::Path(s) => {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(s.clone()))
            }
        }
        FieldValue::Integer(i) => Ok(Some(i.to_string())),
        FieldValue::Boolean(_) | FieldValue::OptionalBoolean(_) | FieldValue::StringList(_) => {
            Err(AgentPlanError::FieldKindMismatch {
                field: field.to_string(),
            })
        }
    }
}

/// Resolve a boolean for Flag/BooleanOption emitters.
fn resolve_bool_value(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
    field: &str,
) -> Result<Option<bool>, AgentPlanError> {
    let Some(value) = effective_value(definition, values, field) else {
        return Ok(None);
    };
    match value {
        FieldValue::Boolean(b) | FieldValue::OptionalBoolean(Some(b)) => Ok(Some(*b)),
        FieldValue::OptionalBoolean(None) => Ok(None),
        _ => Err(AgentPlanError::FieldKindMismatch {
            field: field.to_string(),
        }),
    }
}

/// Resolve a string list for RepeatedOption emitters.
fn resolve_list_value(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
    field: &str,
) -> Result<Option<Vec<String>>, AgentPlanError> {
    let Some(value) = effective_value(definition, values, field) else {
        return Ok(None);
    };
    match value {
        FieldValue::StringList(list) => {
            if list.is_empty() {
                Ok(None)
            } else {
                Ok(Some(list.clone()))
            }
        }
        _ => Err(AgentPlanError::FieldKindMismatch {
            field: field.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Signature stamping
// ---------------------------------------------------------------------------

/// Compute the content digest of the typed launch-signature field values.
///
/// Only fields declared with `launch_signature: true` participate, in
/// declaration order (repository scope, then agent scope). Secrets and
/// display-only fields are excluded by construction: the definition declares
/// which fields participate in the signature.
fn compute_typed_value_hash(
    definition: &AgentDefinition,
    values: &LaunchFieldValues,
) -> DefinitionSha256 {
    let mut buffer = Vec::new();
    for field in definition
        .repository_fields
        .iter()
        .filter(|field| field.launch_signature)
    {
        extend_signature_field(&mut buffer, FieldScope::Repository, &field.id, values);
    }
    for field in definition
        .agent_fields
        .iter()
        .filter(|field| field.launch_signature)
    {
        extend_signature_field(&mut buffer, FieldScope::Agent, &field.id, values);
    }
    DefinitionSha256::digest(&buffer)
}

fn extend_signature_field(
    buffer: &mut Vec<u8>,
    scope: FieldScope,
    id: &str,
    values: &LaunchFieldValues,
) {
    let scope_byte = match scope {
        FieldScope::Repository => b'R',
        FieldScope::Agent => b'A',
    };
    buffer.push(scope_byte);
    buffer.extend_from_slice(id.as_bytes());
    buffer.push(0); // field/value separator
    let value = values.repository(id).or_else(|| values.agent(id));
    if let Some(value) = value {
        match value {
            FieldValue::Boolean(b) | FieldValue::OptionalBoolean(Some(b)) => {
                buffer.push(if *b { b'1' } else { b'0' });
            }
            FieldValue::OptionalBoolean(None) => buffer.push(b'n'),
            FieldValue::String(s) | FieldValue::Path(s) => buffer.extend_from_slice(s.as_bytes()),
            FieldValue::Integer(i) => buffer.extend_from_slice(i.to_string().as_bytes()),
            FieldValue::StringList(list) => {
                for element in list {
                    buffer.extend_from_slice(element.as_bytes());
                    buffer.push(b',');
                }
            }
        }
    }
    buffer.push(0); // record separator
}

#[cfg(test)]
#[path = "agent_plan_tests.rs"]
mod tests;
