//! PR sort configuration types (issue #473).
//!
//! Extracted from `domain/mod.rs` to keep that file within the source-size
//! policy. Sort is a projection-time view transform on the loaded PR list.

use serde::{Deserialize, Serialize};

use super::SortOrder;

/// Sort key for the PR list (issue #473).
///
/// PRs have no priority concept, so only Number/Created/Updated are available.
/// `Updated` is the default and preserves the pre-issue-#473 behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PrSortBy {
    Number,
    Created,
    #[default]
    Updated,
}

impl PrSortBy {
    /// Cycle through the sort keys in canonical order, wrapping around.
    #[must_use]
    pub const fn cycle_next(self) -> Self {
        match self {
            Self::Number => Self::Created,
            Self::Created => Self::Updated,
            Self::Updated => Self::Number,
        }
    }

    /// Cycle backward through the sort keys, wrapping around.
    #[must_use]
    pub const fn cycle_prev(self) -> Self {
        match self {
            Self::Number => Self::Updated,
            Self::Created => Self::Number,
            Self::Updated => Self::Created,
        }
    }

    /// User-facing label for the filter-dialog sort-by cycle field.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Created => "created",
            Self::Updated => "updated",
        }
    }
}

/// Active sort configuration for the PR list (issue #473).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrSortConfig {
    #[serde(default)]
    pub by: PrSortBy,
    #[serde(default)]
    pub order: SortOrder,
}

impl PrSortConfig {
    /// The default sort: `Updated/Desc` — preserves pre-issue-#473 behavior.
    #[must_use]
    pub const fn default_sort() -> Self {
        Self {
            by: PrSortBy::Updated,
            order: SortOrder::Desc,
        }
    }
}
