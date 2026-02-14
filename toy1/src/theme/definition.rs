//! Theme type definitions for Jefe TUI.

use iocraft::Color;
use serde::Deserialize;

/// The kind of theme.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    /// Dark background theme.
    Dark,
    /// Light background theme.
    Light,
    /// ANSI-only colors.
    Ansi,
    /// User-defined custom theme.
    Custom,
}

impl Default for ThemeKind {
    fn default() -> Self {
        Self::Dark
    }
}

/// All color values for a theme, stored as hex strings.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeColors {
    /// Main background color.
    pub background: String,
    /// Primary foreground/text color.
    pub foreground: String,
    /// Bright foreground for emphasis.
    pub bright_foreground: String,
    /// Dimmed foreground.
    pub dim_foreground: String,
    /// Muted color for inactive elements.
    pub muted: String,
    /// Default border color.
    pub border: String,
    /// Border color for focused elements.
    pub border_focused: String,
    /// Panel background.
    pub panel_bg: String,
    /// Panel header text color.
    pub panel_header_fg: String,
    /// Selection foreground.
    pub selection_fg: String,
    /// Selection background.
    pub selection_bg: String,
    /// Scrollbar thumb color.
    pub scrollbar_thumb: String,
    /// Scrollbar track color.
    pub scrollbar_track: String,
    /// Running status color.
    pub status_running: String,
    /// Completed status color.
    pub status_completed: String,
    /// Error status color.
    pub status_error: String,
    /// Waiting status color.
    pub status_waiting: String,
    /// Paused status color.
    pub status_paused: String,
    /// Queued status color.
    pub status_queued: String,
    /// Primary accent.
    pub accent_primary: String,
    /// Warning accent.
    pub accent_warning: String,
    /// Error accent.
    pub accent_error: String,
    /// Success accent.
    pub accent_success: String,
    /// Diff added background.
    pub diff_added_bg: String,
    /// Diff added foreground.
    pub diff_added_fg: String,
    /// Diff removed background.
    pub diff_removed_bg: String,
    /// Diff removed foreground.
    pub diff_removed_fg: String,
    /// Input field background.
    pub input_bg: String,
    /// Input field foreground.
    pub input_fg: String,
    /// Input placeholder text.
    pub input_placeholder: String,
}

impl ThemeColors {
    /// Parse a "#RRGGBB" hex string into an iocraft `Color`.
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

    /// Primary foreground as iocraft Color.
    pub fn fg(&self) -> Color {
        Self::parse_hex(&self.foreground).unwrap_or(Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 })
    }

    /// Background as iocraft Color.
    pub fn bg(&self) -> Color {
        Self::parse_hex(&self.background).unwrap_or(Color::Rgb { r: 0, g: 0, b: 0 })
    }

    /// Bright foreground.
    pub fn bright_fg(&self) -> Color {
        Self::parse_hex(&self.bright_foreground).unwrap_or(self.fg())
    }

    /// Dim foreground.
    pub fn dim_fg(&self) -> Color {
        Self::parse_hex(&self.dim_foreground).unwrap_or(self.fg())
    }

    /// Muted color.
    pub fn muted_color(&self) -> Color {
        Self::parse_hex(&self.muted).unwrap_or(Color::Rgb { r: 0x3a, g: 0x59, b: 0x45 })
    }

    /// Default border.
    pub fn border_color(&self) -> Color {
        Self::parse_hex(&self.border).unwrap_or(self.fg())
    }

    /// Focused border.
    pub fn border_focused_color(&self) -> Color {
        Self::parse_hex(&self.border_focused).unwrap_or(self.bright_fg())
    }

    /// Panel header foreground.
    pub fn panel_header(&self) -> Color {
        Self::parse_hex(&self.panel_header_fg).unwrap_or(self.fg())
    }

    /// Selection background.
    pub fn selection_bg_color(&self) -> Color {
        Self::parse_hex(&self.selection_bg).unwrap_or(self.fg())
    }

    /// Running status color.
    pub fn running(&self) -> Color {
        Self::parse_hex(&self.status_running).unwrap_or(Color::Rgb { r: 0x00, g: 0xff, b: 0x00 })
    }

    /// Completed status color.
    pub fn completed(&self) -> Color {
        Self::parse_hex(&self.status_completed).unwrap_or(Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 })
    }

    /// Error status color.
    pub fn error(&self) -> Color {
        Self::parse_hex(&self.status_error).unwrap_or(Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 })
    }

    /// Waiting status color.
    pub fn waiting(&self) -> Color {
        Self::parse_hex(&self.status_waiting).unwrap_or(Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 })
    }

    /// Paused status color.
    pub fn paused(&self) -> Color {
        Self::parse_hex(&self.status_paused).unwrap_or(Color::Rgb { r: 0x4a, g: 0x70, b: 0x35 })
    }

    /// Queued status color.
    pub fn queued(&self) -> Color {
        Self::parse_hex(&self.status_queued).unwrap_or(Color::Rgb { r: 0x3a, g: 0x59, b: 0x45 })
    }

    /// Primary accent.
    pub fn accent(&self) -> Color {
        Self::parse_hex(&self.accent_primary).unwrap_or(self.fg())
    }

    /// Success accent.
    pub fn success(&self) -> Color {
        Self::parse_hex(&self.accent_success).unwrap_or(Color::Rgb { r: 0x00, g: 0xff, b: 0x00 })
    }
}

