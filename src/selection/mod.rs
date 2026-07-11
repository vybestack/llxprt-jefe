//! Pure, iocraft-free mouse-selection model.
//!
//! This module owns the data model and geometry math for mouse-based text
//! selection across every jefe pane (issue list/detail, PR list/detail,
//! sidebar, agent list, preview, and the terminal snapshot when unfocused).
//!
//! It is deliberately free of any iocraft types (`Color`, `Props`, element
//! trees): the geometry here derives from the same [`crate::layout`]
//! constants the screens use, so a screen `(col, row)` can be mapped to a pane
//! and content coordinate without hit-testing a component tree.
//!
//! # Design
//!
//! - [`SelectablePane`] names every region a user can select text in.
//! - [`SelectionPoint`] is a `(pane, content_line, content_col)` triple —
//!   content coordinates, already adjusted for the pane's scroll offset.
//! - [`TextSelection`] pairs an anchor (mouse-down) and focus (current drag)
//!   point. Both points always live in the *same* pane; selections never cross
//!   pane boundaries.
//! - [`PaneGeometry`] is the screen-space rectangle of one pane, computed by
//!   [`pane_at`] from the active [`crate::state::ScreenMode`] and terminal size.
//! - [`ScreenLayout`] carries the conditional band flags (error banner,
//!   filter controls) that affect vertical row splits in Issues/PR mode.
//!
//! All functions are pure and `#[must_use]`.

mod content;
mod form_content;
mod geometry;
mod layout_descriptor;
mod overlay_content;
mod text;

pub use content::{PaneContent, pane_content_lines};
pub use form_content::{agent_form_content_lines, repository_form_content_lines};
pub use geometry::{PaneGeometry, pane_at};
pub use layout_descriptor::{OverlayPane, ScreenLayout};
pub use text::{
    HighlightRange, SelectablePane, SelectionPoint, TextSelection, normalize_selection,
    point_to_content_coords, row_highlight_range, selection_text,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
