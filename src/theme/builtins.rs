//! Built-in theme definitions ported from the llxprt-code theme set.
//!
//! Source of truth: `packages/cli/src/ui/themes/*.ts` in the llxprt-code repo.
//!
//! # Palette mapping (jefe `ThemeColors` <- llxprt `ColorsTheme`)
//!
//! jefe's TUI renders chrome only (no syntax-highlighted diffs/code yet), so we
//! keep jefe's 11-slot `ThemeColors` and map the richer llxprt palette onto it:
//!
//! | jefe slot            | llxprt source        | rationale                              |
//! |----------------------|----------------------|----------------------------------------|
//! | `background`         | `Background`         | direct                                 |
//! | `foreground`         | `Foreground`*        | primary text (*ANSI themes leave this |
//! |                      |                      | empty; fall back to `AccentBlue`)      |
//! | `accent_primary`     | `AccentBlue`         | primary accent (links, focus)          |
//! | `accent_secondary`   | `DarkGray`           | secondary/muted text                   |
//! | `accent_success`     | `AccentGreen`        | running status / success               |
//! | `accent_warning`     | `AccentYellow`       | warnings                               |
//! | `accent_error`       | `AccentRed`          | errors / dead agents                   |
//! | `border_default`     | `Gray`               | default borders                        |
//! | `border_focused`     | `AccentBlue`         | focused borders (matches llxprt)       |
//! | `selection_bg`       | `AccentBlue`         | selection background                   |
//! | `selection_fg`       | `Background`         | inverse selection foreground           |
//!
//! ## ANSI color resolution
//!
//! ANSI themes (`ansi`, `ansi-light`) use terminal-native color names
//! (`black`, `white`, `blue`, etc.) which jefe's hex parser cannot resolve.
//! These are mapped to their standard xterm-256 RGB hex values at port time:
//!
//! | name       | hex       | | name        | hex       |
//! |------------|-----------|-|-------------|-----------|
//! | black      | `#000000` | | white       | `#ffffff` |
//! | blue       | `#0000ff` | | bluebright  | `#5555ff` |
//! | red        | `#ff0000` | | green       | `#008000` |
//! | yellow     | `#ffff00` | | cyan        | `#00ffff` |
//! | magenta    | `#ff00ff` | | purple      | `#a020f0` |
//! | gray       | `#808080` | | orange      | `#ffa500` |
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-FUNC-009

use super::{ThemeColors, ThemeDefinition, ThemeKind};

/// Helper to build a `ThemeColors` from the llxprt palette fields.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn map_colors(
    background: &str,
    foreground: &str,
    accent_blue: &str,
    dark_gray: &str,
    accent_green: &str,
    accent_yellow: &str,
    accent_red: &str,
    gray: &str,
) -> ThemeColors {
    // foreground falls back to accent_blue when empty (ANSI themes).
    let fg = if foreground.is_empty() {
        accent_blue.to_owned()
    } else {
        foreground.to_owned()
    };
    ThemeColors {
        background: background.to_owned(),
        foreground: fg,
        accent_primary: accent_blue.to_owned(),
        accent_secondary: dark_gray.to_owned(),
        accent_success: accent_green.to_owned(),
        accent_warning: accent_yellow.to_owned(),
        accent_error: accent_red.to_owned(),
        border_default: gray.to_owned(),
        border_focused: accent_blue.to_owned(),
        selection_bg: accent_blue.to_owned(),
        selection_fg: background.to_owned(),
    }
}

/// Green Screen - the default and fallback theme.
fn green_screen() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Green Screen"),
        slug: String::from("green-screen"),
        kind: ThemeKind::Dark,
        colors: ThemeColors::green_screen(),
    }
}

fn ayu_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Ayu Dark"),
        slug: String::from("ayu-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#0b0e14", "#bfbdb6", "#39BAE6", "#24272e", "#AAD94C", "#FFB454", "#F26D78", "#3D4149",
        ),
    }
}

fn ayu_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Ayu Light"),
        slug: String::from("ayu-light"),
        kind: ThemeKind::Light,
        colors: map_colors(
            "#f8f9fa", "#5c6166", "#399ee6", "#cfd1d4", "#86b300", "#f2ae49", "#f07171", "#a6aaaf",
        ),
    }
}

fn atom_one_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Atom One Dark"),
        slug: String::from("atom-one-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#282c34", "#abb2bf", "#61aeee", "#424752", "#98c379", "#e6c07b", "#e06c75", "#5c6370",
        ),
    }
}

fn dracula() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Dracula"),
        slug: String::from("dracula"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#282a36", "#f8f8f2", "#8be9fd", "#424752", "#50fa7b", "#f1fa8c", "#ff5555", "#6272a4",
        ),
    }
}

fn default_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Default Dark"),
        slug: String::from("default-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#1E1E2E", "", "#89B4FA", "#45475a", "#A6E3A1", "#F9E2AF", "#F38BA8", "#6C7086",
        ),
    }
}

fn default_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Default Light"),
        slug: String::from("default-light"),
        kind: ThemeKind::Light,
        colors: map_colors(
            "#FAFAFA", "", "#3B82F6", "#c8cdd5", "#3CA84B", "#D5A40A", "#DD4C4C", "#97a0b0",
        ),
    }
}

fn github_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("GitHub Dark"),
        slug: String::from("github-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#24292e", "#d1d5da", "#79B8FF", "#474e55", "#85E89D", "#FFAB70", "#F97583", "#6A737D",
        ),
    }
}

fn github_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("GitHub Light"),
        slug: String::from("github-light"),
        kind: ThemeKind::Light,
        colors: map_colors(
            "#f8f8f8", "#24292E", "#445588", "#c8c8c8", "#008080", "#990073", "#dd1144", "#999999",
        ),
    }
}

