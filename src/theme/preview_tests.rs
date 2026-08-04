//! Behavioral tests for the reversible, generation-bound theme preview token.
//!
//! @requirement CW07-03

use crate::domain::ThemeId;

use super::{FileThemeManager, PreviewError, ThemeManager, ThemePreviewToken};

fn theme(slug: &str) -> ThemeId {
    ThemeId::parse(slug).unwrap_or_else(|error| panic!("theme fixture: {error}"))
}

/// Apply a preview over a session that is wearing the default theme.
///
/// The active theme is fixed at Green Screen because these tests are about the
/// token's own algebra: only the *first* preview reads the active theme, so
/// varying it would not change what any of them assert.
fn applied(
    generation: u64,
    current_preview: Option<&ThemePreviewToken>,
    slug: &str,
) -> ThemePreviewToken {
    ThemePreviewToken::apply(
        generation,
        current_preview,
        &theme("green-screen"),
        theme(slug),
    )
    .unwrap_or_else(|error| panic!("a live preview must apply: {error}"))
}

#[test]
fn a_preview_names_the_theme_it_shows_and_the_one_it_replaced() {
    let token = applied(7, None, "dracula");

    assert_eq!(token.prior_theme(), &theme("green-screen"));
    assert_eq!(token.preview_theme(), &theme("dracula"));
    assert_eq!(token.generation(), 7);
    assert!(token.is_live(7));
}

#[test]
fn replacing_a_preview_retains_the_theme_the_first_one_replaced() {
    let first = applied(7, None, "dracula");

    let second = ThemePreviewToken::apply(
        7,
        Some(&first),
        // The manager is showing `dracula` by now; the token, not the manager,
        // is what remembers where the user started.
        &theme("dracula"),
        theme("atom-one-dark"),
    )
    .unwrap_or_else(|error| panic!("a live preview must apply: {error}"));

    assert_eq!(second.prior_theme(), &theme("green-screen"));
    assert_eq!(second.preview_theme(), &theme("atom-one-dark"));
    assert_ne!(second.id(), first.id(), "each preview has its own identity");
}

#[test]
fn reverting_yields_the_exact_prior_theme_and_adopting_yields_the_preview() {
    let token = applied(7, None, "dracula");

    assert_eq!(token.clone().revert(), theme("green-screen"));
    assert_eq!(token.adopt(), theme("dracula"));
}

#[test]
fn a_token_from_a_replaced_draft_cannot_issue_a_further_preview() {
    let stale = applied(7, None, "dracula");

    let refused = ThemePreviewToken::apply(
        8,
        Some(&stale),
        &theme("green-screen"),
        theme("atom-one-dark"),
    );

    assert_eq!(
        refused,
        Err(PreviewError::StaleGeneration { token: 7, live: 8 })
    );
    assert!(!stale.is_live(8));
}

#[test]
fn selecting_an_installed_theme_shows_it() {
    let mut manager = FileThemeManager::new();

    let Ok(()) = manager.select(&theme("dracula")) else {
        panic!("an installed theme must be selectable");
    };

    assert_eq!(manager.active_theme().slug, "dracula");
    assert_eq!(manager.active_theme_id(), theme("dracula"));
}

#[test]
fn selecting_an_uninstalled_theme_is_refused_and_shows_nothing_else() {
    let mut manager = FileThemeManager::new();
    let Ok(()) = manager.select(&theme("dracula")) else {
        panic!("an installed theme must be selectable");
    };

    let refused = manager.select(&theme("missing-theme"));

    assert_eq!(
        refused,
        Err(PreviewError::Unavailable(theme("missing-theme")))
    );
    assert_eq!(
        manager.active_theme().slug,
        "dracula",
        "a refused selection substitutes no third theme"
    );
}

#[test]
fn availability_is_answered_from_the_installed_list() {
    let manager = FileThemeManager::new();

    assert!(manager.has_theme(&theme("green-screen")));
    assert!(!manager.has_theme(&theme("missing-theme")));
}

#[test]
fn every_builtin_slug_is_a_valid_theme_identity() {
    for slug in FileThemeManager::new().available_themes() {
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
    assert!(
        std::fs::write(dir.path().join("escape.json"), definition).is_ok(),
        "custom theme fixture must be writable"
    );

    let mut manager = FileThemeManager::new();
    let before = manager.available_themes().len();
    manager.load_from_dir(dir.path());

    assert_eq!(manager.available_themes().len(), before);
}
