//! Theme manager — tracks available themes and the active selection.

use std::path::Path;

use super::definition::{ThemeColors, ThemeDefinition};
use super::loader::{load_embedded_themes, load_themes_from_dir};

/// The default theme slug.
pub const DEFAULT_THEME_SLUG: &str = "green-screen";

/// Manages available themes and the active theme.
pub struct ThemeManager {
    themes: Vec<ThemeDefinition>,
    active_slug: String,
}

impl ThemeManager {
    /// Create a new manager with embedded themes. Default is Green Screen.
    pub fn new() -> Self {
        Self {
            themes: load_embedded_themes(),
            active_slug: DEFAULT_THEME_SLUG.to_owned(),
        }
    }

    /// Returns the active theme definition.
    pub fn active(&self) -> &ThemeDefinition {
        self.themes
            .iter()
            .find(|t| t.slug == self.active_slug)
            .or_else(|| self.themes.first())
            .expect("must have at least one theme")
    }

    /// Shortcut to the active theme's colors.
    pub fn colors(&self) -> &ThemeColors {
        &self.active().colors
    }

    /// Switch theme by slug. Returns false if not found.
    pub fn set_active(&mut self, slug: &str) -> bool {
        if self.themes.iter().any(|t| t.slug == slug) {
            self.active_slug = slug.to_owned();
            true
        } else {
            false
        }
    }

    /// Load additional themes from a directory.
    pub fn load_external(&mut self, dir: &Path) {
        for theme in load_themes_from_dir(dir) {
            if !self.themes.iter().any(|t| t.slug == theme.slug) {
                self.themes.push(theme);
            }
        }
    }
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_green_screen() {
        let mgr = ThemeManager::new();
        assert_eq!(mgr.active().slug, DEFAULT_THEME_SLUG);
    }

    #[test]
    fn set_active_valid() {
        let mut mgr = ThemeManager::new();
        assert!(mgr.set_active("dracula"));
        assert_eq!(mgr.active().slug, "dracula");
    }

    #[test]
    fn set_active_invalid_keeps_current() {
        let mut mgr = ThemeManager::new();
        let before = mgr.active().slug.clone();
        assert!(!mgr.set_active("nope"));
        assert_eq!(mgr.active().slug, before);
    }

    #[test]
    fn load_external_nonexistent_dir_is_noop() {
        let mut mgr = ThemeManager::new();
        let before = mgr.active().slug.clone();
        mgr.load_external(Path::new("/nonexistent"));
        assert_eq!(mgr.active().slug, before);
    }

    #[test]
    fn colors_are_green_screen() {
        let mgr = ThemeManager::new();
        assert_eq!(mgr.colors().background, "#000000");
        assert_eq!(mgr.colors().foreground, "#6a9955");
    }
}
