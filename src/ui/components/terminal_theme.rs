//! Jefe-theme override logic for the embedded agent terminal (issue #179).
//!
//! Extracted from `terminal_view.rs` to keep that file under the
//! source-file-size limit. These are pure functions over
//! `TerminalCellStyle` + the override config; no iocraft component/runtime.

use iocraft::prelude::*;

/// Bundled jefe-theme override for the embedded agent terminal (issue #179).
///
/// When `enabled` is true, runs whose fg/bg is `Color::Reset` (terminal
/// default) are repainted with `fg`/`bg` so the agent pane matches jefe's
/// theme. Explicitly-styled cells pass through unchanged. Carried as a single
/// value to keep `paint_terminal_cells` under the argument-count limit.
#[derive(Debug, Clone, Copy)]
pub struct TerminalThemeOverride {
    pub enabled: bool,
    pub fg: iocraft::Color,
    pub bg: iocraft::Color,
}

impl Default for TerminalThemeOverride {
    fn default() -> Self {
        Self {
            enabled: false,
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }
}

/// Resolve a cell style's intensity into iocraft's exclusive `Weight` enum.
///
/// iocraft's `Weight` cannot represent bold+dim simultaneously, so when both
/// are set (ANSI `DIM_BOLD`, which carries both the BOLD and DIM bits) bold
/// takes precedence — losing dim is less harmful than losing bold. A dim-only
/// style maps to `Weight::Light` (ANSI SGR 2) so dimming survives even on a
/// transparent default foreground (issue #179).
#[must_use]
pub fn terminal_weight(style: &crate::runtime::TerminalCellStyle) -> Weight {
    if style.bold {
        Weight::Bold
    } else if style.dim {
        Weight::Light
    } else {
        Weight::Normal
    }
}

/// Background fill for the agent terminal *content* area (issue #179).
///
/// Override OFF: `Color::Reset` (transparent). The embedded agent's default
/// cells let the host terminal's real background show through instead of
/// jefe's theme bg bleeding in. Override ON: jefe's theme bg, so blank /
/// trailing / default regions are consistently themed (explicit agent cell
/// backgrounds overpaint this fill).
///
/// This is the single source of truth for the content-area fill and is shared
/// by the content container and the empty-state box so both honor the same
/// transparency contract. Jefe's chrome (outer border, title bar) does NOT
/// use this helper — it always carries `rc.bg`.
#[must_use]
pub fn terminal_content_background(
    override_theme: bool,
    theme_bg: iocraft::Color,
) -> iocraft::Color {
    if override_theme {
        theme_bg
    } else {
        iocraft::Color::Reset
    }
}

/// Whether a color represents the terminal default (transparent) background.
///
/// When `paint_terminal_cells` encounters a run whose bg is `Color::Reset`,
/// it skips `set_background_color` so the host terminal's real default
/// background shows through the transparent content container (issue #179).
pub fn is_default_bg(color: iocraft::Color) -> bool {
    matches!(color, iocraft::Color::Reset)
}

/// Whether a color represents the terminal default (transparent) foreground.
pub fn is_default_fg(color: iocraft::Color) -> bool {
    matches!(color, iocraft::Color::Reset)
}

/// Resolve a run's effective foreground and background colors for painting.
///
/// Returns `(fg, bg)` where `bg` is `None` when the run should NOT paint a
/// background (the container/host-terminal default shows through).
///
/// - Override OFF (default): terminal-default channels (`Color::Reset`) pass
///   through unchanged. A `Reset` background yields `None` so it stays
///   transparent (issue #179 bug fix).
/// - Override ON: terminal-default channels are replaced with jefe's theme
///   colors (`theme_fg`/`theme_bg`); explicitly-colored channels pass through.
///   A run whose effective background is still `Reset` after resolution yields
///   `None` (transparent).
///
/// Override guarantees an *opaque* result even if the sourced theme color is
/// itself `Reset`: a `Reset` theme channel is normalized to a concrete
/// fallback (black bg / white fg) so override can never produce an unintended
/// transparent background. The foreground fallback is a concrete, non-`Reset`
/// color but is not guaranteed to contrast with every possible background.
/// Today `ResolvedColors` always supplies concrete `Rgb` values, so this is a
/// defensive contract guarantee rather than a live code path.
///
/// Transformed cells (inverse, selection, cursor) already carry concrete ANSI
/// contrast colors from the runtime layer, so they are never `Reset` and thus
/// retain their high-contrast appearance in both modes — cursors and selection
/// highlights stay visible against any themed background.
#[must_use]
pub fn resolve_run_colors(
    style: &crate::runtime::TerminalCellStyle,
    theme_override: TerminalThemeOverride,
) -> (iocraft::Color, Option<iocraft::Color>) {
    if theme_override.enabled {
        // Default channels become the theme color, normalized to a concrete
        // fallback when the theme color itself is Reset so override always
        // paints an opaque background and a visible foreground.
        let fg = if is_default_fg(style.fg) {
            normalize_override_fg(theme_override.fg)
        } else {
            style.fg
        };
        let bg = if is_default_bg(style.bg) {
            normalize_override_bg(theme_override.bg)
        } else {
            style.bg
        };
        (fg, Some(bg))
    } else {
        let bg = if is_default_bg(style.bg) {
            None
        } else {
            Some(style.bg)
        };
        (style.fg, bg)
    }
}

/// Concrete foreground to use when override is enabled but the theme fg is the
/// terminal default. A concrete (non-`Reset`) fallback guarantees override
/// paints an opaque cell; it is not guaranteed to contrast with every
/// background (today `ResolvedColors` always supplies concrete colors).
fn normalize_override_fg(color: iocraft::Color) -> iocraft::Color {
    if is_default_fg(color) {
        iocraft::Color::White
    } else {
        color
    }
}

/// Concrete background to use when override is enabled but the theme bg is the
/// terminal default. Black is opaque and matches the terminal default look.
fn normalize_override_bg(color: iocraft::Color) -> iocraft::Color {
    if is_default_bg(color) {
        iocraft::Color::Black
    } else {
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::TerminalCellStyle;
    use iocraft::Color;

    // --- terminal_content_background (issue #179) ---

    #[test]
    fn content_background_transparent_when_override_off() {
        assert_eq!(
            terminal_content_background(false, Color::Blue),
            Color::Reset
        );
    }

    #[test]
    fn content_background_is_theme_bg_when_override_on() {
        assert_eq!(terminal_content_background(true, Color::Blue), Color::Blue);
    }

    #[test]
    fn content_background_off_ignores_theme_bg() {
        assert_eq!(
            terminal_content_background(
                false,
                Color::Rgb {
                    r: 30,
                    g: 30,
                    b: 30
                }
            ),
            Color::Reset
        );
    }

    #[test]
    fn content_background_passes_reset_theme_bg_through_when_override_on() {
        assert_eq!(
            terminal_content_background(true, Color::Reset),
            Color::Reset
        );
    }

    // --- terminal_weight (issue #179 DIM preservation) ---

    fn cell_style(bold: bool, dim: bool) -> TerminalCellStyle {
        TerminalCellStyle {
            fg: Color::Reset,
            bg: Color::Reset,
            bold,
            dim,
            underline: false,
        }
    }

    #[test]
    fn weight_normal_when_neither_bold_nor_dim() {
        assert_eq!(terminal_weight(&cell_style(false, false)), Weight::Normal);
    }

    #[test]
    fn weight_bold_for_bold_only() {
        assert_eq!(terminal_weight(&cell_style(true, false)), Weight::Bold);
    }

    #[test]
    fn weight_light_for_dim_only() {
        assert_eq!(terminal_weight(&cell_style(false, true)), Weight::Light);
    }

    #[test]
    fn weight_bold_wins_over_dim() {
        assert_eq!(terminal_weight(&cell_style(true, true)), Weight::Bold);
    }

    // --- is_default_bg / is_default_fg (issue #179) ---

    #[test]
    fn is_default_bg_true_for_reset() {
        assert!(is_default_bg(Color::Reset));
    }

    #[test]
    fn is_default_bg_false_for_concrete_colors() {
        assert!(!is_default_bg(Color::Black));
        assert!(!is_default_bg(Color::White));
        assert!(!is_default_bg(Color::Rgb { r: 0, g: 0, b: 0 }));
        assert!(!is_default_bg(Color::AnsiValue(0)));
    }

    #[test]
    fn is_default_fg_true_for_reset() {
        assert!(is_default_fg(Color::Reset));
    }

    #[test]
    fn is_default_fg_false_for_concrete_colors() {
        assert!(!is_default_fg(Color::White));
        assert!(!is_default_fg(Color::Rgb {
            r: 255,
            g: 255,
            b: 255
        }));
    }

    // --- resolve_run_colors off/on matrix (issue #179) ---

    fn run_style(fg: Color, bg: Color) -> TerminalCellStyle {
        TerminalCellStyle {
            fg,
            bg,
            bold: false,
            dim: false,
            underline: false,
        }
    }

    fn override_on(fg: Color, bg: Color) -> TerminalThemeOverride {
        TerminalThemeOverride {
            enabled: true,
            fg,
            bg,
        }
    }

    fn override_off() -> TerminalThemeOverride {
        TerminalThemeOverride::default()
    }

    #[test]
    fn resolve_default_bg_is_transparent_when_override_off() {
        let (fg, bg) = resolve_run_colors(&run_style(Color::White, Color::Reset), override_off());
        assert_eq!(fg, Color::White);
        assert!(bg.is_none(), "default bg must be transparent (None)");
    }

    #[test]
    fn resolve_explicit_colors_pass_through_when_override_off() {
        let (fg, bg) = resolve_run_colors(&run_style(Color::White, Color::Black), override_off());
        assert_eq!(fg, Color::White);
        assert_eq!(bg, Some(Color::Black));
    }

    #[test]
    fn resolve_override_maps_default_channels_to_theme() {
        let (fg, bg) = resolve_run_colors(
            &run_style(Color::Reset, Color::Reset),
            override_on(Color::Green, Color::Blue),
        );
        assert_eq!(fg, Color::Green);
        assert_eq!(bg, Some(Color::Blue));
    }

    #[test]
    fn resolve_override_leaves_explicit_channels_unchanged() {
        let (fg, bg) = resolve_run_colors(
            &run_style(Color::Red, Color::Yellow),
            override_on(Color::Green, Color::Blue),
        );
        assert_eq!(fg, Color::Red);
        assert_eq!(bg, Some(Color::Yellow));
    }

    #[test]
    fn resolve_override_maps_only_default_bg_with_explicit_fg() {
        let (fg, bg) = resolve_run_colors(
            &run_style(Color::Red, Color::Reset),
            override_on(Color::Green, Color::Blue),
        );
        assert_eq!(fg, Color::Red);
        assert_eq!(bg, Some(Color::Blue));
    }

    #[test]
    fn resolve_override_maps_only_default_fg_with_explicit_bg() {
        let (fg, bg) = resolve_run_colors(
            &run_style(Color::Reset, Color::Yellow),
            override_on(Color::Green, Color::Blue),
        );
        assert_eq!(fg, Color::Green);
        assert_eq!(bg, Some(Color::Yellow));

        let (fg, bg) = resolve_run_colors(
            &run_style(Color::Reset, Color::Rgb { r: 1, g: 2, b: 3 }),
            override_on(Color::Green, Color::Blue),
        );
        assert_eq!(fg, Color::Green);
        assert_eq!(bg, Some(Color::Rgb { r: 1, g: 2, b: 3 }));
    }

    #[test]
    fn resolve_override_normalizes_reset_theme_bg_to_opaque() {
        let (_fg, bg) = resolve_run_colors(
            &run_style(Color::Reset, Color::Reset),
            override_on(Color::Reset, Color::Reset),
        );
        assert_eq!(bg, Some(Color::Black), "override must be opaque");
    }

    #[test]
    fn resolve_override_normalizes_reset_theme_fg_to_concrete() {
        let (fg, _bg) = resolve_run_colors(
            &run_style(Color::Reset, Color::Reset),
            override_on(Color::Reset, Color::Reset),
        );
        assert_eq!(fg, Color::White, "override fg must be concrete (non-Reset)");
    }
}
