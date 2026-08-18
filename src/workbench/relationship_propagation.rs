//! Pure, bounded propagation of typed relationships (issue #385,
//! CW05-05..CW05-07).
//!
//! Propagation is a total function from "what a source port now publishes" to
//! "what every port that source drives holds afterwards". It performs no I/O,
//! moves no focus, and never mutates in place: it computes the whole transition,
//! checks the transition against its bound, and only then hands back a state the
//! caller swaps in. That is what "no partial state" means concretely — a
//! transition that violates its bound produces an error and nothing else, so
//! there is no half-applied screen to reason about.
//!
//! Propagation is one hop. It applies the edges that leave the published port
//! and stops; it does not decide that a driven panel has therefore republished
//! its own outputs, because only that panel knows what it derives. A chain of
//! edges advances one hop per intent, and the acyclicity rule the graph enforces
//! is what makes such a sequence terminate.
//!
//! Two rules decide what a target holds when its source goes absent, and they
//! apply in this order:
//!
//! 1. a target port that is not retained clears, whatever the relationship says,
//!    because a panel that declared it does not keep values must not be handed
//!    one it cannot hold;
//! 2. a retained target follows its relationship's declared empty policy.
//!
//! Explicit master-detail edges stage a *selection* rather than applying it, so
//! the target only moves when the declared activation action fires. Absence is
//! not staged: a source that disappears is not a selection the user might still
//! confirm, so its empty policy applies at once and any staged selection is
//! discarded.

use std::collections::BTreeMap;

use crate::domain::{Id, TypedPortValue};
use crate::persistence::diagnostic::FOLLOW_UP_LIMIT;

use super::descriptor::{PortRef, ScreenDescriptor};
use super::diagnostics::ScrCode;
use super::ids::{OpenScreenId, PanelId, PanelInstanceId};
use super::relationships::{EmptyPolicy, Relationship, RelationshipKind, SessionEmptyPolicy};
use super::resource_schemas::{ResourceSchemaError, ResourceSchemaRegistry};

/// A value crossing a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortValue {
    /// No value.
    Absent,
    /// Every resource of the port's declared type.
    All,
    /// One closed, schema-validated resource value.
    Typed(TypedPortValue),
}

impl PortValue {
    /// Whether this is the absence of a value.
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

/// Exact runtime identity of one port value owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortInstanceKey {
    /// The independently open screen instance.
    pub open_screen_id: OpenScreenId,
    /// The instantiated panel within that screen.
    pub panel_instance_id: PanelInstanceId,
    /// The declared port within that panel.
    pub port_id: super::PortId,
}

/// Runtime identities and schema owner for one open screen definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipInstance {
    open_screen_id: OpenScreenId,
    owner_id: Id,
    panel_instances: BTreeMap<PanelId, PanelInstanceId>,
}

impl RelationshipInstance {
    /// Bind one open screen to its independently allocated panel instances.
    #[must_use]
    pub fn new(
        open_screen_id: OpenScreenId,
        owner_id: Id,
        panel_instances: BTreeMap<PanelId, PanelInstanceId>,
    ) -> Self {
        Self {
            open_screen_id,
            owner_id,
            panel_instances,
        }
    }

    /// The runtime identity of this open screen.
    #[must_use]
    pub const fn open_screen_id(&self) -> OpenScreenId {
        self.open_screen_id
    }

    /// Resolve a declared port to its exact runtime state key.
    #[must_use]
    pub fn port_key(&self, port: &PortRef) -> Option<PortInstanceKey> {
        self.panel_instances
            .get(&port.panel)
            .copied()
            .map(|panel_instance_id| PortInstanceKey {
                open_screen_id: self.open_screen_id,
                panel_instance_id,
                port_id: port.port,
            })
    }
}
/// What every instantiated port holds, plus selections awaiting activation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelationshipState {
    values: BTreeMap<PortInstanceKey, PortValue>,
    staged: BTreeMap<PortInstanceKey, PortValue>,
}

impl RelationshipState {
    /// An empty state, in which every port is absent.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What one exact runtime port holds.
    #[must_use]
    pub fn value(&self, port: &PortInstanceKey) -> PortValue {
        self.values.get(port).cloned().unwrap_or(PortValue::Absent)
    }

    /// The selection staged for one exact runtime target.
    #[must_use]
    pub fn staged(&self, target: &PortInstanceKey) -> Option<&PortValue> {
        self.staged.get(target)
    }
}

