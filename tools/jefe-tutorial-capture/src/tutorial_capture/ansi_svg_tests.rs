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
fn color_svg_renders_without_panic_when_input_exceeds_declared_rows() {
    let lines: Vec<String> = (0..100).map(|_| "x".to_string()).collect();
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
    use crate::svg_render::{SvgRenderMetadata, render_screen_svg, svg_geometry};
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

// ── ANSI edge cases: malformed sequences, non-SGR CSI, OSC ───────────

/// A truncated ESC at the very end of input must not produce a visible
/// escape byte in the SVG text. It should be consumed silently.
#[test]
fn color_svg_handles_truncated_esc_at_end_of_input() {
    let lines = vec!["hello\x1b".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("hello"),
        "visible text before truncated ESC must be rendered: {svg}"
    );
    assert!(
        !svg.contains('\x1b'),
        "truncated ESC must not leak into SVG output: {svg}"
    );
}

/// A truncated CSI sequence (ESC [ with no final byte) must not produce
/// visible escape characters or panic.
#[test]
fn color_svg_handles_truncated_csi_sequence() {
    let lines = vec!["text\x1b[31".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("text"),
        "visible text before truncated CSI must be rendered: {svg}"
    );
    assert!(
        !svg.contains('\x1b'),
        "truncated CSI must not leak ESC into output: {svg}"
    );
    assert!(
        !svg.contains("[31"),
        "truncated CSI parameter bytes must not leak into output: {svg}"
    );
}

/// Non-SGR CSI sequences (cursor movement, erase) must be consumed and
/// discarded without affecting visible text or colors.
#[test]
fn color_svg_consumes_non_sgr_csi_cursor_movement() {
    // ESC [ H = cursor home, ESC [ 2J = erase display
    let lines = vec!["\x1b[H\x1b[2Jhello".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("hello"),
        "text after non-SGR CSI must be rendered: {svg}"
    );
    assert!(
        !svg.contains("[H") && !svg.contains("[2J"),
        "non-SGR CSI sequences must be fully consumed: {svg}"
    );
}

/// OSC sequences (e.g. ESC ] 0;title BEL) must be consumed and not leak
/// into visible text.
#[test]
fn color_svg_consumes_osc_sequence_with_bel_terminator() {
    // ESC ] 0 ; title \x07 (BEL)
    let lines = vec!["\x1b]0;my-window-title\x07visible".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("visible"),
        "text after OSC must be rendered: {svg}"
    );
    assert!(
        !svg.contains("my-window-title"),
        "OSC payload must not leak into visible text: {svg}"
    );
    assert!(
        !svg.contains("]0;"),
        "OSC sequence bytes must be fully consumed: {svg}"
    );
}

/// OSC sequences terminated with ST (ESC backslash) must also be consumed.
#[test]
fn color_svg_consumes_osc_sequence_with_string_terminator() {
    // ESC ] 0 ; title ESC \
    let lines = vec!["\x1b]0;title\x1b\\visible".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("visible"),
        "text after ST-terminated OSC must be rendered: {svg}"
    );
    assert!(
        !svg.contains("]0;title"),
        "ST-terminated OSC payload must not leak: {svg}"
    );
}

/// A standalone escape (ESC + non-bracket byte) must be consumed as a
/// two-byte sequence without affecting text.
#[test]
fn color_svg_consumes_standalone_escape_sequence() {
    // ESC ( B = designate G0 character set (common in many terminals)
    let lines = vec!["\x1b(Bhello".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("hello"),
        "text after standalone ESC must be rendered: {svg}"
    );
    assert!(
        !svg.contains("(B"),
        "standalone escape payload must be consumed: {svg}"
    );
}

/// An unexpected ESC inside a CSI sequence must terminate the current CSI
/// and start a new sequence, not corrupt the output.
#[test]
fn color_svg_handles_unexpected_esc_inside_csi() {
    // ESC [ 3 ESC [ 1 m text — the first CSI is interrupted by a new ESC
    let lines = vec!["\x1b[3\x1b[1mtext".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("text"),
        "text after interrupted CSI must be rendered: {svg}"
    );
    assert!(!svg.contains('\x1b'), "no ESC bytes must leak: {svg}");
}

// ── BLOCKER 6: Malformed/truncated SGR edge cases ───────────────────────

/// BLOCKER 6: A malformed SGR sequence with non-numeric parameter bytes
/// (e.g. `ESC [ aa m`) must not panic. The invalid bytes are consumed and the
/// SGR is treated as having no valid params (reset).
#[test]
fn color_svg_handles_malformed_sgr_non_numeric_params() {
    let lines = vec!["\x1b[aamtext".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("text"),
        "text after malformed SGR must be rendered: {svg}"
    );
    assert!(!svg.contains('\x1b'), "no ESC bytes must leak: {svg}");
}

/// BLOCKER 6: SGR 22 (cancel bold) followed by a base-color SGR (e.g. 32)
/// must use the **base** palette color, not the bright palette. SGR 22 sets
/// `bold = false`, and a subsequent base-color (30-37) must check the current
/// bold state to select the normal palette (indices 0-7), not the bright
/// palette (indices 8-15).
#[test]
fn color_svg_sgr22_then_base_color_uses_base_palette() {
    // ESC [ 1 ; 32 m → bold green (bright, index 10 = #00ff00)
    // ESC [ 22 ; 32 m → cancel bold + green (base, index 2 = #6a9955)
    let line = "\x1b[1;32mBOLD\x1b[22;32mNORMAL\x1b[0m".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("BOLD"));
    assert!(svg.contains("NORMAL"));
    // BOLD segment uses bright green (#00ff00, index 10).
    assert!(
        svg.contains("#00ff00"),
        "bold green must use bright palette #00ff00: {svg}"
    );
    // NORMAL segment uses base green (#6a9955, index 2).
    assert!(
        svg.contains("#6a9955"),
        "SGR 22 then 32 must use base palette #6a9955, not bright: {svg}"
    );
}