fn google_code() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Google Code"),
        slug: String::from("google-code"),
        kind: ThemeKind::Light,
        colors: map_colors(
            "#ffffff", "#444444", "#000088", "#cbcfd7", "#008800", "#666600", "#880000", "#97a0b0",
        ),
    }
}

fn shades_of_purple() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Shades of Purple"),
        slug: String::from("shades-of-purple"),
        kind: ThemeKind::Dark,
        colors: map_colors(
            "#1e1e3f", "#e3dfff", "#a599e9", "#4f4b6e", "#A5FF90", "#fad000", "#ff628c", "#726c86",
        ),
    }
}

fn xcode() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("XCode"),
        slug: String::from("xcode"),
        kind: ThemeKind::Light,
        colors: map_colors(
            "#ffffff", "#444444", "#1c00cf", "#dfdfdf", "#007400", "#836C28", "#c41a16", "#c0c0c0",
        ),
    }
}

fn ansi() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("ANSI"),
        slug: String::from("ansi"),
        kind: ThemeKind::Ansi,
        colors: map_colors(
            "#000000", "", "#0000ff", "#808080", "#008000", "#ffff00", "#ff0000", "#808080",
        ),
    }
}

fn ansi_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("ANSI Light"),
        slug: String::from("ansi-light"),
        kind: ThemeKind::Ansi,
        colors: map_colors(
            "#ffffff", "", "#0000ff", "#808080", "#008000", "#ffa500", "#ff0000", "#808080",
        ),
    }
}

/// All built-in themes, in display order.
///
/// Green Screen is first so it remains the default/fallback (index 0).
#[must_use]
pub fn builtin_themes() -> Vec<ThemeDefinition> {
    vec![
        green_screen(),
        ayu_dark(),
        ayu_light(),
        atom_one_dark(),
        dracula(),
        default_dark(),
        default_light(),
        github_dark(),
        github_light(),
        google_code(),
        shades_of_purple(),
        xcode(),
        ansi(),
        ansi_light(),
    ]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn builtins_include_all_fourteen_pickable_themes() {
        let themes = builtin_themes();
        let expected: &[&str] = &[
            "green-screen",
            "ayu-dark",
            "ayu-light",
            "atom-one-dark",
            "dracula",
            "default-dark",
            "default-light",
            "github-dark",
            "github-light",
            "google-code",
            "shades-of-purple",
            "xcode",
            "ansi",
            "ansi-light",
        ];
        let slugs: Vec<&str> = themes.iter().map(|t| t.slug.as_str()).collect();
        for slug in expected {
            assert!(slugs.contains(slug), "missing built-in theme: {slug}");
        }
        assert_eq!(themes.len(), 14, "exactly 14 pickable built-in themes");
    }

    #[test]
    fn green_screen_is_first_so_it_stays_default_fallback() {
        let themes = builtin_themes();
        assert_eq!(themes[0].slug, "green-screen");
    }

    #[test]
    fn builtins_have_unique_slugs() {
        let themes = builtin_themes();
        let mut slugs: Vec<&str> = themes.iter().map(|t| t.slug.as_str()).collect();
        slugs.sort_unstable();
        let initial = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), initial, "built-in theme slugs must be unique");
    }

    #[test]
    fn builtin_colors_resolve_via_parse_hex() {
        // Every built-in color slot must parse to a valid Color.
        for theme in builtin_themes() {
            let c = &theme.colors;
            assert!(
                ThemeColors::parse_hex(&c.background).is_some(),
                "{} bg",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.foreground).is_some(),
                "{} fg",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.accent_primary).is_some(),
                "{} p",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.accent_secondary).is_some(),
                "{} s",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.accent_success).is_some(),
                "{} ok",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.accent_warning).is_some(),
                "{} warn",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.accent_error).is_some(),
                "{} err",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.border_default).is_some(),
                "{} bd",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.border_focused).is_some(),
                "{} bf",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.selection_bg).is_some(),
                "{} selbg",
                theme.slug
            );
            assert!(
                ThemeColors::parse_hex(&c.selection_fg).is_some(),
                "{} selfg",
                theme.slug
            );
        }
    }

    #[test]
    fn ansi_themes_use_ansi_kind() {
        let themes = builtin_themes();
        let ansi_count = themes
            .iter()
            .filter(|t| t.slug == "ansi" || t.slug == "ansi-light")
            .count();
        assert_eq!(ansi_count, 2, "both ANSI themes must be present");
        for theme in &themes {
            if theme.slug == "ansi" || theme.slug == "ansi-light" {
                assert_eq!(theme.kind, ThemeKind::Ansi, "{}", theme.slug);
            }
        }
    }

    #[test]
    fn light_themes_classified_light() {
        let light_slugs = [
            "ayu-light",
            "default-light",
            "github-light",
            "google-code",
            "xcode",
        ];
        let themes = builtin_themes();
        for theme in &themes {
            if light_slugs.contains(&theme.slug.as_str()) {
                assert_eq!(theme.kind, ThemeKind::Light, "{}", theme.slug);
            }
        }
    }

    #[test]
    fn dark_themes_classified_dark() {
        let dark_slugs = [
            "green-screen",
            "ayu-dark",
            "atom-one-dark",
            "dracula",
            "default-dark",
            "github-dark",
            "shades-of-purple",
        ];
        let themes = builtin_themes();
        for theme in &themes {
            if dark_slugs.contains(&theme.slug.as_str()) {
                assert_eq!(theme.kind, ThemeKind::Dark, "{}", theme.slug);
            }
        }
    }
}
