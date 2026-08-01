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
pub mod config;
pub mod descriptor;
pub mod geometry;
pub mod ids;
pub mod migration;
pub mod resolve;
pub mod screens;
pub mod validate;

#[cfg(test)]
#[path = "ids_tests.rs"]
mod ids_tests;

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

static SHIPPED_REGISTRY: OnceLock<ScreenRegistry> = OnceLock::new();

/// The validated shipped screen registry.
///
/// Built and validated once. Startup calls this before any screen is rendered,
/// so a malformed compiled descriptor stops the program with a diagnostic
/// rather than reaching a renderer that would have to cope with a half-formed
/// screen.
///
/// # Errors
///
/// Returns the first structural violation in the compiled table. That is a
/// programming error in [`screens`], and the descriptor tests fail on it too.
pub fn screen_registry() -> Result<&'static ScreenRegistry, RegistryError> {
    if let Some(registry) = SHIPPED_REGISTRY.get() {
        return Ok(registry);
    }
    let built = builtin_screens()?;
    Ok(SHIPPED_REGISTRY.get_or_init(|| built))
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
pub use config::panel_insets;
pub use descriptor::{Axis, LayoutChild, LayoutNode, PanelDescriptor, ScreenDescriptor, Size};
pub use geometry::{Extent, Insets, Rect};
pub use ids::{
    ID_BYTE_LIMIT, IdError, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN, MAX_SCREENS,
    MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN, PanelId, PanelTypeId, RouteId, ScreenId,
    ScreenInstanceId,
};
pub use migration::{LEGACY_SCREEN_VALUES, MigrationOutcome, migrate_persisted_screen_value};
pub use resolve::{
    PanelState, ResolvedLayout, ResolvedPanel, TooSmall, pty_content_rect, repair_focus,
    resolve_layout,
};
pub use screens::{
    PTY_PANEL_TYPE, REPOSITORIES_PANEL, RegistryError, ScreenRegistry, builtin_screens,
};
pub use validate::{DescriptorError, validate_descriptor};
