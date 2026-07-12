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

/// llxprt palette fields used to build a jefe `ThemeColors`.
///
/// This struct carries the subset of `ColorsTheme` fields that the palette
/// mapping (documented at the top of this module) consumes.
struct LlxprtPalette {
    background: &'static str,
    foreground: &'static str,
    accent_blue: &'static str,
    dark_gray: &'static str,
    accent_green: &'static str,
    accent_yellow: &'static str,
    accent_red: &'static str,
    gray: &'static str,
}

/// Helper to build a `ThemeColors` from the llxprt palette fields.
///
/// # Empty foreground contract
///
/// Some llxprt themes (ANSI, ANSI Light) leave `foreground` as an empty string,
/// relying on the terminal's default text color. jefe's `ThemeColors::parse_hex`
/// requires a `#RRGGBB` value, so empty foregrounds fall back to `accent_blue`
/// here. This is an intentional part of the `LlxprtPalette` → `ThemeColors`
/// mapping — direct construction of `ThemeColors` with empty strings will
/// silently fall back to Green Screen at render time via `ResolvedColors`.
fn map_colors(p: &LlxprtPalette) -> ThemeColors {
    // foreground falls back to accent_blue when empty (ANSI themes).
    let fg = if p.foreground.is_empty() {
        p.accent_blue.to_owned()
    } else {
        p.foreground.to_owned()
    };
    ThemeColors {
        background: p.background.to_owned(),
        foreground: fg,
        accent_primary: p.accent_blue.to_owned(),
        accent_secondary: p.dark_gray.to_owned(),
        accent_success: p.accent_green.to_owned(),
        accent_warning: p.accent_yellow.to_owned(),
        accent_error: p.accent_red.to_owned(),
        border_default: p.gray.to_owned(),
        border_focused: p.accent_blue.to_owned(),
        selection_bg: p.accent_blue.to_owned(),
        selection_fg: p.background.to_owned(),
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
        colors: map_colors(&LlxprtPalette {
            background: "#0b0e14",
            foreground: "#bfbdb6",
            accent_blue: "#39BAE6",
            dark_gray: "#24272e",
            accent_green: "#AAD94C",
            accent_yellow: "#FFB454",
            accent_red: "#F26D78",
            gray: "#3D4149",
        }),
    }
}

fn ayu_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Ayu Light"),
        slug: String::from("ayu-light"),
        kind: ThemeKind::Light,
        colors: map_colors(&LlxprtPalette {
            background: "#f8f9fa",
            foreground: "#5c6166",
            accent_blue: "#399ee6",
            dark_gray: "#cfd1d4",
            accent_green: "#86b300",
            accent_yellow: "#f2ae49",
            accent_red: "#f07171",
            gray: "#a6aaaf",
        }),
    }
}

fn atom_one_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Atom One Dark"),
        slug: String::from("atom-one-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(&LlxprtPalette {
            background: "#282c34",
            foreground: "#abb2bf",
            accent_blue: "#61aeee",
            dark_gray: "#424752",
            accent_green: "#98c379",
            accent_yellow: "#e6c07b",
            accent_red: "#e06c75",
            gray: "#5c6370",
        }),
    }
}

fn dracula() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Dracula"),
        slug: String::from("dracula"),
        kind: ThemeKind::Dark,
        colors: map_colors(&LlxprtPalette {
            background: "#282a36",
            foreground: "#f8f8f2",
            accent_blue: "#8be9fd",
            dark_gray: "#424752",
            accent_green: "#50fa7b",
            accent_yellow: "#f1fa8c",
            accent_red: "#ff5555",
            gray: "#6272a4",
        }),
    }
}

fn default_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Default Dark"),
        slug: String::from("default-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(&LlxprtPalette {
            background: "#1E1E2E",
            foreground: "",
            accent_blue: "#89B4FA",
            dark_gray: "#45475a",
            accent_green: "#A6E3A1",
            accent_yellow: "#F9E2AF",
            accent_red: "#F38BA8",
            gray: "#6C7086",
        }),
    }
}

fn default_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Default Light"),
        slug: String::from("default-light"),
        kind: ThemeKind::Light,
        colors: map_colors(&LlxprtPalette {
            background: "#FAFAFA",
            foreground: "",
            accent_blue: "#3B82F6",
            dark_gray: "#c8cdd5",
            accent_green: "#3CA84B",
            accent_yellow: "#D5A40A",
            accent_red: "#DD4C4C",
            gray: "#97a0b0",
        }),
    }
}

