//! Theme layer - theme loading, resolution, and Green Screen fallback.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-FUNC-009
//!
//! Green Screen is the default and fallback theme per REQ-FUNC-009.

use iocraft::Color;
use serde::{Deserialize, Serialize};

mod builtins;

pub use builtins::builtin_themes;

/// Theme kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    #[default]
    #[serde(alias = "Dark")]
    Dark,
    #[serde(alias = "Light")]
    Light,
    #[serde(alias = "Ansi")]
    Ansi,
    #[serde(alias = "Custom")]
    Custom,
}

/// Color palette for a theme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub background: String,
    pub foreground: String,
    pub accent_primary: String,
    pub accent_secondary: String,
    pub accent_success: String,
    pub accent_warning: String,
    pub accent_error: String,
    pub border_default: String,
    pub border_focused: String,
    pub selection_bg: String,
    pub selection_fg: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::green_screen()
    }
}

impl ThemeColors {
    /// Green Screen color palette - the default and fallback.
    #[must_use]
    pub fn green_screen() -> Self {
        Self {
            background: String::from("#000000"),
            foreground: String::from("#6a9955"),
            accent_primary: String::from("#6a9955"),
            accent_secondary: String::from("#6a9955"),
            accent_success: String::from("#00ff00"),
            accent_warning: String::from("#6a9955"),
            accent_error: String::from("#6a9955"),
            border_default: String::from("#6a9955"),
            border_focused: String::from("#00ff00"),
            selection_bg: String::from("#6a9955"),
            selection_fg: String::from("#000000"),
        }
    }

    /// Parse a "#RRGGBB" hex string into an iocraft `Color`.
    #[must_use]
    pub fn parse_hex(color_str: &str) -> Option<Color> {
        let hex = color_str.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color::Rgb { r, g, b })
    }
}

/// Resolved colors for a component - pre-extracted iocraft `Color` values.
///
/// Avoids repeated hex parsing and Option unwrapping in UI code.
/// Similar to toy1's `ResolvedColors` pattern.
#[derive(Clone, Copy)]
pub struct ResolvedColors {
    /// Primary foreground (`#6a9955` - the dominant color).
    pub fg: Color,
    /// Bright foreground (`#00ff00` - use sparingly: running status, focus).
    pub bright: Color,
    /// Dim foreground (`#4a7035` - secondary/muted text).
    pub dim: Color,
    /// Default border color.
    pub border: Color,
    /// Focused border (brighter to indicate pane focus).
    pub border_focused: Color,
    /// Background color.
    pub bg: Color,
    /// Selection foreground (for inverse-video selection).
    pub sel_fg: Color,
    /// Selection background (for inverse-video selection).
    pub sel_bg: Color,
}

/// Bundled foreground + background colors for selection highlighting.
///
/// Extracted from [`ResolvedColors`] to keep helper function argument counts
/// under the clippy `too_many_arguments` threshold (6).
#[derive(Debug, Clone, Copy)]
pub struct SelectionColors {
    /// Inverse-video foreground for selected text.
    pub fg: Color,
    /// Inverse-video background for selected text.
    pub bg: Color,
}

/// Bundled foreground + background colors for a list row's default (non-selected)
/// styling.
///
/// Lets render helpers receive themed values instead of `Color::Reset` (which
/// leaks the terminal default background and can produce a visible haze).
#[derive(Debug, Clone, Copy)]
pub struct RowColors {
    /// Default text color for a row.
    pub fg: Color,
    /// Themed background color for a row (avoids `Color::Reset` haze).
    pub bg: Color,
}

impl RowColors {
    /// Derive row colors from a [`ResolvedColors`] snapshot.
    #[must_use]
    pub const fn from_resolved(rc: &ResolvedColors) -> Self {
        Self {
            fg: rc.fg,
            bg: rc.bg,
        }
    }
}

impl SelectionColors {
    /// Derive selection colors from a [`ResolvedColors`] snapshot.
    #[must_use]
    pub const fn from_resolved(rc: &ResolvedColors) -> Self {
        Self {
            fg: rc.sel_fg,
            bg: rc.sel_bg,
        }
    }
}

impl Default for ResolvedColors {
    fn default() -> Self {
        Self::from_theme(None)
    }
}

impl ResolvedColors {
    /// Green Screen fallback colors.
    const GREEN_SCREEN_FG: Color = Color::Rgb {
        r: 0x6a,
        g: 0x99,
        b: 0x55,
    };
    const GREEN_SCREEN_BRIGHT: Color = Color::Rgb {
        r: 0x00,
        g: 0xff,
        b: 0x00,
    };
    const GREEN_SCREEN_DIM: Color = Color::Rgb {
        r: 0x4a,
        g: 0x70,
        b: 0x35,
    };
    const GREEN_SCREEN_BG: Color = Color::Rgb { r: 0, g: 0, b: 0 };

