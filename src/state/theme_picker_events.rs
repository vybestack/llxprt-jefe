//! Theme-picker modal events.
//!
//! Grouped out of the flat [`crate::state::AppEvent`] enum so that file stays
//! inside the source-size gate: it had reached the hard limit exactly, which
//! meant no feature could add an event without first making room. These
//! variants are a self-contained modal vocabulary and were the smallest
//! cohesive group to lift out.

/// One interaction with the theme-picker modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemePickerEvent {
    /// Open the modal with a snapshot of available themes.
    /// Payload: `(slug, name)` pairs, plus the currently active slug.
    Open {
        available_themes: Vec<(String, String)>,
        active_slug: String,
    },
    /// Move the highlight up one entry.
    NavigateUp,
    /// Move the highlight down one entry.
    NavigateDown,
    /// Confirm the current selection.
    ///
    /// The slug is derived from the modal's `selected_index` at dispatch time
    /// (see `modal_handlers::apply_theme_picker_selection`).
    Confirm,
    /// Toggle the "Apply jefe theme to agent" checkbox (issue #179).
    ToggleOverride,
    /// Dismiss the modal without applying a selection.
    Close,
}
