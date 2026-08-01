//! Driving a compiled screen's declared master-detail coupling (issue #385,
//! CW05-09).
//!
//! The issue and pull-request screens have always invalidated their detail pane
//! when the list selection moved. That rule now lives in their descriptors as a
//! declared master-detail relationship, and this module is how the reducer asks
//! it what to do, so the descriptor is the single statement of which panels are
//! coupled and a user-authored screen expresses the same coupling the same way.
//!
//! Nothing here mutates. It answers one question — did the detail input move? —
//! and the caller decides what invalidating means for its own state.

#[cfg(test)]
#[path = "screen_relationships_tests.rs"]
mod screen_relationships_tests;

use crate::workbench::{
    PortValue, RelationshipState, ScreenId, SourceIntent, master_detail_edge, propagate,
    screen_descriptor,
};

/// Whether the screen's declared master-detail edge moves its detail input when
/// the list selection changes from `previous` to `current`.
///
/// Returns `true` when the screen's descriptor cannot be consulted. That is a
/// malformed compiled table, which startup already refuses, and clearing a
/// detail request that may be stale is always safe while keeping one that no
/// longer matches its selection is not.
#[must_use]
pub fn detail_follows_selection(
    screen: ScreenId,
    previous: &PortValue,
    current: &PortValue,
) -> bool {
    let descriptor = match screen_descriptor(screen) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            tracing::error!(%screen, %error, "no compiled descriptor for the active screen");
            return true;
        }
    };
    let Some((source, target)) = master_detail_edge(descriptor) else {
        return false;
    };
    // The prior selection is replayed rather than stored, because the reducer
    // already knows both ends of the change and a second copy of the same fact
    // could only ever disagree with it.
    let seeded = match propagate(
        descriptor,
        &RelationshipState::new(),
        &SourceIntent::Publish {
            port: source,
            value: previous.clone(),
        },
    ) {
        Ok(transition) => transition.state,
        Err(error) => {
            tracing::error!(%screen, %error, "seeding the master-detail relationship failed");
            return true;
        }
    };
    match propagate(
        descriptor,
        &seeded,
        &SourceIntent::Publish {
            port: source,
            value: current.clone(),
        },
    ) {
        Ok(transition) => transition
            .updates
            .iter()
            .any(|update| update.port == target),
        Err(error) => {
            tracing::error!(%screen, %error, "propagating the master-detail relationship failed");
            true
        }
    }
}

/// The value a list publishes for the subject it has selected.
#[must_use]
pub fn subject(identity: Option<u64>) -> PortValue {
    identity.map_or(PortValue::Absent, |number| {
        PortValue::Subject(number.to_string())
    })
}
