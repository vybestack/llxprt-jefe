//! Closed plugin package contract (issue #389 CW-09).
//!
//! This module is the pure, provider-free bottom of the package capability
//! DAG: identity values, the closed manifest schema, and static validation.
//! It performs no file I/O, starts no process, and depends on nothing outside
//! the `domain` layer.

pub mod action;
pub mod code;
pub mod coordinate;
pub mod field;
pub mod limits;
pub mod manifest;
pub mod plugin_id;
pub mod provider;
pub mod reader;
mod reader_parts;
pub mod surface;
pub mod values;

pub use action::{Action, ActionConfirmation, ActionDraft, ActionError, ActionOutcome};
pub use code::PluginCode;
pub use coordinate::{PackageCoordinate, PackageCoordinateError};
pub use field::{Field, FieldDraft, FieldError, FieldKind, RestartScope, Scalar};
pub use manifest::{Manifest, ManifestDraft, ManifestError, PluginDefaults};
pub use plugin_id::{PluginId, PluginIdError, PluginIdErrorReason};
pub use provider::{Provider, ProviderError, ProviderMode, ProviderSelection};
pub use reader::{ManifestReadError, read_manifest};
pub use surface::{
    ConfigSchema, ConfigSchemaError, EventKind, EventSchemaEntry, ModelKind, Panel, PanelDraft,
    PanelError, Port, Route, RouteDraft, RouteError, ScreenContribution, ScreenContributionError,
};
pub use values::{
    HostTriple, HostTripleError, RelativePath, RelativePathError, RelativePathErrorReason,
    SecretReference, SecretReferenceError,
};
