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
/// Each row's `selected` and `active` flags carry all rendering information;
/// the raw index is intentionally excluded to avoid out-of-bounds hazards.
#[must_use]
pub fn theme_picker_view(state: &AppState) -> Option<Vec<ThemePickerRow>> {
    let ModalState::ThemePicker {
        available_themes,
        selected_index,
        active_slug,
        ..
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

    Some(rows)
}

/// Pure projection of the theme picker's "Apply jefe theme to agent" toggle.
///
/// Returns `None` when no theme picker modal is open.
#[must_use]
pub fn theme_picker_override_view(state: &AppState) -> Option<bool> {
    let ModalState::ThemePicker { override_theme, .. } = &state.modal else {
        return None;
    };
    Some(*override_theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ModalState;

    fn picker_state(themes: Vec<(String, String)>, selected: usize, active: &str) -> AppState {
        AppState {
            modal: ModalState::ThemePicker {
                available_themes: themes,
                selected_index: selected,
                active_slug: active.to_owned(),
                override_theme: false,
            },
            ..AppState::default()
        }
    }

    #[test]
    fn returns_none_when_picker_not_open() {
        let state = AppState::default();
        assert!(theme_picker_view(&state).is_none());
    }

    #[test]
    fn returns_rows_when_picker_open() {
        let state = picker_state(
            vec![
                ("green-screen".into(), "Green Screen".into()),
                ("dracula".into(), "Dracula".into()),
            ],
            1,
            "green-screen",
        );

        let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
        assert_eq!(rows.len(), 2);
        assert!(rows[1].selected);
        assert!(!rows[0].selected);
        assert!(rows[0].active);
        assert!(!rows[1].active);
    }

    #[test]
    fn rows_carry_slug_and_name() {
        let state = picker_state(
            vec![("atom-one-dark".into(), "Atom One Dark".into())],
            0,
            "atom-one-dark",
        );

        let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
        assert_eq!(rows[0].slug, "atom-one-dark");
        assert_eq!(rows[0].name, "Atom One Dark");
        assert!(rows[0].active);
    }

    #[test]
    fn empty_picker_returns_empty_rows() {
        let state = picker_state(vec![], 0, "");

        let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
        assert!(rows.is_empty());
    }

    #[test]
    fn active_and_selected_are_independent() {
        // Active slug is "b" but selection is on "a" — verify they don't overlap.
        let state = picker_state(
            vec![
                ("a".into(), "Theme A".into()),
                ("b".into(), "Theme B".into()),
            ],
            0,
            "b",
        );

        let rows = theme_picker_view(&state).unwrap_or_else(|| panic!("picker open"));
        assert!(rows[0].selected);
        assert!(!rows[0].active);
        assert!(!rows[1].selected);
        assert!(rows[1].active);
    }
}
