//! The list-to-detail ports and relationship the bundled workspace screens
//! declare (issue #385, CW05-09).
//!
//! The issue and pull-request screens have always invalidated their detail pane
//! when the list selection moved. That rule is stated here as data rather than
//! as reducer glue, in exactly the form a user-authored screen states it, so
//! there is one description of what "list drives detail" means and one engine
//! that carries it out.

use super::descriptor::{PortDescriptor, PortDirection, PortRef, ScreenDescriptor};
use super::ids::{IdError, PanelId, PortId, VersionedTypeId};
use super::relationships::{ActivationMode, EmptyPolicy, Relationship, RelationshipKind};

/// The port a workspace list publishes its selection on.
pub const SELECTION_PORT: &str = "selection";
/// The port a workspace detail consumes its subject on.
pub const SUBJECT_PORT: &str = "subject";

/// The list panel's selection output, when the screen couples list and detail.
///
/// # Errors
///
/// Returns the violated identifier rule for a malformed compiled name.
pub fn selection_port(
    subject_type: Option<&'static str>,
    direction: PortDirection,
) -> Result<Option<PortDescriptor>, IdError> {
    typed_port(SELECTION_PORT, subject_type, direction, false)
}

/// The detail panel's subject input, when the screen couples list and detail.
///
/// It is not retained: a workspace detail pane shows the current selection and
/// nothing when there is none, which is what the screens do today.
///
/// # Errors
///
/// Returns the violated identifier rule for a malformed compiled name.
pub fn subject_port(
    subject_type: Option<&'static str>,
    direction: PortDirection,
) -> Result<Option<PortDescriptor>, IdError> {
    typed_port(SUBJECT_PORT, subject_type, direction, false)
}

fn typed_port(
    id: &'static str,
    subject_type: Option<&'static str>,
    direction: PortDirection,
    retained: bool,
) -> Result<Option<PortDescriptor>, IdError> {
    let Some(subject_type) = subject_type else {
        return Ok(None);
    };
    Ok(Some(PortDescriptor {
        id: PortId::parse(id)?,
        direction,
        type_id: VersionedTypeId::parse(subject_type)?,
        required: false,
        retained,
    }))
}

/// The list-to-detail coupling a workspace screen declares, if it declares one.
///
/// Moving the selection republishes the subject at once, and an empty selection
/// clears the detail — the behavior the issue and pull-request screens have
/// always had.
///
/// # Errors
///
/// Returns the violated identifier rule for a malformed compiled name.
pub fn workspace_relationships(
    list: &'static str,
    detail: &'static str,
    subject_type: Option<&'static str>,
) -> Result<Vec<Relationship>, IdError> {
    if subject_type.is_none() {
        return Ok(Vec::new());
    }
    Ok(vec![Relationship {
        kind: RelationshipKind::MasterDetail {
            activation: ActivationMode::Immediate,
            empty: EmptyPolicy::ShowNone,
        },
        source: PortRef {
            panel: PanelId::parse(list)?,
            port: PortId::parse(SELECTION_PORT)?,
        },
        target: PortRef {
            panel: PanelId::parse(detail)?,
            port: PortId::parse(SUBJECT_PORT)?,
        },
    }])
}

/// The master-detail edge a screen declares, if it declares one.
///
/// Reducers ask the descriptor rather than naming ports themselves, so the
/// declaration stays the single source of truth for which panels are coupled.
#[must_use]
pub fn master_detail_edge(descriptor: &ScreenDescriptor) -> Option<(PortRef, PortRef)> {
    descriptor
        .relationships
        .iter()
        .find(|relationship| matches!(relationship.kind, RelationshipKind::MasterDetail { .. }))
        .map(|relationship| (relationship.source, relationship.target))
}
