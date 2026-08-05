//! Closed plugin package contract (issue #389 CW-09).
//!
//! This module is the pure, provider-free bottom of the package capability
//! DAG: identity values, the closed manifest schema, and static validation.
//! It performs no file I/O, starts no process, and depends on nothing outside
//! the `domain` layer.

pub mod code;
pub mod coordinate;
pub mod limits;
pub mod plugin_id;

pub use code::PluginCode;
pub use coordinate::{PackageCoordinate, PackageCoordinateError};
pub use plugin_id::{PluginId, PluginIdError, PluginIdErrorReason};
