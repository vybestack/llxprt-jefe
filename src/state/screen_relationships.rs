//! Driving a compiled screen's declared master-detail coupling (issue #385,
//! CW05-09).
//!
//! The issue and pull-request screens have always invalidated their detail pane
//! when the list selection moved. That coupling now lives in their descriptors
//! as a declared master-detail relationship, and this module is how the reducer
//! asks it what to do, so the descriptor is the single statement of which panels
//! are coupled and a user-authored screen expresses the same coupling the same
//! way.
//!
//! What moves the source stays the reducer's business: a list selection has
//! moved when the selected row has moved, which is the same rule as before and
//! does not depend on two rows never naming the same subject. What the coupling
//! *means* — whether there is a detail to drive, and what value it receives — is
//! the descriptor's, resolved through the shared propagation engine.
//!
//! Nothing here mutates. It answers what the detail input becomes, and the
//! caller decides what that means for its own state.

#[cfg(test)]
#[path = "screen_relationships_tests.rs"]
mod screen_relationships_tests;

use crate::workbench::{
    PortValue, RelationshipState, ScreenId, SourceIntent, master_detail_edge, propagate,
    screen_descriptor,
};

/// What the screen's declared master-detail relationship gives its detail input
/// for this selection, or `None` when the screen declares no such coupling.
///
/// A screen whose descriptor cannot be consulted is treated as coupled. That is
/// a malformed compiled table, which startup already refuses; clearing a detail
/// request that may be stale is always safe, while keeping one that no longer
/// matches its selection is not.
#[must_use]
pub fn detail_target_for(screen: ScreenId, selection: &PortValue) -> Option<PortValue> {
    let descriptor = match screen_descriptor(screen) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            tracing::error!(%screen, %error, "no compiled descriptor for the active screen");
            return Some(PortValue::Absent);
        }
    };
    let (source, target) = master_detail_edge(descriptor)?;
    match propagate(
        descriptor,
        &RelationshipState::new(),
        &SourceIntent::Publish {
            port: source,
            value: selection.clone(),
        },
    ) {
        Ok(transition) => Some(transition.state.value(&target)),
        Err(error) => {
            tracing::error!(%screen, %error, "propagating the master-detail relationship failed");
            Some(PortValue::Absent)
        }
    }
}

/// Whether the screen declares that its detail pane follows its list selection.
#[must_use]
pub fn couples_list_to_detail(screen: ScreenId) -> bool {
    detail_target_for(screen, &PortValue::Absent).is_some()
}

/// The value a list publishes for the subject it has selected.
#[must_use]
pub fn subject(identity: Option<u64>) -> PortValue {
    identity.map_or(PortValue::Absent, |number| {
        PortValue::Subject(number.to_string())
    })
}
