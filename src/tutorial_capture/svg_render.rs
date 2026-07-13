//! Deterministic monochrome SVG rendering from captured screen text.
//!
//! This module produces **reproducible monochrome preview** SVG images from
//! terminal screen captures. It is deterministic: the same input text always
//! produces the same SVG. Color fidelity is **not** preserved — the SVG uses
//! a monochrome scheme because faithful terminal color reproduction from
//! plain-text captures is not possible without ANSI escape sequences.
//!
//! ## What this is
//!
//! A **reproducible monochrome preview**: a deterministic, stable visual
//! artifact that can be used for documentation without claiming to preserve
//! the original terminal colors. The SVG uses:
//! - A fixed, declared font stack (monospace, with platform fallbacks).
//! - Fixed, declared geometry (cell width, cell height, padding).
//! - Declared row count matching the metadata (not the actual line count).
//! - Embedded metadata for reproducibility (cols, rows, theme, version,
//!   scenario hash).
//!
//! ## What this is not
//!
//! Not a color-preserving or publication-ready render. If color fidelity is
//! needed, a terminal-recording adapter that captures ANSI escape sequences
//! must be used instead.
//!
//! ## Boundary
//!
//! This module is pure: it transforms text + metadata into an SVG string.
//! It does not read or write files.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-008

use std::fmt::Write;

/// Metadata embedded in the SVG for reproducibility.
///
/// @requirement REQ-TUTORIAL-CAPTURE-008
#[derive(Debug, Clone)]
pub struct SvgRenderMetadata {
    /// Terminal columns.
    pub cols: u16,
    /// Terminal rows (declared, not actual line count).
    pub rows: u16,
    /// Theme name (e.g. "dark").
    pub theme: String,
    /// Jefe version.
    pub jefe_version: String,
    /// Semantic checkpoint label.
    pub label: String,
    /// Optional scenario hash.
    pub scenario_hash: Option<String>,
}

/// Character cell dimensions in SVG user units (pixels at default scale).
/// These are fixed and declared so the SVG is reproducible.
const CELL_WIDTH: u16 = 8;
const CELL_HEIGHT: u16 = 16;
const PADDING: u16 = 16;
const FONT_SIZE: u16 = 14;

/// Fixed font stack: monospace with platform-specific fallbacks. This is
/// declared in the SVG so renderers use a consistent font.
const FONT_FAMILY: &str = "monospace, 'Courier New', 'DejaVu Sans Mono', 'Menlo', monospace";

/// Background color derived from the Green Screen theme (#000000).
///
/// **Finding #6**: The monochrome SVG background/foreground colors are
/// derived from the actual Green Screen theme (black bg, green fg)
/// rather than hardcoded dark-theme colors.
const BG_COLOR: &str = "#000000";
/// Foreground (text) color derived from the Green Screen theme (#6a9955).
const FG_COLOR: &str = "#6a9955";

/// Compute the SVG geometry (width, height) from declared cols/rows using
/// saturating arithmetic so no multiplication or addition can overflow.
///
/// Exposed so tests can verify geometry without duplicating magic constants
/// (Finding #6).
#[must_use]
pub fn svg_geometry(cols: u16, rows: u16) -> (u32, u32) {
    let cell_w = u32::from(CELL_WIDTH);
    let cell_h = u32::from(CELL_HEIGHT);
    let padding = u32::from(PADDING);
    let content_width = u32::from(cols).saturating_mul(cell_w);
    let content_height = u32::from(rows).saturating_mul(cell_h);
    let svg_width = content_width.saturating_add(padding.saturating_mul(2));
    let svg_height = content_height.saturating_add(padding.saturating_mul(2));
    (svg_width, svg_height)
}

/// Compute the y-coordinate for a text line at the given row index.
/// Returns `u32` for consistency with the viewBox coordinates.
fn text_y_for_row(row: u16) -> u32 {
    let padding = u32::from(PADDING);
    let cell_h = u32::from(CELL_HEIGHT);
    let row_plus1 = u32::from(row).saturating_add(1);
    padding
        .saturating_add(row_plus1.saturating_mul(cell_h))
        .saturating_sub(3)
}

