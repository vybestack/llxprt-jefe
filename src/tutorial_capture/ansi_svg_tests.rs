//! Tests for the ANSI color-preserving SVG renderer.
//!
//! Extracted from `ansi_svg.rs` to keep the implementation file under the
//! source-file-size warning limit.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-005

use super::*;

fn sample_metadata() -> ColorSvgMetadata {
    ColorSvgMetadata {
        cols: 80,
        rows: 24,
        theme: "green-screen".to_string(),
        jefe_version: "0.0.28".to_string(),
        label: "dashboard".to_string(),
        scenario_hash: Some("abc123".to_string()),
    }
}

#[test]
fn color_svg_has_xml_namespace() {
    let svg = render_color_svg(&["hello".to_string()], &sample_metadata());
    assert!(svg.contains(r#"xmlns="http://www.w3.org/2000/svg""#));
}

#[test]
fn color_svg_has_title_with_label() {
    let svg = render_color_svg(&["hello".to_string()], &sample_metadata());
    assert!(svg.contains("<title>dashboard</title>"));
}

#[test]
fn color_svg_renders_plain_text() {
    let svg = render_color_svg(&["plain text".to_string()], &sample_metadata());
    assert!(svg.contains("plain text"));
}

#[test]
fn color_svg_parses_16_color_red() {
    let line = "\x1b[31mRED TEXT\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("RED TEXT"));
    assert!(svg.contains("#ff5555"), "must contain red color hex: {svg}");
}

#[test]
fn color_svg_parses_bold_green() {
    let line = "\x1b[1;32mBOLD GREEN\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BOLD GREEN"));
    assert!(
        svg.contains(r#"font-weight="bold""#),
        "must contain bold attribute: {svg}"
    );
    assert!(
        svg.contains("#00ff00"),
        "must contain bright green color hex (bold uses bright palette)"
    );
}

#[test]
fn color_svg_parses_underline_blue() {
    let line = "\x1b[4;34mUNDERLINE BLUE\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("UNDERLINE BLUE"));
    assert!(
        svg.contains(r#"text-decoration="underline""#),
        "must contain underline attribute: {svg}"
    );
}

#[test]
fn color_svg_parses_256_color() {
    let line = "\x1b[38;5;208m256 ORANGE\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("256 ORANGE"));
    // Color index 208: 208 - 16 = 192. 192 / 36 = 5 (r=255), (192 % 36) / 6 = 2 (g=135), 192 % 6 = 0 (b=0)
    assert!(
        svg.contains("#ff8700"),
        "must contain 256-orange color hex: {svg}"
    );
}

#[test]
fn color_svg_parses_rgb_color() {
    let line = "\x1b[38;2;128;0;255mRGB PURPLE\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("RGB PURPLE"));
    assert!(
        svg.contains("#8000ff"),
        "must contain RGB purple hex: {svg}"
    );
}

#[test]
fn color_svg_parses_background_color() {
    let line = "\x1b[41mBG RED\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BG RED"));
    assert!(svg.contains("#ff5555"), "must contain red bg color: {svg}");
    // Background rects must NOT use opacity overlay.
    assert!(
        !svg.contains(r#"opacity="0.7""#),
        "background must not use opacity overlay: {svg}"
    );
}

// ── Finding #4: SGR 94/104 bright blue opaque color ───────────────────

/// Finding #4: SGR 94 (bright blue foreground) must render with the opaque
/// bright blue color `#5555ff`, not the semi-transparent gray `#aaaaaaaa`.
#[test]
fn color_svg_sgr94_bright_blue_fg_is_opaque() {
    let line = "\x1b[94mBRIGHT BLUE FG\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BRIGHT BLUE FG"));
    assert!(
        svg.contains("#5555ff"),
        "SGR 94 must render opaque bright blue #5555ff, not gray: {svg}"
    );
    assert!(
        !svg.contains("#aaaaaaaa"),
        "SGR 94 must not render the old semi-transparent gray: {svg}"
    );
}

/// Finding #4: SGR 104 (bright blue background) must render with the opaque
/// bright blue color `#5555ff`.
#[test]
fn color_svg_sgr104_bright_blue_bg_is_opaque() {
    let line = "\x1b[104mBRIGHT BLUE BG\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BRIGHT BLUE BG"));
    assert!(
        svg.contains("#5555ff"),
        "SGR 104 must render opaque bright blue background #5555ff: {svg}"
    );
}

