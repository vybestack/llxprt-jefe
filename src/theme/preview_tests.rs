//! Behavioral tests for the reversible, generation-bound theme preview.
//!
//! @requirement CW07-03

use crate::domain::ThemeId;

use super::{FileThemeManager, PreviewError, ThemeManager};

fn theme(slug: &str) -> ThemeId {
    ThemeId::parse(slug).unwrap_or_else(|error| panic!("theme fixture: {error}"))
}

fn manager() -> FileThemeManager {
    FileThemeManager::new()
}

#[test]
fn a_preview_shows_the_new_theme_and_remembers_the_one_it_replaced() {
    let mut manager = manager();

    let Ok(token) = manager.apply_preview(1, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };

    assert_eq!(manager.active_theme().slug, "dracula");
    assert_eq!(token.prior_theme(), &theme("green-screen"));
    assert_eq!(token.preview_theme(), &theme("dracula"));
    assert_eq!(token.generation(), 1);
}

#[test]
fn a_second_preview_replaces_the_shown_theme_and_retains_the_original_prior_theme() {
    let mut manager = manager();
    let Ok(first) = manager.apply_preview(1, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };

    let Ok(second) = manager.apply_preview(1, Some(&first), &theme("atom-one-dark")) else {
        panic!("an installed theme must preview");
    };

    assert_eq!(manager.active_theme().slug, "atom-one-dark");
    assert_eq!(
        second.prior_theme(),
        &theme("green-screen"),
        "the theme the user actually started from is retained"
    );
    assert_eq!(second.preview_theme(), &theme("atom-one-dark"));
    assert_ne!(second.id(), first.id(), "each preview has its own identity");
}

#[test]
fn previewing_back_to_the_prior_theme_still_carries_a_token() {
    let mut manager = manager();
    let Ok(first) = manager.apply_preview(4, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };

    let Ok(second) = manager.apply_preview(4, Some(&first), &theme("green-screen")) else {
        panic!("an installed theme must preview");
    };

    assert_eq!(manager.active_theme().slug, "green-screen");
    assert_eq!(second.prior_theme(), &theme("green-screen"));
    assert_eq!(second.preview_theme(), &theme("green-screen"));
}

#[test]
fn reverting_restores_the_exact_prior_theme() {
    let mut manager = manager();
    let Ok(first) = manager.apply_preview(1, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };
    let Ok(second) = manager.apply_preview(1, Some(&first), &theme("atom-one-dark")) else {
        panic!("an installed theme must preview");
    };

    let Ok(()) = manager.revert_preview(1, &second) else {
        panic!("reverting an installed prior theme must succeed");
    };

    assert_eq!(manager.active_theme().slug, "green-screen");
}

#[test]
fn adopting_keeps_the_previewed_theme_active() {
    let mut manager = manager();
    let Ok(token) = manager.apply_preview(1, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };

    let Ok(()) = manager.adopt_preview(1, &token) else {
        panic!("adopting an installed preview must succeed");
    };

    assert_eq!(manager.active_theme().slug, "dracula");
}

#[test]
fn an_uninstalled_theme_is_refused_and_leaves_the_active_theme_alone() {
    let mut manager = manager();

    let Err(error) = manager.apply_preview(1, None, &theme("missing-theme")) else {
        panic!("an uninstalled theme must be refused");
    };

    assert_eq!(error, PreviewError::Unavailable(theme("missing-theme")));
    assert_eq!(manager.active_theme().slug, "green-screen");
}

#[test]
fn a_token_from_a_replaced_draft_is_refused_by_every_operation() {
    let mut manager = manager();
    let Ok(token) = manager.apply_preview(1, None, &theme("dracula")) else {
        panic!("an installed theme must preview");
    };
    let stale = PreviewError::StaleGeneration { token: 1, live: 2 };

    assert_eq!(manager.adopt_preview(2, &token), Err(stale.clone()));
    assert_eq!(manager.revert_preview(2, &token), Err(stale.clone()));
    assert_eq!(
        manager.apply_preview(2, Some(&token), &theme("atom-one-dark")),
        Err(stale)
    );
    assert_eq!(
        manager.active_theme().slug,
        "dracula",
        "a refused operation changes nothing"
    );
}

#[test]
fn every_builtin_slug_is_a_valid_theme_identity() {
    for slug in manager().available_themes() {
        assert!(
            ThemeId::parse(&slug).is_ok(),
            "built-in slug {slug} must be a valid theme identity"
        );
    }
}

#[test]
fn a_custom_theme_whose_slug_is_not_an_identity_is_not_loaded() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("temp themes dir fixture");
    };
    let definition = r##"{
        "name": "Escape",
        "slug": "../escape",
        "kind": "dark",
        "colors": {
            "background": "#000000","foreground": "#ffffff","accent_primary": "#0000ff",
            "accent_secondary": "#888888","accent_success": "#00ff00","accent_warning": "#ffff00",
            "accent_error": "#ff0000","border_default": "#444444","border_focused": "#0000ff",
            "selection_bg": "#0000ff","selection_fg": "#000000"
        }
    }"##;
    if std::fs::write(dir.path().join("escape.json"), definition).is_err() {
        panic!("custom theme fixture must be writable");
    }

    let mut manager = manager();
    let before = manager.available_themes().len();
    manager.load_from_dir(dir.path());

    assert_eq!(manager.available_themes().len(), before);
}

#[test]
fn the_active_identity_follows_the_active_theme() {
    let mut manager = manager();
    assert_eq!(manager.active_theme_id(), theme("green-screen"));

    let Ok(()) = manager.set_active("dracula") else {
        panic!("an installed theme must be selectable");
    };

    assert_eq!(manager.active_theme_id(), theme("dracula"));
}