    /// Resolve colors from an optional theme, with Green Screen fallbacks.
    #[must_use]
    pub fn from_theme(colors: Option<&ThemeColors>) -> Self {
        match colors {
            Some(c) => Self {
                fg: ThemeColors::parse_hex(&c.foreground).unwrap_or(Self::GREEN_SCREEN_FG),
                bright: ThemeColors::parse_hex(&c.accent_success)
                    .unwrap_or(Self::GREEN_SCREEN_BRIGHT),
                dim: ThemeColors::parse_hex(&c.accent_secondary).unwrap_or(Self::GREEN_SCREEN_DIM),
                border: ThemeColors::parse_hex(&c.border_default).unwrap_or(Self::GREEN_SCREEN_FG),
                border_focused: ThemeColors::parse_hex(&c.border_focused)
                    .unwrap_or(Self::GREEN_SCREEN_BRIGHT),
                bg: ThemeColors::parse_hex(&c.background).unwrap_or(Self::GREEN_SCREEN_BG),
                sel_fg: ThemeColors::parse_hex(&c.selection_fg).unwrap_or(Self::GREEN_SCREEN_BG),
                sel_bg: ThemeColors::parse_hex(&c.selection_bg).unwrap_or(Self::GREEN_SCREEN_FG),
            },
            None => Self {
                fg: Self::GREEN_SCREEN_FG,
                bright: Self::GREEN_SCREEN_BRIGHT,
                dim: Self::GREEN_SCREEN_DIM,
                border: Self::GREEN_SCREEN_FG,
                border_focused: Self::GREEN_SCREEN_BRIGHT,
                bg: Self::GREEN_SCREEN_BG,
                sel_fg: Self::GREEN_SCREEN_BG,
                sel_bg: Self::GREEN_SCREEN_FG,
            },
        }
    }
}

/// Theme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub name: String,
    pub slug: String,
    pub kind: ThemeKind,
    pub colors: ThemeColors,
}

impl Default for ThemeDefinition {
    fn default() -> Self {
        Self::green_screen()
    }
}

impl ThemeDefinition {
    /// Built-in Green Screen theme.
    #[must_use]
    pub fn green_screen() -> Self {
        Self {
            name: String::from("Green Screen"),
            slug: String::from("green-screen"),
            kind: ThemeKind::Dark,
            colors: ThemeColors::green_screen(),
        }
    }
}

/// Theme resolution error.
#[derive(Debug, Clone)]
pub enum ThemeError {
    NotFound(String),
    ParseError(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(slug) => write!(f, "theme not found: {slug}"),
            Self::ParseError(msg) => write!(f, "theme parse error: {msg}"),
        }
    }
}

impl std::error::Error for ThemeError {}

/// Theme manager trait.
pub trait ThemeManager {
    /// Get available theme slugs.
    fn available_themes(&self) -> Vec<String>;

    /// Get the currently active theme.
    fn active_theme(&self) -> &ThemeDefinition;

    /// Set the active theme by slug.
    /// Falls back to Green Screen if slug is invalid.
    fn set_active(&mut self, slug: &str) -> Result<(), ThemeError>;

    /// Resolve a theme by slug, with Green Screen fallback.
    fn resolve(&self, slug: &str) -> ThemeDefinition;

    /// Get all available themes as `(slug, name)` pairs in a single pass.
    ///
    /// Default implementation uses `available_themes()` + `resolve()`.
    /// Implementors with direct access to the theme list can override
    /// to avoid the O(n²) repeated linear scans.
    fn themes_with_names(&self) -> Vec<(String, String)> {
        self.available_themes()
            .into_iter()
            .map(|slug| {
                let name = self.resolve(&slug).name;
                (slug, name)
            })
            .collect()
    }
}

/// Stub implementation of ThemeManager for testing.
#[derive(Debug)]
pub struct StubThemeManager {
    themes: Vec<ThemeDefinition>,
    active_index: usize,
}

impl Default for StubThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StubThemeManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            themes: vec![ThemeDefinition::green_screen()],
            active_index: 0,
        }
    }
}

