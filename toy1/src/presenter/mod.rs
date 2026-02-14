//! Presenters for transforming data into view models.
//!
//! Presenters sit between the data layer and UI components,
//! formatting data for display (e.g., elapsed time, status icons).

pub mod format;

pub use format::{format_elapsed, status_icon};