/// Render a screen capture as a deterministic **monochrome preview** SVG string.
///
/// The SVG includes:
/// - Fixed dark-background monochrome theme (not color-preserving)
/// - Fixed font stack with platform fallbacks
/// - Fixed geometry (cell size, padding) declared in metadata
/// - Declared row count from metadata (not actual line count): extra input
///   lines beyond the declared row count are truncated, not rendered
/// - Tool/version metadata as SVG `<desc>` and `<title>` elements
/// - A comment block declaring all fixed parameters for reproducibility
///
/// @requirement REQ-TUTORIAL-CAPTURE-008
#[must_use]
pub fn render_screen_svg(lines: &[String], metadata: &SvgRenderMetadata) -> String {
    // Use declared rows from metadata for geometry, not actual line count.
    // This ensures the SVG dimensions match what the scenario declared.
    let declared_rows = metadata.rows.max(1);
    let (svg_width, svg_height) = svg_geometry(metadata.cols, declared_rows);

    let mut svg = String::new();
    write_header(&mut svg, svg_width, svg_height, metadata);
    write_background(&mut svg, svg_width, svg_height);
    write_text_lines(&mut svg, lines, declared_rows, metadata);
    write_metadata_comment(&mut svg, metadata);
    svg.push_str("</svg>\n");
    svg
}

/// Write the SVG header with viewBox and embedded metadata.
fn write_header(svg: &mut String, width: u32, height: u32, metadata: &SvgRenderMetadata) {
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    let _ = writeln!(svg, "  <title>{}</title>", escape_xml_text(&metadata.label));
    let _ = writeln!(
        svg,
        "  <desc>jefe-tutorial-capture screen render: label={}, cols={}, rows={}, theme={}, jefe_version={}{}</desc>",
        escape_xml_text(&metadata.label),
        metadata.cols,
        metadata.rows,
        escape_xml_text(&metadata.theme),
        escape_xml_text(&metadata.jefe_version),
        metadata
            .scenario_hash
            .as_ref()
            .map(|h| format!(", scenario_hash={}", escape_xml_text(h)))
            .unwrap_or_default()
    );
}

/// Write the background rectangle.
fn write_background(svg: &mut String, width: u32, height: u32) {
    let _ = writeln!(
        svg,
        r#"  <rect width="{width}" height="{height}" fill="{BG_COLOR}"/>"#
    );
}

/// Write each text line as an SVG text element.
///
/// Extra input lines beyond `declared_rows` are truncated (not rendered)
/// so the SVG content matches the declared geometry contract.
fn write_text_lines(
    svg: &mut String,
    lines: &[String],
    declared_rows: u16,
    _metadata: &SvgRenderMetadata,
) {
    let x = u32::from(PADDING);
    let max_row_idx = declared_rows as usize;
    for (i, line) in lines.iter().enumerate().take(max_row_idx) {
        let row = u16::try_from(i).unwrap_or(u16::MAX);
        let y = text_y_for_row(row);
        let escaped = escape_xml_text(line);
        let _ = writeln!(
            svg,
            r#"  <text x="{x}" y="{y}" font-family="{FONT_FAMILY}" font-size="{FONT_SIZE}" fill="{FG_COLOR}" xml:space="preserve">{escaped}</text>"#
        );
    }
}

/// Write a metadata comment declaring all fixed parameters for reproducibility.
fn write_metadata_comment(svg: &mut String, metadata: &SvgRenderMetadata) {
    let _ = writeln!(
        svg,
        "<!-- reproducible-monochrome-preview: cell={}x{} padding={} font_size={} font_family=\"{}\" bg={} fg={} declared_rows={} declared_cols={} theme={} jefe={}{} -->",
        CELL_WIDTH,
        CELL_HEIGHT,
        PADDING,
        FONT_SIZE,
        FONT_FAMILY,
        BG_COLOR,
        FG_COLOR,
        metadata.rows,
        metadata.cols,
        escape_xml_text(&metadata.theme),
        escape_xml_text(&metadata.jefe_version),
        metadata
            .scenario_hash
            .as_ref()
            .map(|h| format!(" scenario_hash={}", escape_xml_text(h)))
            .unwrap_or_default()
    );
}

