//! Typed reads of the panel configuration bag (issue #384).
//!
//! A panel's chrome — the border and title rows the panel draws *inside* its
//! own rectangle — is descriptor data, not renderer trivia: the resolver has to
//! know it to produce a content rectangle, and the wrap, selection, and PTY
//! consumers read that content rectangle. It therefore lives in the panel's
//! configuration bag rather than being re-derived per screen.
//!
//! Reads are total: a missing or wrongly typed key yields the default rather
//! than an error, because the configuration bag is shared with future
//! panel-specific keys this module does not know about.

use crate::domain::{Id, TypedMap, TypedValue};

use super::geometry::Insets;

/// Configuration key for rows consumed at the top of a panel.
pub const CHROME_TOP: &str = "chrome.top";
/// Configuration key for rows consumed at the bottom of a panel.
pub const CHROME_BOTTOM: &str = "chrome.bottom";
/// Configuration key for columns consumed on the left of a panel.
pub const CHROME_LEFT: &str = "chrome.left";
/// Configuration key for columns consumed on the right of a panel.
pub const CHROME_RIGHT: &str = "chrome.right";

/// Read a panel's chrome insets from its configuration bag.
#[must_use]
pub fn panel_insets(config: &TypedMap) -> Insets {
    Insets::new(
        read_cells(config, CHROME_TOP),
        read_cells(config, CHROME_BOTTOM),
        read_cells(config, CHROME_LEFT),
        read_cells(config, CHROME_RIGHT),
    )
}

/// Build a configuration bag describing a panel's chrome.
///
/// # Errors
///
/// Returns `None` if a key is not a valid configuration identifier, which is a
/// programming error in this crate rather than a runtime condition.
#[must_use]
pub fn insets_config(insets: Insets) -> Option<TypedMap> {
    let mut config = TypedMap::new();
    for (key, value) in [
        (CHROME_TOP, insets.top),
        (CHROME_BOTTOM, insets.bottom),
        (CHROME_LEFT, insets.left),
        (CHROME_RIGHT, insets.right),
    ] {
        if value == 0 {
            continue;
        }
        config.insert(Id::parse(key).ok()?, TypedValue::Integer(i64::from(value)));
    }
    Some(config)
}

/// Read one non-negative cell count, defaulting to zero.
///
/// An integer outside the cell range is a mistake in a compiled descriptor
/// rather than a condition the running program can encounter, so it is named in
/// a warning instead of quietly becoming zero and shrinking a panel's chrome.
fn read_cells(config: &TypedMap, key: &str) -> u16 {
    Id::parse(key)
        .ok()
        .and_then(|id| config.get(&id))
        .and_then(|value| match value {
            TypedValue::Integer(cells) => u16::try_from(*cells).ok().or_else(|| {
                tracing::warn!(
                    key,
                    cells = *cells,
                    "chrome inset outside the cell range; reading it as no chrome"
                );
                None
            }),
            _ => None,
        })
        .unwrap_or(0)
}
