//! Theme layer - theme loading, resolution, and Green Screen fallback.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001
//! @requirement REQ-FUNC-009
//!
//! Green Screen is the default and fallback theme per REQ-FUNC-009.

use iocraft::Color;
use serde::{Deserialize, Serialize};

/// Theme kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
    Ansi,
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
    /// Create with Green Screen as the only theme.
    #[must_use]
    pub fn new() -> Self {
        Self {
            themes: vec![ThemeDefinition::green_screen()],
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
}