/// What a source did, expressed without reference to any renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceIntent {
    /// A source port now publishes this value.
    Publish {
        /// The output port.
        port: PortRef,
        /// What it now publishes.
        value: PortValue,
    },
    /// The declared activation action fired for one explicitly driven target.
    ///
    /// The action's identity is resolved before this point; the engine only
    /// needs to know which target the user confirmed.
    Activate {
        /// The explicitly driven input port.
        target: PortRef,
    },
}

/// One port whose value the transition changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortUpdate {
    /// The port that changed.
    pub port: PortRef,
    /// What it now holds.
    pub value: PortValue,
}

/// A computed, not-yet-committed transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipTransition {
    /// The state to swap in.
    pub state: RelationshipState,
    /// Every change, in relationship declaration order, with the source first.
    pub updates: Vec<PortUpdate>,
    /// How many relationship follow-ups the transition performed.
    ///
    /// This counts work the edges did, not changes the caller can see: an
    /// explicit edge that stages a selection is a follow-up even though it
    /// moves no port, and the source's own publication is not one because it is
    /// what caused the transition rather than something it caused.
    pub follow_ups: usize,
}

/// The transition was abandoned before anything was committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropagationAbort {
    /// The transition would change more ports than one transition may.
    FollowUpLimit {
        /// How many changes it attempted.
        attempted: usize,
    },
    /// A typed resource did not satisfy the immutable published schema.
    InvalidResource(ResourceSchemaError),
    /// An instantiated panel mapping was missing for a declared port.
    MissingPanelInstance { panel: PanelId },
    /// A typed value did not match its source port's declared type and version.
    PortTypeMismatch { port: PortRef },
    /// `All` is policy output, not a source-publishable resource value.
    InvalidSourceValue { port: PortRef },
}

impl PropagationAbort {
    /// The operator-facing code an abandoned transition reports.
    ///
    /// An abandoned transition is a refused screen behavior, so it carries the
    /// same code a refused screen registry does: whatever the user sees, the
    /// answer is that the named screen did not do what it declared.
    #[must_use]
    pub const fn code(&self) -> ScrCode {
        ScrCode::E301
    }
}

impl std::fmt::Display for PropagationAbort {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FollowUpLimit { attempted } => write!(
                formatter,
                "{}: transition attempted {attempted} follow-ups (max {FOLLOW_UP_LIMIT})",
                self.code()
            ),
            Self::InvalidResource(error) => {
                write!(
                    formatter,
                    "{}: invalid relationship resource: {error}",
                    self.code()
                )
            }
            Self::MissingPanelInstance { panel } => write!(
                formatter,
                "{}: panel {panel} has no runtime instance",
                self.code()
            ),
            Self::PortTypeMismatch { port } => write!(
                formatter,
                "{}: resource type does not match source port {port}",
                self.code()
            ),
            Self::InvalidSourceValue { port } => write!(
                formatter,
                "{}: source port {port} cannot publish a policy value",
                self.code()
            ),
        }
    }
}

impl std::error::Error for PropagationAbort {}

/// Compute the whole transition one source intent produces.
///
/// The bound is enforced by this function rather than assumed from validation,
/// because a bound that only holds when something else was checked first is not
/// a bound.
///
/// # Errors
///
/// Returns [`PropagationAbort`] when the transition would exceed
/// [`FOLLOW_UP_LIMIT`] changes. The supplied state is untouched.
pub fn propagate(
    descriptor: &ScreenDescriptor,
    schemas: &ResourceSchemaRegistry,
    instance: &RelationshipInstance,
    state: &RelationshipState,
    intent: &SourceIntent,
) -> Result<RelationshipTransition, PropagationAbort> {
    let mut draft = Draft {
        state: state.clone(),
        updates: Vec::new(),
        follow_ups: 0,
    };
    match intent {
        SourceIntent::Publish { port, value } => {
            validate_source(descriptor, schemas, instance, port, value)?;
            publish(descriptor, instance, &mut draft, port, value)?;
        }
        SourceIntent::Activate { target } => activate(instance, &mut draft, target)?,
    }
    if draft.follow_ups > FOLLOW_UP_LIMIT {
        return Err(PropagationAbort::FollowUpLimit {
            attempted: draft.follow_ups,
        });
    }
    Ok(RelationshipTransition {
        state: draft.state,
        updates: draft.updates,
        follow_ups: draft.follow_ups,
    })
}