/// Finding #4: The bright blue palette entry (index 12) must be opaque.
#[test]
fn palette_16_bright_blue_is_opaque() {
    assert_eq!(
        PALETTE_16[12], "#5555ff",
        "bright blue palette entry must be opaque #5555ff"
    );
    assert_ne!(
        PALETTE_16[12], "#aaaaaaaa",
        "bright blue must not be the old semi-transparent gray"
    );
}

/// Finding #4: SGR 94 combined with bold must still use the opaque bright blue.
#[test]
fn color_svg_sgr94_bold_bright_blue_is_opaque() {
    let line = "\x1b[1;94mBOLD BRIGHT BLUE\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BOLD BRIGHT BLUE"));
    assert!(
        svg.contains("#5555ff"),
        "SGR 1;94 bold bright blue must be opaque #5555ff: {svg}"
    );
}

/// Finding #4: SGR 94 and 104 together (bright blue on bright blue).
#[test]
fn color_svg_sgr94_104_bright_blue_fg_bg() {
    let line = "\x1b[94;104mFG ON BG\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("FG ON BG"));
    // Both fg and bg should be #5555ff.
    let count = svg.matches("#5555ff").count();
    assert!(
        count >= 2,
        "SGR 94;104 must use #5555ff for both fg and bg (found {count} occurrences): {svg}"
    );
}

#[test]
fn color_svg_is_deterministic() {
    let lines = vec!["\x1b[31mred\x1b[0m".to_string(), "plain".to_string()];
    let meta = sample_metadata();
    let svg1 = render_color_svg(&lines, &meta);
    let svg2 = render_color_svg(&lines, &meta);
    assert_eq!(svg1, svg2, "same input must produce same output");
}

#[test]
fn color_svg_xml_escapes_text() {
    let line = "a & b < c > d".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("a &amp; b &lt; c &gt; d"));
}

#[test]
fn color_svg_metadata_comment_declares_color_preserving() {
    let svg = render_color_svg(&["x".to_string()], &sample_metadata());
    assert!(
        svg.contains("color-preserving-svg"),
        "must declare itself as color-preserving: {svg}"
    );
}

#[test]
fn color_svg_desc_includes_geometry_metadata() {
    let svg = render_color_svg(&["x".to_string()], &sample_metadata());
    assert!(svg.contains("cols=80"));
    assert!(svg.contains("rows=24"));
    assert!(svg.contains("theme=green-screen"));
    assert!(svg.contains("scenario_hash=abc123"));
}

#[test]
fn color_svg_handles_reset_after_color() {
    let line = "\x1b[31mred\x1b[0m normal".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("red"));
    assert!(svg.contains("normal"));
}

#[test]
fn color_svg_handles_empty_line() {
    let svg = render_color_svg(&[String::new()], &sample_metadata());
    assert!(svg.contains("</svg>"));
}

#[test]
fn color_svg_handles_multiple_colors_on_one_line() {
    let line = "\x1b[31mR\x1b[32mG\x1b[34mB\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains(">R<") || svg.contains("R<"));
    assert!(svg.contains("#ff5555"), "red must be present");
    assert!(svg.contains("#6a9955"), "green must be present");
    assert!(svg.contains("#89b4fa"), "blue must be present");
}

