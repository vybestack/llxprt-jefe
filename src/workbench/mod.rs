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

pub use allocate::LayoutError;
pub use config::panel_insets;
pub use descriptor::{Axis, LayoutChild, LayoutNode, PanelDescriptor, ScreenDescriptor, Size};
pub use geometry::{Extent, Insets, Rect};
pub use ids::{
    ID_BYTE_LIMIT, IdError, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN, MAX_SCREENS,
    MAX_SPLIT_CHILDREN, MIN_SPLIT_CHILDREN, PanelId, PanelTypeId, RouteId, ScreenId,
    ScreenInstanceId,
};
pub use migration::{LEGACY_SCREEN_VALUES, migrate_legacy_screen_value};
pub use resolve::{
    PanelState, ResolvedLayout, ResolvedPanel, TooSmall, pty_content_rect, repair_focus,
    resolve_layout,
};
pub use screens::{
    ACTIONS, ALL_SCREENS, DASHBOARD, ERRORS, ISSUES, PTY_PANEL_TYPE, PULL_REQUESTS, REPOSITORIES,
    REPOSITORIES_PANEL, ScreenRegistry, TERMINALS, builtin_screens,
};
pub use validate::{DescriptorError, validate_descriptor};