impl ThemeManager for StubThemeManager {
    fn available_themes(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.slug.clone()).collect()
    }

    fn active_theme(&self) -> &ThemeDefinition {
        &self.themes[self.active_index]
    }

    fn set_active(&mut self, slug: &str) -> Result<(), ThemeError> {
        if let Some(idx) = self.themes.iter().position(|t| t.slug == slug) {
            self.active_index = idx;
            Ok(())
        } else {
            // Fallback to Green Screen
            self.active_index = 0;
            Err(ThemeError::NotFound(slug.to_string()))
        }
    }

    fn resolve(&self, slug: &str) -> ThemeDefinition {
        self.themes
            .iter()
            .find(|t| t.slug == slug)
            .cloned()
            .unwrap_or_else(ThemeDefinition::green_screen)
    }
}

/// Real implementation of ThemeManager with file loading.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P12
/// @requirement REQ-FUNC-009
#[derive(Debug)]
pub struct FileThemeManager {
    themes: Vec<ThemeDefinition>,
    active_index: usize,
}

impl Default for FileThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileThemeManager {
    /// Create with the full built-in llxprt theme set.
    ///
    /// Green Screen is always first (index 0) so it remains the default and
    /// fallback per REQ-FUNC-009. `builtin_themes()` guarantees this ordering.
    #[must_use]
    pub fn new() -> Self {
        Self {
            themes: builtin_themes(),
            active_index: 0,
        }
    }

    /// Load additional themes from a directory.
    ///
    /// Theme files are JSON with format matching ThemeDefinition.
    /// Invalid files are skipped with no error.
    pub fn load_from_dir(&mut self, dir: &std::path::Path) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(theme) = serde_json::from_str::<ThemeDefinition>(&content)
            {
                // Don't add duplicate slugs
                if !self.themes.iter().any(|t| t.slug == theme.slug) {
                    self.themes.push(theme);
                }
            }
        }
    }

    /// Create from settings theme preference.
    ///
    /// Applies the theme from settings, falling back to Green Screen.
    #[must_use]
    pub fn with_theme(mut self, slug: &str) -> Self {
        let _ = self.set_active(slug); // Ignore error, fallback applied
        self
    }
}

