//! Internal screen descriptors and the sole executable layout resolver
//! (issue #384).
//!
//! The workbench is an I/O-free description of what screens exist, which panels
//! they contain, and how those panels divide the available cells. It owns no
//! state, performs no I/O, touches no terminal, and knows nothing about
//! rendering, so it can be exercised exhaustively as pure data.
//!
//! - [`ids`] — validated identifier vocabulary and the declared structural limits.
//! - [`descriptor`] — the closed descriptor and layout-tree value types.
//! - [`validate`] — the structural invariants every compiled descriptor satisfies.
//! - [`screens`] — the compiled descriptors, which are the sole definition of
//!   each shipped screen.
//! - [`migration`] — the one-way mapping from legacy persisted screen values to
//!   stable [`ids::ScreenId`]s.

pub mod allocate;
pub mod compose;
pub mod config;
pub mod descriptor;
pub mod diagnostics;
pub mod geometry;
pub mod ids;
pub mod intern;
pub mod lowering_error;
pub mod migration;
pub mod panel_types;
pub mod relationship_propagation;
pub mod relationships;
pub mod resolve;
pub mod screen_file;
pub mod screen_file_bounds;
pub mod screen_file_shape;
pub mod screen_lowering;
pub mod screen_lowering_layout;
pub mod screen_lowering_values;
pub mod screens;
pub mod validate;

#[cfg(test)]
#[path = "screen_file_tests.rs"]
mod screen_file_tests;

#[cfg(test)]
#[path = "compose_fixtures.rs"]
mod compose_fixtures;

#[cfg(test)]
#[path = "compose_tests.rs"]
mod compose_tests;

#[cfg(test)]
#[path = "relationship_fixtures.rs"]
mod relationship_fixtures;

#[cfg(test)]
#[path = "relationships_tests.rs"]
mod relationships_tests;

#[cfg(test)]
#[path = "relationship_propagation_tests.rs"]
mod relationship_propagation_tests;

#[cfg(test)]
#[path = "ids_tests.rs"]
mod ids_tests;

#[cfg(test)]
#[path = "custom_ids_tests.rs"]
mod custom_ids_tests;

#[cfg(test)]
#[path = "intern_tests.rs"]
mod intern_tests;

#[cfg(test)]
#[path = "config_tests.rs"]
mod config_tests;

#[cfg(test)]
#[path = "descriptor_tests.rs"]
mod descriptor_tests;

#[cfg(test)]
#[path = "screens_tests.rs"]
mod screens_tests;

#[cfg(test)]
#[path = "migration_tests.rs"]
mod migration_tests;

#[cfg(test)]
#[path = "allocate_tests.rs"]
mod allocate_tests;

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod resolve_tests;

use std::sync::OnceLock;

static PUBLISHED_REGISTRY: OnceLock<ScreenRegistry> = OnceLock::new();

/// The published screen registry could not be replaced.
///
/// Publication is a one-time startup step, so this means composition ran twice
/// or ran after something already read the registry. Both are ordering mistakes
/// in this program, not anything a user did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryAlreadyPublished;

impl std::fmt::Display for RegistryAlreadyPublished {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the screen registry was already published")
    }
}

impl std::error::Error for RegistryAlreadyPublished {}

/// Publish the composed registry, which every later read observes.
///
/// Publication is atomic: either the whole composed set becomes the authority
/// or none of it does. Startup composes compiled and lowered descriptors into
/// one candidate and publishes it here exactly once, before anything renders.
///
/// # Errors
///
/// Returns [`RegistryAlreadyPublished`] when a registry is already in place.
pub fn publish_screen_registry(registry: ScreenRegistry) -> Result<(), RegistryAlreadyPublished> {
    PUBLISHED_REGISTRY
        .set(registry)
        .map_err(|_| RegistryAlreadyPublished)
}

/// The validated screen registry.
///
/// Returns whatever startup published. When nothing has been published — in a
/// unit test, or on a path that reads before composition runs — the compiled
/// screens are built and published instead, so a reader always sees a complete,
/// validated set rather than an absent one.
///
/// # Errors
///
/// Returns the first structural violation in the compiled table. That is a
/// programming error in [`screens`], and the descriptor tests fail on it too.
pub fn screen_registry() -> Result<&'static ScreenRegistry, RegistryError> {
    if let Some(registry) = PUBLISHED_REGISTRY.get() {
        return Ok(registry);
    }
    let built = builtin_screens()?;
    Ok(PUBLISHED_REGISTRY.get_or_init(|| built))
}

/// The descriptor for one screen.
///
/// # Errors
///
/// Returns the registry error if the compiled table is malformed.
pub fn screen_descriptor(id: ScreenId) -> Result<&'static ScreenDescriptor, RegistryError> {
    let registry = screen_registry()?;
    registry
        .get(id)
        .ok_or(RegistryError::MissingScreen { screen: id })
}

pub use allocate::LayoutError;
pub use compose::{CompositionRefused, ScreenComposition, compose_screens};
pub use config::panel_insets;
pub use descriptor::{
    Axis, LayoutChild, LayoutNode, PanelDescriptor, PortDescriptor, PortDirection, PortRef,
    ScreenDescriptor, Size,
};
pub use diagnostics::{ScrCode, ScreenDiagnostic};
pub use geometry::{Extent, Insets, Rect};
pub use ids::{
    CUSTOM_MEMBER_BYTE_LIMIT, CUSTOM_SCREEN_NAMESPACE, CustomScreenId, ID_BYTE_LIMIT, IdError,
    MAX_ACTIVATION_FIELDS, MAX_BINDINGS_PER_SCREEN, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN,
    MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN, MAX_SCREENS, MAX_SPLIT_CHILDREN,
    MIN_SPLIT_CHILDREN, PanelId, PanelTypeId, PortId, RouteId, ScreenId, ScreenIdentity,
    ScreenInstanceId, VersionedTypeId,
};
pub use intern::{InternExhausted, MAX_INTERNED_IDENTIFIERS, intern};
pub use lowering_error::LoweringError;
pub use migration::{LEGACY_SCREEN_VALUES, MigrationOutcome, migrate_persisted_screen_value};
pub use panel_types::{DEFINABLE_PANEL_TYPES, PanelTypeError, resolve_panel_type};
pub use relationship_propagation::{
    PortUpdate, PortValue, PropagationAbort, RelationshipState, RelationshipTransition,
    SourceIntent, propagate,
};
pub use relationships::{
    ActivationMode, EmptyPolicy, Relationship, RelationshipError, RelationshipKind,
    SessionEmptyPolicy, validate_relationships,
};
pub use resolve::{
    PanelState, ResolvedLayout, ResolvedPanel, TooSmall, pty_content_rect, repair_focus,
    resolve_layout,
};
pub use screen_file::{ScreenFile, parse_screen_file};
pub use screen_file_bounds::{ScreenSyntaxError, ScreenSyntaxReason};
pub use screen_lowering::{LoweredScreen, ScreenProvenance, lower_screen};
pub use screens::{
    PTY_PANEL_TYPE, REPOSITORIES_PANEL, RegistryError, ScreenRegistry, builtin_screens,
};
pub use validate::{DescriptorError, validate_descriptor};
