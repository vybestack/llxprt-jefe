//! Typed same-screen port relationships and their graph invariants
//! (issue #385, CW05-08).
//!
//! A relationship is the only way one panel may influence another. Panels do
//! not reach into each other and do not observe each other's state; a screen
//! declares that one panel's output port feeds another panel's input port, and
//! the reducer moves values along those edges. That indirection is what lets a
//! user-authored screen wire panels together without being able to run code.
//!
//! The graph is deliberately narrow. Every rule below exists because breaking it
//! makes a screen's behavior either ambiguous or unbounded:
//!
//! - **Same screen.** A port reference that resolves nowhere in this descriptor
//!   is rejected, so a definition cannot reach into another screen's panels.
//! - **Output to input, same versioned type.** Direction and type are checked
//!   exactly, version included, so a panel that starts publishing a new shape
//!   fails validation instead of quietly feeding the wrong value.
//! - **Acyclic.** Propagation advances one hop per intent, so a cycle would let
//!   a screen drive itself forever as each panel republished in turn.
//!   Acyclicity is measured over panels, because a panel that consumes a value
//!   is what re-derives what it publishes.
//! - **One incoming controlling edge per target.** Two edges driving one input
//!   would make the input's value depend on evaluation order.
//! - **One outgoing edge per source port and kind, and no same-kind fan-out
//!   from a panel.** A panel that drives two details of the same kind has no
//!   single answer to "which detail follows this selection".

use std::collections::{BTreeMap, BTreeSet};

use super::descriptor::{PortDirection, PortRef, ScreenDescriptor};
use super::ids::{MAX_RELATIONSHIPS_PER_SCREEN, PanelId, VersionedTypeId};

/// When a master-detail target follows its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActivationMode {
    /// Follow in the same transition the source changed in.
    Immediate,
    /// Stage the source and follow only when the declared activation action
    /// fires.
    Explicit,
}

/// What a master-detail target shows once its source is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EmptyPolicy {
    /// Clear the target.
    ShowNone,
    /// Set the target to the typed all-value.
    ShowAll,
    /// Leave the target's prior value in place.
    Retain,
}

/// What a session target does once its source is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionEmptyPolicy {
    /// Clear the session attachment.
    Detach,
    /// Leave the attachment in place.
    Retain,
}

/// The closed set of relationship kinds and their policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RelationshipKind {
    /// The source narrows what the target operates on.
    Scope,
    /// The source selects the subject the target elaborates.
    MasterDetail {
        /// When the target follows.
        activation: ActivationMode,
        /// What the target shows when the source is absent.
        empty: EmptyPolicy,
    },
    /// The source names the session the target attaches to.
    SessionTarget {
        /// What the target does when the source is absent.
        empty: SessionEmptyPolicy,
    },
}

impl RelationshipKind {
    /// The stable text naming this kind, ignoring its policies.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::MasterDetail { .. } => "master-detail",
            Self::SessionTarget { .. } => "session-target",
        }
    }
}

/// One declared edge between two same-screen ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relationship {
    /// Kind and policies.
    pub kind: RelationshipKind,
    /// The output port that drives the edge.
    pub source: PortRef,
    /// The input port the edge drives.
    pub target: PortRef,
}

/// A violated relationship-graph invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipError {
    /// The screen declares more than [`MAX_RELATIONSHIPS_PER_SCREEN`] edges.
    TooMany {
        /// Declared edge count.
        count: usize,
    },
    /// A port reference names nothing this screen declares.
    OutOfScope {
        /// The unresolvable reference.
        reference: PortRef,
    },
    /// An endpoint has the wrong direction for its role.
    WrongDirection {
        /// The offending reference.
        reference: PortRef,
        /// The direction the role requires.
        expected: PortDirection,
    },
    /// The endpoints carry different versioned types.
    TypeMismatch {
        /// Type the source publishes.
        source: VersionedTypeId,
        /// Type the target consumes.
        target: VersionedTypeId,
    },
    /// The edges form a cycle among panels.
    Cycle {
        /// A panel on the cycle.
        panel: PanelId,
    },
    /// Two edges drive one input port.
    DuplicateIncoming {
        /// The over-driven target.
        target: PortRef,
    },
    /// Two edges of one kind leave one output port.
    DuplicateOutgoing {
        /// The over-used source.
        source: PortRef,
        /// The repeated kind.
        kind: &'static str,
    },
    /// One panel drives two targets with the same kind.
    SameKindFanOut {
        /// The offending source panel.
        panel: PanelId,
        /// The repeated kind.
        kind: &'static str,
    },
}