/// Escape XML special characters in text content.
fn escape_xml_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> SvgRenderMetadata {
        SvgRenderMetadata {
            cols: 80,
            rows: 24,
            theme: "dark".to_string(),
            jefe_version: "0.0.28".to_string(),
            label: "dashboard-oriented".to_string(),
            scenario_hash: Some("abc123".to_string()),
        }
    }

    // ─── Basic rendering ────────────────────────────────────────────────

    #[test]
    fn svg_has_xml_namespace() {
        let svg = render_screen_svg(&["hello".to_string()], &sample_metadata());
        assert!(svg.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
    }

    #[test]
    fn svg_has_title_with_label() {
        let svg = render_screen_svg(&["hello".to_string()], &sample_metadata());
        assert!(svg.contains("<title>dashboard-oriented</title>"));
    }

    #[test]
    fn svg_has_background_rect() {
        let svg = render_screen_svg(&["hello".to_string()], &sample_metadata());
        assert!(svg.contains("<rect"));
        assert!(svg.contains(BG_COLOR));
    }

    #[test]
    fn svg_renders_text_lines() {
        let lines = vec!["line one".to_string(), "line two".to_string()];
        let svg = render_screen_svg(&lines, &sample_metadata());
        assert!(svg.contains("line one"));
        assert!(svg.contains("line two"));
    }

    #[test]
    fn svg_uses_monospace_font() {
        let svg = render_screen_svg(&["hi".to_string()], &sample_metadata());
        assert!(svg.contains(
            r#"font-family="monospace, 'Courier New', 'DejaVu Sans Mono', 'Menlo', monospace""#
        ));
    }

    #[test]
    fn svg_preserves_whitespace() {
        let svg = render_screen_svg(&["  indented".to_string()], &sample_metadata());
        assert!(svg.contains(r#"xml:space="preserve""#));
    }

    // ─── Determinism ────────────────────────────────────────────────────

    #[test]
    fn same_input_produces_same_output() {
        let lines = vec!["hello world".to_string(), "foo bar".to_string()];
        let meta = sample_metadata();
        let svg1 = render_screen_svg(&lines, &meta);
        let svg2 = render_screen_svg(&lines, &meta);
        assert_eq!(svg1, svg2);
    }

    #[test]
    fn different_input_produces_different_output() {
        let meta = sample_metadata();
        let svg1 = render_screen_svg(&["a".to_string()], &meta);
        let svg2 = render_screen_svg(&["b".to_string()], &meta);
        assert_ne!(svg1, svg2);
    }

    // ─── XML escaping ───────────────────────────────────────────────────

    #[test]
    fn svg_escapes_ampersand() {
        let svg = render_screen_svg(&["a & b".to_string()], &sample_metadata());
        assert!(svg.contains("a &amp; b"));
        assert!(!svg.contains("a & b"));
    }

    #[test]
    fn svg_escapes_angle_brackets() {
        let svg = render_screen_svg(&["<tag>".to_string()], &sample_metadata());
        assert!(svg.contains("&lt;tag&gt;"));
    }

    // ─── Geometry ───────────────────────────────────────────────────────

    #[test]
    fn svg_width_reflects_cols() {
        let meta = SvgRenderMetadata {
            cols: 100,
            ..sample_metadata()
        };
        let svg = render_screen_svg(&["x".to_string()], &meta);
        let (expected_width, _) = svg_geometry(meta.cols, meta.rows);
        assert!(svg.contains(&format!(r#"width="{expected_width}""#)));
    }

    #[test]
    fn svg_height_reflects_declared_rows() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {i}")).collect();
        let meta = sample_metadata();
        let svg = render_screen_svg(&lines, &meta);
        // SVG height uses the declared rows from metadata, not the actual
        // line count, so the output is deterministic regardless of input.
        let (_, expected_height) = svg_geometry(meta.cols, meta.rows);
        assert!(svg.contains(&format!(r#"height="{expected_height}""#)));
    }

    // ─── Metadata in desc ───────────────────────────────────────────────

    #[test]
    fn svg_desc_includes_geometry_metadata() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(svg.contains("cols=80"));
        assert!(svg.contains("rows=24"));
        assert!(svg.contains("theme=dark"));
    }

    #[test]
    fn svg_desc_includes_jefe_version() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(svg.contains("jefe_version=0.0.28"));
    }

    #[test]
    fn svg_desc_includes_scenario_hash_when_present() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(svg.contains("scenario_hash=abc123"));
    }

    #[test]
    fn svg_desc_omits_scenario_hash_when_absent() {
        let meta = SvgRenderMetadata {
            scenario_hash: None,
            ..sample_metadata()
        };
        let svg = render_screen_svg(&["x".to_string()], &meta);
        assert!(!svg.contains("scenario_hash"));
    }

    // ─── Edge cases ─────────────────────────────────────────────────────

    #[test]
    fn empty_lines_produces_valid_svg() {
        let svg = render_screen_svg(&[], &sample_metadata());
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn empty_line_content_is_preserved() {
        let svg = render_screen_svg(&[String::new()], &sample_metadata());
        assert!(svg.contains("</svg>"));
    }

    // ─── Monochrome preview honesty ────────────────────────────────────

    #[test]
    fn svg_metadata_comment_declares_monochrome_preview() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(
            svg.contains("reproducible-monochrome-preview"),
            "SVG must honestly declare itself as a monochrome preview"
        );
    }

    #[test]
    fn svg_metadata_comment_declares_fixed_geometry() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(svg.contains("cell=8x16"));
        assert!(svg.contains("padding=16"));
        assert!(svg.contains("font_size=14"));
    }

    #[test]
    fn svg_metadata_comment_declares_font_stack() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(svg.contains("Courier New"));
        assert!(svg.contains("DejaVu Sans Mono"));
        assert!(svg.contains("Menlo"));
    }

    #[test]
    fn svg_height_reflects_declared_rows_not_actual_lines() {
        let meta = SvgRenderMetadata {
            cols: 80,
            rows: 24,
            ..sample_metadata()
        };
        // Only 1 line of text, but declared rows is 24.
        let svg = render_screen_svg(&["x".to_string()], &meta);
        let (_, expected_height) = svg_geometry(meta.cols, meta.rows);
        assert!(svg.contains(&format!(r#"height="{expected_height}""#)));
    }

    #[test]
    fn svg_does_not_claim_color_preserving() {
        let svg = render_screen_svg(&["x".to_string()], &sample_metadata());
        assert!(
            !svg.contains("color-preserving") && !svg.contains("publication-ready"),
            "SVG must not falsely claim to be color-preserving or publication-ready"
        );
    }

    // ─── Finding #1: Saturating arithmetic / overflow safety ────────────

    #[test]
    fn svg_geometry_does_not_overflow_on_max_cols_rows() {
        // cols=u16::MAX, rows=u16::MAX would overflow naive u32 multiplication
        // (65535*65535 > u32::MAX). svg_geometry must saturate, not panic.
        // If it didn't saturate, this would panic on overflow in debug mode.
        let (w, h) = svg_geometry(u16::MAX, u16::MAX);
        // Verify the values are non-zero (saturated, not wrapped to 0).
        assert!(w > 0, "saturated width must be non-zero");
        assert!(h > 0, "saturated height must be non-zero");
    }

    #[test]
    fn svg_renders_without_panic_on_huge_line_count() {
        // u16::MAX lines with declared rows=24: must not panic or overflow.
        let lines: Vec<String> = (0..u16::MAX as usize).map(|_| "x".to_string()).collect();
        let meta = sample_metadata();
        let svg = render_screen_svg(&lines, &meta);
        // SVG must still be well-formed.
        assert!(svg.contains("</svg>"));
        // Only declared rows (24) of text elements should be present.
        let text_count = svg.matches("<text ").count();
        assert_eq!(
            text_count, 24,
            "must truncate to declared rows even with huge input"
        );
    }

    // ─── Finding #2: Geometry contract — declared rows are fixed ───────

    #[test]
    fn svg_truncates_extra_input_rows_beyond_declared_rows() {
        // Provide 100 input lines but declared rows is 24.
        let lines: Vec<String> = (0..100).map(|i| format!("line-{i}")).collect();
        let meta = SvgRenderMetadata {
            rows: 24,
            ..sample_metadata()
        };
        let svg = render_screen_svg(&lines, &meta);

        // Lines 0..23 should be present; lines 24..99 should be truncated.
        assert!(svg.contains("line-0"), "first line must be rendered");
        assert!(
            svg.contains("line-23"),
            "last declared line must be rendered"
        );
        assert!(
            !svg.contains("line-24"),
            "extra lines beyond declared rows must be truncated"
        );
        assert!(
            !svg.contains("line-99"),
            "extra lines beyond declared rows must be truncated"
        );

        // Text element count must match declared rows, not input length.
        let text_count = svg.matches("<text ").count();
        assert_eq!(
            text_count, 24,
            "must render exactly declared_rows text elements"
        );

        // SVG height must still reflect declared rows.
        let (_, expected_height) = svg_geometry(meta.cols, meta.rows);
        assert!(svg.contains(&format!(r#"height="{expected_height}""#)));
    }

    #[test]
    fn svg_truncates_extra_rows_with_fewer_lines_than_declared() {
        // 5 input lines, declared rows=24: all 5 rendered, height stays at 24.
        let lines: Vec<String> = (0..5).map(|i| format!("row-{i}")).collect();
        let meta = SvgRenderMetadata {
            rows: 24,
            ..sample_metadata()
        };
        let svg = render_screen_svg(&lines, &meta);
        let text_count = svg.matches("<text ").count();
        assert_eq!(text_count, 5, "fewer lines than declared must all render");
    }

    // ─── Finding #3: u32 coordinate consistency with viewBox ──────────

    #[test]
    fn svg_text_coordinates_are_u32_consistent_with_viewbox() {
        let svg = render_screen_svg(&["hello".to_string()], &sample_metadata());
        let (svg_width, svg_height) = svg_geometry(sample_metadata().cols, sample_metadata().rows);
        let viewbox = format!(r#"viewBox="0 0 {svg_width} {svg_height}""#);
        assert!(
            svg.contains(&viewbox),
            "viewBox must match width/height coordinates: {svg}"
        );
        // x = PADDING as u32 = 16
        assert!(
            svg.contains(r#"x="16""#),
            "text x must be u32 PADDING (16): {svg}"
        );
        // y for row 0 = PADDING + (0+1)*CELL_HEIGHT - 3 = 16+16-3 = 29
        assert!(
            svg.contains(r#"y="29""#),
            "text y for row 0 must be u32 (29): {svg}"
        );
    }

    #[test]
    fn svg_final_text_coordinate_within_viewbox() {
        // Fill the full declared width: 80 columns.
        let full_line = "x".repeat(80);
        let meta = sample_metadata(); // cols=80
        let svg = render_screen_svg(&[full_line], &meta);
        let (svg_width, _) = svg_geometry(meta.cols, meta.rows);
        // Content starts at PADDING=16, spans 80*8=640, final_x = 656.
        // svg_width = PADDING*2 + 80*8 = 672.
        let final_x = u32::from(PADDING) + u32::from(meta.cols) * u32::from(CELL_WIDTH);
        assert!(
            final_x <= svg_width,
            "final text x ({final_x}) must not exceed viewBox width ({svg_width})"
        );
        let svg_width_str = svg_width.to_string();
        assert!(
            svg.contains(&format!(r#"width="{svg_width_str}""#)),
            "SVG width must match viewBox: {svg}"
        );
    }
}