fn validate_source(
    descriptor: &ScreenDescriptor,
    schemas: &ResourceSchemaRegistry,
    instance: &RelationshipInstance,
    port: &PortRef,
    value: &PortValue,
) -> Result<(), PropagationAbort> {
    match value {
        PortValue::Absent => Ok(()),
        PortValue::All => Err(PropagationAbort::InvalidSourceValue { port: *port }),
        PortValue::Typed(value) => {
            let Some(declared) = descriptor.port(port) else {
                return Err(PropagationAbort::PortTypeMismatch { port: *port });
            };
            let version_matches = declared.type_id.version() == value.schema_version.to_string();
            if declared.type_id.name() != value.type_id.as_str() || !version_matches {
                return Err(PropagationAbort::PortTypeMismatch { port: *port });
            }
            schemas
                .validate(&instance.owner_id, value)
                .map_err(PropagationAbort::InvalidResource)
        }
    }
}

/// A transition under construction, discarded whole if it breaks its bound.
struct Draft {
    state: RelationshipState,
    updates: Vec<PortUpdate>,
    follow_ups: usize,
}

/// Apply a source publication and every edge that leaves it.
fn publish(
    descriptor: &ScreenDescriptor,
    instance: &RelationshipInstance,
    draft: &mut Draft,
    port: &PortRef,
    value: &PortValue,
) -> Result<(), PropagationAbort> {
    set(instance, draft, port, value.clone())?;
    for relationship in &descriptor.relationships {
        if &relationship.source != port {
            continue;
        }
        draft.follow_ups += 1;
        drive(descriptor, instance, draft, relationship, value)?;
    }
    Ok(())
}

/// Apply one edge for one published source value.
fn drive(
    descriptor: &ScreenDescriptor,
    instance: &RelationshipInstance,
    draft: &mut Draft,
    relationship: &Relationship,
    value: &PortValue,
) -> Result<(), PropagationAbort> {
    let target = relationship.target;
    let target_key = instance
        .port_key(&target)
        .ok_or(PropagationAbort::MissingPanelInstance {
            panel: target.panel,
        })?;
    if value.is_absent() {
        draft.state.staged.remove(&target_key);
        if let Some(resolved) = absent_value(descriptor, relationship) {
            set(instance, draft, &target, resolved)?;
        }
        return Ok(());
    }
    if is_explicit(relationship.kind) {
        draft.state.staged.insert(target_key, value.clone());
        return Ok(());
    }
    set(instance, draft, &target, value.clone())
}

/// What a target holds once its source is absent, or `None` to leave it alone.
fn absent_value(descriptor: &ScreenDescriptor, relationship: &Relationship) -> Option<PortValue> {
    let retained = descriptor
        .port(&relationship.target)
        .is_some_and(|port| port.retained);
    if !retained {
        return Some(PortValue::Absent);
    }
    match relationship.kind {
        RelationshipKind::Scope => None,
        RelationshipKind::MasterDetail { empty, .. } => match empty {
            EmptyPolicy::ShowNone => Some(PortValue::Absent),
            EmptyPolicy::ShowAll => Some(PortValue::All),
            EmptyPolicy::Retain => None,
        },
        RelationshipKind::SessionTarget { empty } => match empty {
            SessionEmptyPolicy::Detach => Some(PortValue::Absent),
            SessionEmptyPolicy::Retain => None,
        },
    }
}

/// Apply the selection an explicit edge staged for this target.
fn activate(
    instance: &RelationshipInstance,
    draft: &mut Draft,
    target: &PortRef,
) -> Result<(), PropagationAbort> {
    let target_key = instance
        .port_key(target)
        .ok_or(PropagationAbort::MissingPanelInstance {
            panel: target.panel,
        })?;
    let Some(value) = draft.state.staged.remove(&target_key) else {
        return Ok(());
    };
    draft.follow_ups += 1;
    set(instance, draft, target, value)
}

/// Record one change, unless the port already holds that value.
fn set(
    instance: &RelationshipInstance,
    draft: &mut Draft,
    port: &PortRef,
    value: PortValue,
) -> Result<(), PropagationAbort> {
    let key = instance
        .port_key(port)
        .ok_or(PropagationAbort::MissingPanelInstance { panel: port.panel })?;
    if draft.state.value(&key) == value {
        return Ok(());
    }
    draft.state.values.insert(key, value.clone());
    draft.updates.push(PortUpdate { port: *port, value });
    Ok(())
}

/// Whether this kind waits for an activation action.
const fn is_explicit(kind: RelationshipKind) -> bool {
    matches!(
        kind,
        RelationshipKind::MasterDetail {
            activation: super::relationships::ActivationMode::Explicit,
            ..
        }
    )
}
