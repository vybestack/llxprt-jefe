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

pub mod activation;
pub mod allocate;
pub mod compose;
#[cfg(test)]
#[path = "compose_settings_tests.rs"]
mod compose_settings_tests;
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
pub mod resource_schemas;
pub mod route;
pub mod screen_file;
pub mod screen_file_bounds;
pub mod screen_file_shape;
pub mod screen_lowering;
pub mod screen_lowering_layout;
pub mod screen_lowering_values;
pub mod screens;
pub mod screens_ports;
pub mod validate;

#[cfg(test)]
#[path = "screen_file_fixtures.rs"]
mod screen_file_fixtures;

#[cfg(test)]
#[path = "screen_file_tests.rs"]
mod screen_file_tests;

#[cfg(test)]
#[path = "screen_file_bound_tests.rs"]
mod screen_file_bound_tests;

#[cfg(test)]
#[path = "screen_lowering_resource_tests.rs"]
mod screen_lowering_resource_tests;

#[cfg(test)]
#[path = "compose_fixtures.rs"]
mod compose_fixtures;

#[cfg(test)]
pub(crate) use compose_fixtures::{
    control_origin_composition, control_origin_definition, try_control_origin_composition,
    try_control_origin_composition_with_definitions,
};

#[cfg(test)]
#[path = "compose_tests.rs"]
mod compose_tests;

#[cfg(test)]
#[path = "compose_package_tests.rs"]
mod compose_package_tests;

#[cfg(test)]
#[path = "relationship_fixtures.rs"]
pub(crate) mod relationship_fixtures;

#[cfg(test)]
#[path = "relationships_tests.rs"]
mod relationships_tests;

#[cfg(test)]
#[path = "relationship_propagation_tests.rs"]
mod relationship_propagation_tests;
#[cfg(test)]
#[path = "resource_schemas_tests.rs"]
mod resource_schemas_tests;

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

#[cfg(test)]
#[path = "route_tests.rs"]
mod route_tests;

pub use activation::{ActivationField, ActivationKind, ScreenBinding};
pub use allocate::LayoutError;
pub use compose::{CompositionRefused, ScreenComposition, compose_screens};
pub use config::panel_insets;
pub use descriptor::{
    Axis, HostPanelCapability, HostPanelModelSource, HostScreenCapability, LayoutChild, LayoutNode,
    OverlayKind, PanelDescriptor, PortDescriptor, PortDirection, PortRef, ScreenDescriptor, Size,
};
pub use diagnostics::{ScrCode, ScreenDiagnostic};
pub use geometry::{Extent, Insets, Rect};
pub use ids::{
    BuiltinScreenId, CUSTOM_MEMBER_BYTE_LIMIT, CUSTOM_SCREEN_NAMESPACE, CustomScreenId,
    DASHBOARD_IDENTITY, DASHBOARD_SCREEN_ID, ID_BYTE_LIMIT, IdError, MAX_ACTIVATION_FIELDS,
    MAX_BINDINGS_PER_SCREEN, MAX_FIELDS_PER_RESOURCE, MAX_LAYOUT_DEPTH, MAX_PANELS_PER_SCREEN,
    MAX_PORTS_PER_PANEL, MAX_RELATIONSHIPS_PER_SCREEN, MAX_RESOURCES_PER_SCREEN, MAX_SCREENS,
    MAX_SPLIT_CHILDREN, OpenScreenId, PanelId, PanelInstanceId, PanelTypeId, PluginScreenId,
    PortId, RouteId, ScreenId, ScreenIdentity, ScreenInstanceId, ScreenInstanceIdExhausted,
    VersionedTypeId,
};
pub use intern::{InternExhausted, MAX_INTERNED_IDENTIFIERS, intern};
pub use lowering_error::LoweringError;
pub use migration::{LEGACY_SCREEN_VALUES, MigrationOutcome, migrate_persisted_screen_value};
pub use panel_types::{DEFINABLE_PANEL_TYPES, PanelTypeError, find_panel_type, resolve_panel_type};
pub use relationship_propagation::{
    PortInstanceKey, PortUpdate, PortValue, PropagationAbort, RelationshipInstance,
    RelationshipInstanceError, RelationshipState, RelationshipTransition, SourceIntent, propagate,
};
pub use relationships::{
    ActivationMode, EmptyPolicy, Relationship, RelationshipError, RelationshipKind,
    SessionEmptyPolicy, validate_relationships,
};
pub use resolve::{
    LayoutGeneration, PanelFrame, PanelState, ResolvedLayout, ResolvedPanel, RuntimeViewport,
    TooSmall, pty_content_rect, repair_focus, resolve_layout,
};
pub use resource_schemas::{
    BuiltinResourceSchemaError, ResourceSchema, ResourceSchemaError, ResourceSchemaRegistry,
    builtin_resource_schemas,
};
pub use route::{
    ActivationError, ActivationValue, ActivationValues, MAX_ACTIVATION_BYTES, NavCode,
    RouteDeclaration, route_declaration,
};
pub use screen_file::{ScreenFile, parse_screen_file};
pub use screen_file_bounds::{ScreenSyntaxError, ScreenSyntaxReason};
pub use screen_lowering::{LoweredScreen, ScreenProvenance, lower_package_screen, lower_screen};
pub use screens::{
    ACTIONS_LIST_PANEL, ERRORS_LIST_PANEL, ISSUES_LIST_PANEL, PTY_PANEL_TYPE,
    PULL_REQUESTS_LIST_PANEL, PackagePanelBinding, REPOSITORIES_PANEL, RegistryError,
    SELECTION_PORT, SETTINGS_AGENT_TYPES_PANEL, SETTINGS_APPEARANCE_PANEL,
    SETTINGS_DIAGNOSTICS_PANEL, SETTINGS_GENERAL_PANEL, SETTINGS_KEYS_PANEL,
    SETTINGS_PLUGINS_PANEL, SETTINGS_SCREENS_PANEL, SETTINGS_SECTIONS_PANEL, SUBJECT_PORT,
    ScreenRegistry, TERMINALS_LIST_PANEL, builtin_screens, initial_focus, route_of,
};
pub use validate::{DescriptorError, validate_descriptor};
