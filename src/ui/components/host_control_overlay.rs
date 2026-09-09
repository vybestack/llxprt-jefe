//! Shared rendering shell for host-owned overlay control projections.

use iocraft::prelude::*;

use crate::host_controls::{HostControlRow, HostControlRowStyle, HostControlTitleStyle};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for one host-owned overlay projected by a sealed HostControl factory.
#[derive(Default, Props)]
pub struct HostControlOverlayProps {
    /// Operator-facing overlay title.
    pub title: String,
    /// Projection-selected title weight.
    pub(crate) title_style: HostControlTitleStyle,
    /// Factory-projected control rows.
    pub(crate) rows: Vec<HostControlRow>,
    /// First visible projected row.
    pub viewport: usize,
    /// Maximum visible projected rows.
    pub viewport_rows: usize,
    /// Overlay width.
    pub width: u32,
    /// Overlay height from the same typed layout used by hit-testing.
    pub height: u32,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Operator-facing footer.
    pub footer: String,
}

/// Render one host-owned overlay without inspecting screen or package identity.
#[component]
pub fn HostControlOverlay(props: &HostControlOverlayProps) -> impl Into<AnyElement<'static>> {
    let colors = ResolvedColors::from_theme(Some(&props.colors));
    let visible: Vec<AnyElement<'static>> = props
        .rows
        .iter()
        .skip(props.viewport)
        .take(props.viewport_rows)
        .map(|row| {
            let color = match row.style {
                HostControlRowStyle::Normal => colors.fg,
                HostControlRowStyle::Bright => colors.bright,
                HostControlRowStyle::Dim => colors.dim,
            };
            element! { Text(content: row.text.clone(), color) }.into_any()
        })
        .collect();
    let title_weight = match props.title_style {
        HostControlTitleStyle::Emphasized => Weight::Bold,
        HostControlTitleStyle::Plain => Weight::Normal,
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: props.width,
            height: props.height,
            border_style: BorderStyle::Round,
            border_color: colors.border_focused,
            background_color: colors.bg,
            padding: 1u32,
        ) {
            Text(content: props.title.clone(), weight: title_weight, color: colors.fg)
            #(visible)
            Text(content: props.footer.clone(), color: colors.dim)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_controls::{HostControlRow, HostControlRowStyle, HostControlTitleStyle};

    fn test_colors() -> ThemeColors {
        ThemeColors {
            background: "#000000".to_owned(),
            foreground: "#010203".to_owned(),
            accent_primary: "#010203".to_owned(),
            accent_secondary: "#070809".to_owned(),
            accent_success: "#040506".to_owned(),
            accent_warning: "#010203".to_owned(),
            accent_error: "#010203".to_owned(),
            border_default: "#010203".to_owned(),
            border_focused: "#0a0b0c".to_owned(),
            selection_bg: "#010203".to_owned(),
            selection_fg: "#000000".to_owned(),
        }
    }

    fn render_ansi(
        title: &str,
        title_style: HostControlTitleStyle,
        rows: Vec<HostControlRow>,
    ) -> String {
        let mut element = element! {
            Box(width: 40u32, height: 12u32) {
                HostControlOverlay(
                    title: title.to_owned(),
                    title_style,
                    rows,
                    viewport: 0usize,
                    viewport_rows: 8usize,
                    width: 40u32,
                    height: 12u32,
                    colors: test_colors(),
                    footer: String::new(),
                )
            }
        };
        let canvas = element.render(Some(40));
        let mut buffer = Vec::new();
        canvas
            .write_ansi(&mut buffer)
            .unwrap_or_else(|error| panic!("write ANSI: {error}"));
        String::from_utf8_lossy(&buffer).into_owned()
    }

    fn strip_ansi(input: &str) -> String {
        let mut output = String::new();
        let mut chars = input.chars();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                for escaped in chars.by_ref() {
                    if escaped.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                output.push(character);
            }
        }
        output
    }

    #[test]
    fn shell_resolves_row_styles_to_theme_colors() {
        let rows = vec![
            HostControlRow::plain("normal"),
            HostControlRow::plain("bright").with_style(HostControlRowStyle::Bright),
            HostControlRow::plain("dim").with_style(HostControlRowStyle::Dim),
        ];

        let ansi = render_ansi("Overlay", HostControlTitleStyle::Emphasized, rows);

        assert!(
            ansi.contains("\u{1b}[38;2;1;2;3m"),
            "normal color missing: {ansi:?}"
        );
        assert!(
            ansi.contains("\u{1b}[38;2;4;5;6m"),
            "bright color missing: {ansi:?}"
        );
        assert!(
            ansi.contains("\u{1b}[38;2;7;8;9m"),
            "dim color missing: {ansi:?}"
        );
    }

    #[test]
    fn form_title_keeps_legacy_indent_and_plain_weight() {
        let ansi = render_ansi(" Edit Agent", HostControlTitleStyle::Plain, Vec::new());
        let plain = strip_ansi(&ansi);

        assert!(
            plain.contains("│  Edit Agent"),
            "legacy title indent missing: {plain:?}"
        );
        assert!(
            !ansi.contains("\u{1b}[1m"),
            "form title must remain plain: {ansi:?}"
        );
    }

    #[test]
    fn non_form_title_keeps_existing_emphasis() {
        let ansi = render_ansi("Help", HostControlTitleStyle::Emphasized, Vec::new());

        assert!(
            ansi.contains("\u{1b}[1m"),
            "overlay title must stay bold: {ansi:?}"
        );
    }
}