/// Resolved colors for a component — pre-extracted from `Option<ThemeColors>`.
/// Avoids repeated Option unwrapping and type-annotation noise in UI code.
#[derive(Clone, Copy)]
pub struct ResolvedColors {
    /// Primary foreground (`#6a9955` — the dominant color).
    pub fg: Color,
    /// Bright foreground (`#00ff00` — use SPARINGLY: running status dot only).
    pub bright: Color,
    /// Dim foreground (`#4a7035` — secondary/muted text).
    pub dim: Color,
    /// Default border.
    pub border: Color,
    /// Focused border (slightly brighter to indicate pane focus).
    pub border_focused: Color,
    /// Background color.
    pub bg: Color,
    /// Selection foreground (black text for inverse-video selection).
    pub sel_fg: Color,
    /// Selection background (green bg for inverse-video selection).
    pub sel_bg: Color,
}

impl ResolvedColors {
    /// Resolve colors from an optional theme, with green-screen fallbacks.
    pub fn from_theme(colors: Option<&ThemeColors>) -> Self {
        match colors {
            Some(c) => Self {
                fg: c.fg(),
                bright: c.bright_fg(),
                dim: c.dim_fg(),
                border: c.border_color(),
                border_focused: c.border_color(), // same color — style change indicates focus
                bg: c.bg(),
                sel_fg: c.bg(), // inverse: black text
                sel_bg: c.fg(), // inverse: green background
            },
            None => Self {
                fg: Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 },
                bright: Color::Rgb { r: 0x00, g: 0xff, b: 0x00 },
                dim: Color::Rgb { r: 0x4a, g: 0x70, b: 0x35 },
                border: Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 },
                border_focused: Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 },
                bg: Color::Rgb { r: 0, g: 0, b: 0 },
                sel_fg: Color::Rgb { r: 0, g: 0, b: 0 },
                sel_bg: Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 },
            },
        }
    }

    /// Pick border color based on focus state.
    pub fn border_for(&self, focused: bool) -> Color {
        if focused {
            self.border_focused
        } else {
            self.border
        }
    }
}

/// A complete theme definition loaded from JSON.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeDefinition {
    /// Display name.
    pub name: String,
    /// URL-safe slug.
    pub slug: String,
    /// Kind.
    pub kind: ThemeKind,
    /// Colors.
    pub colors: ThemeColors,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_valid() {
        assert_eq!(
            ThemeColors::parse_hex("#6a9955"),
            Some(Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 })
        );
    }

    #[test]
    fn parse_hex_black() {
        assert_eq!(ThemeColors::parse_hex("#000000"), Some(Color::Rgb { r: 0, g: 0, b: 0 }));
    }

    #[test]
    fn parse_hex_no_hash() {
        assert!(ThemeColors::parse_hex("6a9955").is_none());
    }

    #[test]
    fn parse_hex_too_short() {
        assert!(ThemeColors::parse_hex("#fff").is_none());
    }

    #[test]
    fn theme_kind_deserialize() {
        let dark: ThemeKind = serde_json::from_str("\"dark\"").expect("deserialize dark");
        assert_eq!(dark, ThemeKind::Dark);
    }

    #[test]
    fn resolved_colors_fallback() {
        let rc = ResolvedColors::from_theme(None);
        assert_eq!(rc.fg, Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 });
        assert_eq!(rc.bright, Color::Rgb { r: 0x00, g: 0xff, b: 0x00 });
        assert_eq!(rc.dim, Color::Rgb { r: 0x4a, g: 0x70, b: 0x35 });
        assert_eq!(rc.border, Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 });
        assert_eq!(rc.border_focused, Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 });
        assert_eq!(rc.bg, Color::Rgb { r: 0, g: 0, b: 0 });
    }
}