impl std::fmt::Display for RelationshipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany { count } => write!(
                formatter,
                "screen declares {count} relationships (max {MAX_RELATIONSHIPS_PER_SCREEN})"
            ),
            Self::OutOfScope { reference } => {
                write!(formatter, "{reference} is not a port of this screen")
            }
            Self::WrongDirection {
                reference,
                expected,
            } => write!(
                formatter,
                "{reference} must be an {} port",
                expected.as_str()
            ),
            Self::TypeMismatch { source, target } => write!(
                formatter,
                "source publishes {source} but target consumes {target}"
            ),
            Self::Cycle { panel } => {
                write!(
                    formatter,
                    "relationships form a cycle through panel {panel}"
                )
            }
            Self::DuplicateIncoming { target } => {
                write!(
                    formatter,
                    "{target} is driven by more than one relationship"
                )
            }
            Self::DuplicateOutgoing { source, kind } => write!(
                formatter,
                "{source} declares more than one {kind} relationship"
            ),
            Self::SameKindFanOut { panel, kind } => write!(
                formatter,
                "panel {panel} drives more than one {kind} target"
            ),
        }
    }
}

impl std::error::Error for RelationshipError {}

/// Check every relationship invariant for one screen.
///
/// # Errors
///
/// Returns the first violated invariant, naming the offending reference, panel,
/// or type.
pub fn validate_relationships(descriptor: &ScreenDescriptor) -> Result<(), RelationshipError> {
    if descriptor.relationships.len() > MAX_RELATIONSHIPS_PER_SCREEN {
        return Err(RelationshipError::TooMany {
            count: descriptor.relationships.len(),
        });
    }
    for relationship in &descriptor.relationships {
        check_endpoints(descriptor, relationship)?;
    }
    check_uniqueness(descriptor)?;
    check_acyclic(descriptor)
}

/// Check that both endpoints exist, face the right way, and agree on type.
fn check_endpoints(
    descriptor: &ScreenDescriptor,
    relationship: &Relationship,
) -> Result<(), RelationshipError> {
    let source = descriptor
        .port(&relationship.source)
        .ok_or(RelationshipError::OutOfScope {
            reference: relationship.source,
        })?;
    let target = descriptor
        .port(&relationship.target)
        .ok_or(RelationshipError::OutOfScope {
            reference: relationship.target,
        })?;
    if source.direction != PortDirection::Output {
        return Err(RelationshipError::WrongDirection {
            reference: relationship.source,
            expected: PortDirection::Output,
        });
    }
    if target.direction != PortDirection::Input {
        return Err(RelationshipError::WrongDirection {
            reference: relationship.target,
            expected: PortDirection::Input,
        });
    }
    if source.type_id != target.type_id {
        return Err(RelationshipError::TypeMismatch {
            source: source.type_id,
            target: target.type_id,
        });
    }
    Ok(())
}

/// Check the incoming, outgoing, and fan-out uniqueness rules.
fn check_uniqueness(descriptor: &ScreenDescriptor) -> Result<(), RelationshipError> {
    let mut incoming: BTreeSet<PortRef> = BTreeSet::new();
    let mut outgoing: BTreeSet<(PortRef, &'static str)> = BTreeSet::new();
    let mut fan_out: BTreeSet<(PanelId, &'static str)> = BTreeSet::new();
    for relationship in &descriptor.relationships {
        let kind = relationship.kind.as_str();
        if !incoming.insert(relationship.target) {
            return Err(RelationshipError::DuplicateIncoming {
                target: relationship.target,
            });
        }
        if !outgoing.insert((relationship.source, kind)) {
            return Err(RelationshipError::DuplicateOutgoing {
                source: relationship.source,
                kind,
            });
        }
        if !fan_out.insert((relationship.source.panel, kind)) {
            return Err(RelationshipError::SameKindFanOut {
                panel: relationship.source.panel,
                kind,
            });
        }
    }
    Ok(())
}

/// Check that the panel graph the edges induce has no cycle.
///
/// A self edge is a one-node cycle, so it needs no separate rule.
fn check_acyclic(descriptor: &ScreenDescriptor) -> Result<(), RelationshipError> {
    let mut edges: BTreeMap<PanelId, Vec<PanelId>> = BTreeMap::new();
    for relationship in &descriptor.relationships {
        edges
            .entry(relationship.source.panel)
            .or_default()
            .push(relationship.target.panel);
    }
    let roots: Vec<PanelId> = edges.keys().copied().collect();
    let mut settled: BTreeSet<PanelId> = BTreeSet::new();
    for panel in roots {
        let mut on_path = BTreeSet::new();
        visit(panel, &edges, &mut settled, &mut on_path)?;
    }
    Ok(())
}

/// Depth-first walk that reports the first panel found twice on one path.
fn visit(
    panel: PanelId,
    edges: &BTreeMap<PanelId, Vec<PanelId>>,
    settled: &mut BTreeSet<PanelId>,
    on_path: &mut BTreeSet<PanelId>,
) -> Result<(), RelationshipError> {
    if settled.contains(&panel) {
        return Ok(());
    }
    if !on_path.insert(panel) {
        return Err(RelationshipError::Cycle { panel });
    }
    for next in edges.get(&panel).map(Vec::as_slice).unwrap_or_default() {
        visit(*next, edges, settled, on_path)?;
    }
    on_path.remove(&panel);
    settled.insert(panel);
    Ok(())
}