impl ThemeManager for FileThemeManager {
    fn available_themes(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.slug.clone()).collect()
    }

    fn active_theme(&self) -> &ThemeDefinition {
        &self.themes[self.active_index]
    }

    fn set_active(&mut self, slug: &str) -> Result<(), ThemeError> {
        if let Some(idx) = self.themes.iter().position(|t| t.slug == slug) {
            self.active_index = idx;
            Ok(())
        } else {
            // Fallback to Green Screen (always index 0)
            self.active_index = 0;
            Err(ThemeError::NotFound(slug.to_string()))
        }
    }

    fn resolve(&self, slug: &str) -> ThemeDefinition {
        self.themes
            .iter()
            .find(|t| t.slug == slug)
            .cloned()
            .unwrap_or_else(ThemeDefinition::green_screen)
    }

    fn themes_with_names(&self) -> Vec<(String, String)> {
        self.themes
            .iter()
            .map(|t| (t.slug.clone(), t.name.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_green_screen() {
        let mgr = StubThemeManager::new();
        assert_eq!(mgr.active_theme().slug, "green-screen");
    }

    #[test]
    fn green_screen_colors_are_dark() {
        let theme = ThemeDefinition::green_screen();
        assert_eq!(theme.kind, ThemeKind::Dark);
        assert_eq!(theme.colors.background, "#000000");
        assert_eq!(theme.colors.foreground, "#6a9955");
    }

    #[test]
    fn resolve_unknown_returns_green_screen() {
        let mgr = StubThemeManager::new();
        let theme = mgr.resolve("nonexistent");
        assert_eq!(theme.slug, "green-screen");
    }

    #[test]
    fn set_active_unknown_falls_back_to_green_screen() {
        let mut mgr = StubThemeManager::new();
        let result = mgr.set_active("nonexistent");
        assert!(result.is_err());
        assert_eq!(mgr.active_theme().slug, "green-screen");
    }

    #[test]
    fn file_manager_loads_builtin_themes() {
        let mgr = FileThemeManager::new();
        let slugs = mgr.available_themes();
        // Built-ins include at least the full pickable set.
        assert!(slugs.contains(&"green-screen".to_string()));
        assert!(slugs.contains(&"dracula".to_string()));
        assert!(slugs.contains(&"atom-one-dark".to_string()));
        assert!(slugs.len() >= 14);
        // Green Screen is always first (index 0) — guaranteed by builtin_themes().
        assert_eq!(slugs[0], "green-screen");
        assert_eq!(mgr.active_theme().slug, "green-screen");
    }

    #[test]
    fn file_manager_set_active_switches_to_builtin() {
        let mut mgr = FileThemeManager::new();
        assert!(mgr.set_active("dracula").is_ok());
        assert_eq!(mgr.active_theme().slug, "dracula");
    }

    #[test]
    fn load_from_dir_loads_custom_themes() {
        let temp = tempfile::tempdir().unwrap_or_else(|_| panic!("create temp themes dir"));

        let custom = r##"{
            "name": "My Custom",
            "slug": "my-custom",
            "kind": "dark",
            "colors": {
                "background": "#000000",
                "foreground": "#ffffff",
                "accent_primary": "#0000ff",
                "accent_secondary": "#888888",
                "accent_success": "#00ff00",
                "accent_warning": "#ffff00",
                "accent_error": "#ff0000",
                "border_default": "#444444",
                "border_focused": "#0000ff",
                "selection_bg": "#0000ff",
                "selection_fg": "#000000"
            }
        }"##;
        std::fs::write(temp.path().join("my-custom.json"), custom)
            .unwrap_or_else(|_| panic!("write custom theme"));

        let mut mgr = FileThemeManager::new();
        mgr.load_from_dir(temp.path());

        let slugs = mgr.available_themes();
        assert!(slugs.contains(&"my-custom".to_string()));
        assert!(mgr.set_active("my-custom").is_ok());
        assert_eq!(mgr.active_theme().slug, "my-custom");
    }

    #[test]
    fn load_from_dir_skips_malformed_json() {
        let temp = tempfile::tempdir().unwrap_or_else(|_| panic!("create temp themes dir"));

        // Malformed JSON — should be skipped, not panic.
        std::fs::write(temp.path().join("broken.json"), "{ this is not valid json")
            .unwrap_or_else(|_| panic!());
        // Missing required fields — deserialization fails, should be skipped.
        std::fs::write(temp.path().join("incomplete.json"), r#"{"name":"NoSlug"}"#)
            .unwrap_or_else(|_| panic!());

        let mut mgr = FileThemeManager::new();
        let before = mgr.available_themes().len();
        mgr.load_from_dir(temp.path());
        let after = mgr.available_themes().len();

        // No new themes added; only built-ins remain.
        assert_eq!(before, after);
    }

    #[test]
    fn load_from_dir_dedupes_duplicate_slugs() {
        let temp = tempfile::tempdir().unwrap_or_else(|_| panic!("create temp themes dir"));

        let theme_a = r##"{
            "name": "Custom A",
            "slug": "dup-slug",
            "kind": "dark",
            "colors": {
                "background": "#000000","foreground": "#ffffff","accent_primary": "#0000ff",
                "accent_secondary": "#888888","accent_success": "#00ff00","accent_warning": "#ffff00",
                "accent_error": "#ff0000","border_default": "#444444","border_focused": "#0000ff",
                "selection_bg": "#0000ff","selection_fg": "#000000"
            }
        }"##;
        let theme_b = r##"{
            "name": "Custom B",
            "slug": "dup-slug",
            "kind": "light",
            "colors": {
                "background": "#ffffff","foreground": "#000000","accent_primary": "#0000ff",
                "accent_secondary": "#888888","accent_success": "#00ff00","accent_warning": "#ffff00",
                "accent_error": "#ff0000","border_default": "#444444","border_focused": "#0000ff",
                "selection_bg": "#0000ff","selection_fg": "#000000"
            }
        }"##;
        std::fs::write(temp.path().join("a.json"), theme_a).unwrap_or_else(|_| panic!());
        std::fs::write(temp.path().join("b.json"), theme_b).unwrap_or_else(|_| panic!());

        let mut mgr = FileThemeManager::new();
        mgr.load_from_dir(temp.path());

        let dup_count = mgr
            .available_themes()
            .iter()
            .filter(|s| *s == "dup-slug")
            .count();
        assert_eq!(dup_count, 1, "duplicate slug must be deduped");
    }

    #[test]
    fn load_from_dir_handles_missing_directory_gracefully() {
        let mut mgr = FileThemeManager::new();
        let before = mgr.available_themes().len();
        // Non-existent directory — no panic, no themes added.
        mgr.load_from_dir(std::path::Path::new("/nonexistent/jefe/themes/dir"));
        assert_eq!(mgr.available_themes().len(), before);
    }

    #[test]
    fn load_from_dir_ignores_non_json_files() {
        let temp = tempfile::tempdir().unwrap_or_else(|_| panic!("create temp themes dir"));

        std::fs::write(temp.path().join("readme.txt"), "not a theme").unwrap_or_else(|_| panic!());
        std::fs::write(temp.path().join("config.toml"), "slug = 'ignored'")
            .unwrap_or_else(|_| panic!());

        let mut mgr = FileThemeManager::new();
        let before = mgr.available_themes().len();
        mgr.load_from_dir(temp.path());
        assert_eq!(mgr.available_themes().len(), before);
    }
}