fn github_dark() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("GitHub Dark"),
        slug: String::from("github-dark"),
        kind: ThemeKind::Dark,
        colors: map_colors(&LlxprtPalette {
            background: "#24292e",
            foreground: "#d1d5da",
            accent_blue: "#79B8FF",
            dark_gray: "#474e55",
            accent_green: "#85E89D",
            accent_yellow: "#FFAB70",
            accent_red: "#F97583",
            gray: "#6A737D",
        }),
    }
}

fn github_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("GitHub Light"),
        slug: String::from("github-light"),
        kind: ThemeKind::Light,
        colors: map_colors(&LlxprtPalette {
            background: "#f8f8f8",
            foreground: "#24292E",
            accent_blue: "#445588",
            dark_gray: "#c8c8c8",
            accent_green: "#008080",
            accent_yellow: "#990073",
            accent_red: "#dd1144",
            gray: "#999999",
        }),
    }
}

fn google_code() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Google Code"),
        slug: String::from("google-code"),
        kind: ThemeKind::Light,
        colors: map_colors(&LlxprtPalette {
            background: "#ffffff",
            foreground: "#444444",
            accent_blue: "#000088",
            dark_gray: "#cbcfd7",
            accent_green: "#008800",
            accent_yellow: "#666600",
            accent_red: "#880000",
            gray: "#97a0b0",
        }),
    }
}

fn shades_of_purple() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("Shades of Purple"),
        slug: String::from("shades-of-purple"),
        kind: ThemeKind::Dark,
        colors: map_colors(&LlxprtPalette {
            background: "#1e1e3f",
            foreground: "#e3dfff",
            accent_blue: "#a599e9",
            dark_gray: "#4f4b6e",
            accent_green: "#A5FF90",
            accent_yellow: "#fad000",
            accent_red: "#ff628c",
            gray: "#726c86",
        }),
    }
}

fn xcode() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("XCode"),
        slug: String::from("xcode"),
        kind: ThemeKind::Light,
        colors: map_colors(&LlxprtPalette {
            background: "#ffffff",
            foreground: "#444444",
            accent_blue: "#1c00cf",
            dark_gray: "#dfdfdf",
            accent_green: "#007400",
            accent_yellow: "#836C28",
            accent_red: "#c41a16",
            gray: "#c0c0c0",
        }),
    }
}

fn ansi() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("ANSI"),
        slug: String::from("ansi"),
        kind: ThemeKind::Ansi,
        colors: map_colors(&LlxprtPalette {
            background: "#000000",
            foreground: "",
            accent_blue: "#0000ff",
            dark_gray: "#808080",
            accent_green: "#008000",
            accent_yellow: "#ffff00",
            accent_red: "#ff0000",
            gray: "#808080",
        }),
    }
}

fn ansi_light() -> ThemeDefinition {
    ThemeDefinition {
        name: String::from("ANSI Light"),
        slug: String::from("ansi-light"),
        kind: ThemeKind::Ansi,
        colors: map_colors(&LlxprtPalette {
            background: "#ffffff",
            foreground: "",
            accent_blue: "#0000ff",
            dark_gray: "#808080",
            accent_green: "#008000",
            // Orange (#ffa500) is used instead of yellow because yellow is
            // illegible on a white background. This matches the llxprt-code
            // source (ansi-light.ts uses AccentYellow: 'orange').
            accent_yellow: "#ffa500",
            accent_red: "#ff0000",
            gray: "#808080",
        }),
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
            let all_colors = [
                ("background", &c.background),
                ("foreground", &c.foreground),
                ("accent_primary", &c.accent_primary),
                ("accent_secondary", &c.accent_secondary),
                ("accent_success", &c.accent_success),
                ("accent_warning", &c.accent_warning),
                ("accent_error", &c.accent_error),
                ("border_default", &c.border_default),
                ("border_focused", &c.border_focused),
                ("selection_bg", &c.selection_bg),
                ("selection_fg", &c.selection_fg),
            ];
            for (label, color_str) in all_colors {
                assert!(
                    ThemeColors::parse_hex(color_str).is_some(),
                    "{}: {} = {} did not parse",
                    theme.slug,
                    label,
                    color_str
                );
            }
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
