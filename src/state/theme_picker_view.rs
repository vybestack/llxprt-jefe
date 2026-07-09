//! Pure projection for the theme picker modal.
//!
//! This is the iocraft-free, testable view-model layer: it derives the
//! rows/selection/highlight data that the iocraft `ThemePicker` screen renders.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-009

use crate::state::{AppState, ModalState};

/// A single row in the theme picker list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePickerRow {
    pub slug: String,
    pub name: String,
    /// Whether this row is the currently selected (highlighted) one.
    pub selected: bool,
    /// Whether this theme is the active theme (the one applied in the manager).
    pub active: bool,
}

/// Pure projection of the theme picker modal into renderable rows.
///
/// Returns `None` when no theme picker modal is open.
#[must_use]
pub fn theme_picker_view(state: &AppState) -> Option<(Vec<ThemePickerRow>, usize)> {
    let ModalState::ThemePicker {
        available_themes,
        selected_index,
        active_slug,
    } = &state.modal
    else {
        return None;
    };

    let rows = available_themes
        .iter()
        .enumerate()
        .map(|(idx, (slug, name))| ThemePickerRow {
            slug: slug.clone(),
            name: name.clone(),
            selected: idx == *selected_index,
            active: slug == active_slug,
        })
        .collect();

    Some((rows, *selected_index))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default
)]
mod tests {
    use super::*;
    use crate::state::ModalState;

    #[test]
    fn returns_none_when_picker_not_open() {
        let state = AppState::default();
        assert!(theme_picker_view(&state).is_none());
    }

    #[test]
    fn returns_rows_when_picker_open() {
        let mut state = AppState::default();
        state.modal = ModalState::ThemePicker {
            available_themes: vec![
                ("green-screen".into(), "Green Screen".into()),
                ("dracula".into(), "Dracula".into()),
            ],
            selected_index: 1,
            active_slug: "green-screen".into(),
        };

        let (rows, selected) = theme_picker_view(&state).expect("picker open");
        assert_eq!(rows.len(), 2);
        assert_eq!(selected, 1);
        assert!(rows[1].selected);
        assert!(!rows[0].selected);
        // Active marker follows active_slug, not selection.
        assert!(rows[0].active);
        assert!(!rows[1].active);
    }

    #[test]
    fn rows_carry_slug_and_name() {
        let mut state = AppState::default();
        state.modal = ModalState::ThemePicker {
            available_themes: vec![("atom-one-dark".into(), "Atom One Dark".into())],
            selected_index: 0,
            active_slug: "atom-one-dark".into(),
        };

        let (rows, _) = theme_picker_view(&state).expect("picker open");
        assert_eq!(rows[0].slug, "atom-one-dark");
        assert_eq!(rows[0].name, "Atom One Dark");
        assert!(rows[0].active);
    }

    #[test]
    fn empty_picker_returns_empty_rows() {
        let mut state = AppState::default();
        state.modal = ModalState::ThemePicker {
            available_themes: vec![],
            selected_index: 0,
            active_slug: String::new(),
        };

        let (rows, _) = theme_picker_view(&state).expect("picker open");
        assert!(rows.is_empty());
    }
}