/// BLOCKER 6: Truncated `38;5` (missing color index) must not panic and must
/// not apply a wrong color. The foreground stays at whatever it was before
/// the truncated sequence.
#[test]
fn color_svg_handles_truncated_38_5_sequence() {
    // ESC [ 38 ; 5 — truncated (missing color index), then "text"
    let line = "\x1b[38;5text".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(
        svg.contains("text"),
        "text after truncated 38;5 must be rendered: {svg}"
    );
    assert!(!svg.contains('\x1b'), "no ESC bytes must leak: {svg}");
}

/// BLOCKER 6: Truncated `38;2` (missing RGB components) must not panic and
/// must not apply a wrong color.
#[test]
fn color_svg_handles_truncated_38_2_sequence() {
    // ESC [ 38 ; 2 ; 128 — truncated (missing G and B), then "text"
    let line = "\x1b[38;2;128text".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    // Just check it doesn't panic.
    assert!(svg.contains("</svg>"));
}

/// BLOCKER 6: Truncated `48;5` (missing color index) for background must not
/// panic and must not apply a wrong background color.
#[test]
fn color_svg_handles_truncated_48_5_sequence() {
    let line = "\x1b[48;5text".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(
        svg.contains("text"),
        "text after truncated 48;5 must be rendered: {svg}"
    );
    assert!(!svg.contains('\x1b'), "no ESC bytes must leak: {svg}");
}

/// BLOCKER 6: Truncated `48;2` (missing RGB components) for background must
/// not panic.
#[test]
fn color_svg_handles_truncated_48_2_sequence() {
    let line = "\x1b[48;2;64text".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("</svg>"));
    assert!(!svg.contains('\x1b'), "no ESC bytes must leak: {svg}");
}

