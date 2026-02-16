//! Theme type definitions for Jefe TUI.

use iocraft::Color;
use serde::Deserialize;

/// Color values used by the current toy UI.
///
/// The JSON theme files may contain additional fields; serde ignores those
/// unknown keys so we can keep compatibility while only modeling values the UI
/// actually uses today.
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
    /// Default border color.
    pub border: String,
    /// Border color for focused elements.
    pub border_focused: String,
    /// Selection foreground.
    pub selection_fg: String,
    /// Selection background.
    pub selection_bg: String,
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
        Self::parse_hex(&self.foreground).unwrap_or(Color::Rgb {
            r: 0x6a,
            g: 0x99,
            b: 0x55,
        })
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

    /// Default border.
    pub fn border_color(&self) -> Color {
        Self::parse_hex(&self.border).unwrap_or(self.fg())
    }

    /// Focused border.
    pub fn border_focused_color(&self) -> Color {
        Self::parse_hex(&self.border_focused).unwrap_or(self.bright_fg())
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
                border_focused: c.border_focused_color(),
                bg: c.bg(),
                sel_fg: ThemeColors::parse_hex(&c.selection_fg).unwrap_or(c.bg()),
                sel_bg: ThemeColors::parse_hex(&c.selection_bg).unwrap_or(c.fg()),
            },
            None => Self {
                fg: Color::Rgb {
                    r: 0x6a,
                    g: 0x99,
                    b: 0x55,
                },
                bright: Color::Rgb {
                    r: 0x00,
                    g: 0xff,
                    b: 0x00,
                },
                dim: Color::Rgb {
                    r: 0x4a,
                    g: 0x70,
                    b: 0x35,
                },
                border: Color::Rgb {
                    r: 0x6a,
                    g: 0x99,
                    b: 0x55,
                },
                border_focused: Color::Rgb {
                    r: 0x00,
                    g: 0xff,
                    b: 0x00,
                },
                bg: Color::Rgb { r: 0, g: 0, b: 0 },
                sel_fg: Color::Rgb { r: 0, g: 0, b: 0 },
                sel_bg: Color::Rgb {
                    r: 0x6a,
                    g: 0x99,
                    b: 0x55,
                },
            },
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
    fn resolved_colors_fallback() {
        let rc = ResolvedColors::from_theme(None);
        assert_eq!(rc.fg, Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 });
        assert_eq!(rc.bright, Color::Rgb { r: 0x00, g: 0xff, b: 0x00 });
        assert_eq!(rc.dim, Color::Rgb { r: 0x4a, g: 0x70, b: 0x35 });
        assert_eq!(rc.border, Color::Rgb { r: 0x6a, g: 0x99, b: 0x55 });
        assert_eq!(rc.border_focused, Color::Rgb { r: 0x00, g: 0xff, b: 0x00 });
        assert_eq!(rc.bg, Color::Rgb { r: 0, g: 0, b: 0 });
    }
}