#[test]
fn color_svg_height_reflects_declared_rows() {
    let meta = sample_metadata();
    let svg = render_color_svg(&["x".to_string()], &meta);
    let (_, expected_height) = color_svg_geometry(meta.cols, meta.rows);
    assert!(svg.contains(&format!(r#"height="{expected_height}""#)));
}

#[test]
fn color_svg_width_reflects_cols() {
    let meta = ColorSvgMetadata {
        cols: 100,
        ..sample_metadata()
    };
    let svg = render_color_svg(&["x".to_string()], &meta);
    let (expected_width, _) = color_svg_geometry(meta.cols, meta.rows);
    assert!(svg.contains(&format!(r#"width="{expected_width}""#)));
}

#[test]
fn palette_256_has_256_entries() {
    let p = palette_256();
    assert_eq!(p.len(), 256);
}

#[test]
fn palette_256_first_16_match_palette_16() {
    let p = palette_256();
    for (i, expected) in PALETTE_16.iter().enumerate() {
        assert_eq!(&p[i], expected, "palette mismatch at index {i}");
    }
}

#[test]
fn color_svg_uses_monospace_font() {
    let svg = render_color_svg(&["hi".to_string()], &sample_metadata());
    assert!(svg.contains(FONT_FAMILY));
}

#[test]
fn color_svg_preserves_whitespace() {
    let line = "  indented".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("indented"));
    // xml:space=preserve must be present so leading spaces render.
    assert!(
        svg.contains(r#"xml:space="preserve""#),
        "text must use xml:space=preserve: {svg}"
    );
}

#[test]
fn color_svg_clamps_rgb_components() {
    // RGB values out of range (300, -1, 500) must be clamped to 0-255.
    let line = "\x1b[38;2;300;-1;500mRGB CLAMP\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(
        svg.contains("#ff00ff"),
        "must clamp 300->255, -1->0, 500->255 to #ff00ff: {svg}"
    );
}

#[test]
fn color_svg_background_emitted_before_text() {
    let line = "\x1b[41mBG RED\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    // The rect for background must appear before the text element in document order.
    let rect_pos = svg.find("<rect");
    let text_pos = svg.find("<text");
    assert!(rect_pos.is_some(), "must have a rect element: {svg}");
    assert!(text_pos.is_some(), "must have a text element: {svg}");
    assert!(
        rect_pos < text_pos,
        "background rect must appear before text in document order"
    );
}

#[test]
fn color_svg_handles_wide_cjk_characters() {
    // CJK characters take 2 cells in terminal width.
    let line = "日本語".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("日本語"));
}

// ── Finding #6: Green Screen theme derivation ─────────────────────────

/// The SVG background must use the Green Screen theme background (#000000),
/// not a hardcoded dark-theme color.
#[test]
fn color_svg_uses_green_screen_background() {
    let svg = render_color_svg(&["text".to_string()], &sample_metadata());
    assert!(
        svg.contains("#000000"),
        "SVG background must use Green Screen black: {svg}"
    );
}

/// The SVG default foreground must use the Green Screen theme foreground
/// (#6a9955), not a hardcoded dark-theme color.
#[test]
fn color_svg_uses_green_screen_foreground() {
    let svg = render_color_svg(&["plain text".to_string()], &sample_metadata());
    assert!(
        svg.contains("#6a9955"),
        "SVG foreground must use Green Screen green: {svg}"
    );
}

/// SGR reset (code 0) restores the Green Screen default colors.
#[test]
fn color_svg_sgr_reset_restores_green_screen_defaults() {
    let line = "\x1b[31mRED\x1b[0m normal".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    // After reset, "normal" text uses the default foreground (#6a9955).
    assert!(
        svg.contains("#6a9955"),
        "reset text must use Green Screen default fg: {svg}"
    );
}

// ── Finding #1: Saturating arithmetic / overflow safety ─────────────

/// The geometry helper must saturate instead of overflowing at max cols/rows.
#[test]
fn color_svg_geometry_does_not_overflow_on_max_cols_rows() {
    // If it didn't saturate, this would panic on overflow in debug mode.
    let (w, h) = color_svg_geometry(u16::MAX, u16::MAX);
    assert!(w > 0, "saturated width must be non-zero");
    assert!(h > 0, "saturated height must be non-zero");
}

/// Rendering a huge line count must not panic or overflow.
#[test]
fn color_svg_renders_without_panic_on_huge_line_count() {
    let lines: Vec<String> = (0..u16::MAX as usize).map(|_| "x".to_string()).collect();
    let meta = sample_metadata();
    let svg = render_color_svg(&lines, &meta);
    assert!(svg.contains("</svg>"));
    // Only declared rows (24) of text elements should be present.
    let text_count = svg.matches("<text ").count();
    assert_eq!(
        text_count, 24,
        "must truncate to declared rows even with huge input"
    );
}

// ── Finding #2: Geometry contract — declared rows are fixed ─────────

/// Extra input lines beyond declared rows must be truncated.
#[test]
fn color_svg_truncates_extra_input_rows_beyond_declared_rows() {
    let lines: Vec<String> = (0..100)
        .map(|i| format!("\x1b[31mline-{i}\x1b[0m"))
        .collect();
    let meta = ColorSvgMetadata {
        rows: 24,
        ..sample_metadata()
    };
    let svg = render_color_svg(&lines, &meta);

    assert!(svg.contains("line-0"));
    assert!(svg.contains("line-23"));
    assert!(
        !svg.contains("line-24"),
        "extra lines beyond declared rows must be truncated"
    );

    let text_count = svg.matches("<text ").count();
    assert_eq!(
        text_count, 24,
        "must render exactly declared_rows text elements"
    );
}

// ── Finding #3: All SVG coordinates u32 consistent with viewBox ──────────

/// Finding #3: The text element x and y coordinates must be u32 to match the
/// viewBox width/height. All coordinates in the SVG must be the same type
/// so they are consistent with the declared geometry.
#[test]
fn color_svg_coordinates_are_u32_consistent_with_viewbox() {
    let lines = vec!["hello".to_string()];
    let meta = sample_metadata();
    let svg = render_color_svg(&lines, &meta);
    let (svg_width, svg_height) = color_svg_geometry(meta.cols, meta.rows);

    // The viewBox must use the same u32 values as width/height.
    let viewbox = format!(r#"viewBox="0 0 {svg_width} {svg_height}""#);
    assert!(
        svg.contains(&viewbox),
        "viewBox must match width/height coordinates: {svg}"
    );

    // Text element x must be the u32 PADDING value (16).
    assert!(
        svg.contains(r#"x="16""#),
        "text x coordinate must be u32 PADDING (16): {svg}"
    );
    // Text element y for row 0 must be: PADDING + (0+1)*CELL_HEIGHT - 3 = 16+16-3 = 29.
    assert!(
        svg.contains(r#"y="29""#),
        "text y coordinate for row 0 must be u32 (29): {svg}"
    );
}

/// Finding #3: The final text element's rightmost x-coordinate (x + content
/// width) must not exceed the SVG viewBox width.
#[test]
fn color_svg_final_text_coordinate_within_viewbox() {
    // A line filling the full declared width: 80 columns of 'x'.
    let full_line = "x".repeat(80);
    let lines = vec![full_line];
    let meta = sample_metadata(); // cols=80
    let svg = render_color_svg(&lines, &meta);
    let (svg_width, _) = color_svg_geometry(meta.cols, meta.rows);

    // Content starts at x=PADDING (16) and spans 80 cells * CELL_WIDTH (8) = 640.
    // Final x = PADDING + 640 = 656, which must be <= svg_width = PADDING*2 + 80*8 = 672.
    let final_x = u32::from(PADDING) + u32::from(meta.cols) * u32::from(CELL_WIDTH);
    assert!(
        final_x <= svg_width,
        "final text x ({final_x}) must not exceed viewBox width ({svg_width})"
    );
    // The last text element's x must be within the viewBox.
    let svg_width_str = svg_width.to_string();
    assert!(
        svg.contains(&format!(r#"width="{svg_width_str}""#)),
        "SVG width must match viewBox: {svg}"
    );
}

/// Finding #3: Background rect coordinates must also be u32 consistent.
#[test]
fn color_svg_background_rect_coordinates_are_u32() {
    let line = "\x1b[41mBG RED\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    // The rect x must be the u32 PADDING value (16).
    assert!(
        svg.contains(r#"<rect x="16""#),
        "background rect x must be u32 PADDING (16): {svg}"
    );
}

/// Finding #3: For the monochrome SVG, coordinates must also be u32.
#[test]
fn monochrome_svg_coordinates_are_u32_consistent_with_viewbox() {
    use crate::tutorial_capture::svg_render::{SvgRenderMetadata, render_screen_svg, svg_geometry};
    let meta = SvgRenderMetadata {
        cols: 80,
        rows: 24,
        theme: "dark".to_string(),
        jefe_version: "0.0.28".to_string(),
        label: "test".to_string(),
        scenario_hash: None,
    };
    let svg = render_screen_svg(&["hello".to_string()], &meta);
    let (svg_width, svg_height) = svg_geometry(meta.cols, meta.rows);

    let viewbox = format!(r#"viewBox="0 0 {svg_width} {svg_height}""#);
    assert!(
        svg.contains(&viewbox),
        "monochrome viewBox must match width/height: {svg}"
    );
    assert!(
        svg.contains(r#"x="16""#),
        "monochrome text x must be u32 PADDING (16): {svg}"
    );
    assert!(
        svg.contains(r#"y="29""#),
        "monochrome text y for row 0 must be u32 (29): {svg}"
    );
}