/// BLOCKER 6: An empty SGR sequence (`ESC [ m`) is equivalent to `ESC [ 0 m`
/// (reset). It must reset colors to defaults.
#[test]
fn color_svg_empty_sgr_resets_to_defaults() {
    let line = "\x1b[31mRED\x1b[m normal".to_string();
    let svg = render_color_svg(&[line], &sample_metadata());
    assert!(svg.contains("RED"));
    assert!(svg.contains("normal"));
    // After the empty SGR reset, "normal" text uses the default foreground.
    assert!(
        svg.contains("#6a9955"),
        "empty SGR must reset fg to default: {svg}"
    );
}

/// BLOCKER 6: A lone `ESC` (not followed by `[` or `]`) at the start of input
/// must be consumed, not rendered as visible text.
#[test]
fn color_svg_lone_esc_at_start_is_consumed() {
    let lines = vec!["\x1bXhello".to_string()];
    let svg = render_color_svg(&lines, &sample_metadata());
    assert!(
        svg.contains("hello"),
        "text after lone ESC must render: {svg}"
    );
    assert!(!svg.contains('\x1b'), "lone ESC must not leak: {svg}");
    assert!(!svg.contains('X'), "ESC payload must be consumed: {svg}");
}

// ─── Task #5: XML comment safety ──────────────────────────────────────

/// Variable metadata (theme, jefe_version, scenario_hash) must NOT appear in
/// the XML comment — only in `<title>`/`<desc>` where it is escaped.
#[test]
fn color_svg_metadata_comment_omits_variable_metadata() {
    let meta = ColorSvgMetadata {
        theme: "unique-theme-marker".to_string(),
        jefe_version: "unique-version-marker".to_string(),
        scenario_hash: Some("unique-hash-marker".to_string()),
        ..sample_metadata()
    };
    let svg = render_color_svg(&["x".to_string()], &meta);
    let comment_start = svg
        .find("<!--")
        .unwrap_or_else(|| panic!("no XML comment found: {svg}"));
    let comment_end = svg[comment_start..].find("-->").map_or_else(
        || panic!("unterminated XML comment: {svg}"),
        |position| comment_start + position + 3,
    );
    let comment = &svg[comment_start..comment_end];
    assert!(
        !comment.contains("unique-theme-marker"),
        "theme must not appear in comment: {comment}"
    );
    assert!(
        !comment.contains("unique-version-marker"),
        "jefe_version must not appear in comment: {comment}"
    );
    assert!(
        !comment.contains("unique-hash-marker"),
        "scenario_hash must not appear in comment: {comment}"
    );
}

/// Metadata containing `--` or `-->` must not break the XML comment. The
/// variable metadata is retained in `<desc>` where it is escaped.
#[test]
fn color_svg_metadata_with_double_hyphen_does_not_break_comment() {
    let meta = ColorSvgMetadata {
        theme: "evil--theme".to_string(),
        jefe_version: "0.0.28--bad".to_string(),
        scenario_hash: Some("hash-->injection".to_string()),
        ..sample_metadata()
    };
    let svg = render_color_svg(&["x".to_string()], &meta);
    let first_open = svg
        .find("<!--")
        .unwrap_or_else(|| panic!("no comment open: {svg}"));
    let first_close = svg
        .find("-->")
        .unwrap_or_else(|| panic!("no comment close: {svg}"));
    assert!(
        first_close > first_open,
        "comment close must come after open: {svg}"
    );
    let comment_body = &svg[first_open + 4..first_close];
    assert!(
        !comment_body.contains("-->"),
        "metadata with --> must not prematurely close comment: {comment_body}"
    );
    assert!(
        !comment_body.contains("evil--theme"),
        "variable theme must not be in comment: {comment_body}"
    );
    assert!(
        svg.contains("evil--theme"),
        "theme with -- must be retained in desc/title (escaped): {svg}"
    );
}
