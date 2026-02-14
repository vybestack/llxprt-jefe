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

    /// Cycle to the next theme.
    pub fn cycle_next(&mut self) {
        if self.themes.is_empty() {
            return;
        }
        let idx = self
            .themes
            .iter()
            .position(|t| t.slug == self.active_slug)
            .unwrap_or(0);
        let next = (idx + 1) % self.themes.len();
        self.active_slug = self.themes[next].slug.clone();
    }

    /// All available themes.
    pub fn available(&self) -> &[ThemeDefinition] {
        &self.themes
    }

    /// (slug, name) pairs.
    pub fn names(&self) -> Vec<(&str, &str)> {
        self.themes.iter().map(|t| (t.slug.as_str(), t.name.as_str())).collect()
    }

    /// Load additional themes from a directory.
    pub fn load_external(&mut self, dir: &Path) {
        for theme in load_themes_from_dir(dir) {
            if !self.themes.iter().any(|t| t.slug == theme.slug) {
                self.themes.push(theme);
            }
        }
    }

    /// Active theme slug.
    pub fn active_slug(&self) -> &str {
        &self.active_slug
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
        assert_eq!(mgr.active_slug(), "green-screen");
    }

    #[test]
    fn set_active_valid() {
        let mut mgr = ThemeManager::new();
        assert!(mgr.set_active("dracula"));
        assert_eq!(mgr.active().slug, "dracula");
    }

    #[test]
    fn set_active_invalid() {
        let mut mgr = ThemeManager::new();
        assert!(!mgr.set_active("nope"));
        assert_eq!(mgr.active_slug(), "green-screen");
    }

    #[test]
    fn cycle_wraps() {
        let mut mgr = ThemeManager::new();
        let n = mgr.available().len();
        let start = mgr.active_slug().to_owned();
        for _ in 0..n {
            mgr.cycle_next();
        }
        assert_eq!(mgr.active_slug(), start);
    }

    #[test]
    fn colors_are_green_screen() {
        let mgr = ThemeManager::new();
        assert_eq!(mgr.colors().background, "#000000");
        assert_eq!(mgr.colors().foreground, "#6a9955");
    }
}
