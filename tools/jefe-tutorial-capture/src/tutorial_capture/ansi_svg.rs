//! Color-preserving ANSI-to-SVG adapter.
//!
//! This module parses ANSI escape sequences from `tmux capture-pane -e` output
//! and renders them as a color SVG that preserves 16-color, 256-color, and
//! RGB foreground/background colors, plus bold and underline attributes.
//!
//! ## Boundary
//!
//! This module is pure: it transforms ANSI-escaped text + metadata into an
//! SVG string. It does not read or write files or call tmux.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-008

use std::fmt::Write;

/// Metadata embedded in the color SVG for reproducibility.
#[derive(Debug, Clone)]
pub struct ColorSvgMetadata {
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
const CELL_WIDTH: u16 = 8;
const CELL_HEIGHT: u16 = 16;
const PADDING: u16 = 16;
const FONT_SIZE: u16 = 14;

/// Fixed font stack for reproducibility.
const FONT_FAMILY: &str = "monospace, 'Courier New', 'DejaVu Sans Mono', 'Menlo', monospace";

/// Default background derived from the Green Screen theme (#000000).
///
/// **Finding #6**: The ANSI SVG default/reset palette is derived from the
/// actual Green Screen theme colors (black background, green foreground)
/// rather than hardcoded dark-theme colors.
const DEFAULT_BG: &str = "#000000";
/// Default foreground derived from the Green Screen theme (#6a9955).
const DEFAULT_FG: &str = "#6a9955";

// ── ANSI color tables ─────────────────────────────────────────────────

/// Standard 16-color palette (indices 0-15).
///
/// **Finding #6**: Derived from the Green Screen theme. The primary colors
/// are black/green per the Green Screen theme, with standard ANSI positions
/// for other colors so SGR sequences render reasonably.
/// [0-7: normal, 8-15: bright]
const PALETTE_16: [&str; 16] = [
    "#000000", // 0 black (Green Screen bg)
    "#ff5555", // 1 red
    "#6a9955", // 2 green (Green Screen fg)
    "#f9e2af", // 3 yellow
    "#89b4fa", // 4 blue
    "#f5c2e7", // 5 magenta
    "#94e2d5", // 6 cyan
    "#6a9955", // 7 white (mapped to green fg for Green Screen)
    "#3a3a3a", // 8 bright black
    "#ff8080", // 9 bright red
    "#00ff00", // 10 bright green (Green Screen bright)
    "#ffff00", // 11 bright yellow
    "#5555ff", // 12 bright blue (opaque, fully saturated)
    "#ffaaff", // 13 bright magenta
    "#aaffff", // 14 bright cyan
    "#00ff00", // 15 bright white (mapped to bright green for Green Screen)
];

/// Generate the 256-color palette as hex strings.
fn palette_256() -> Vec<String> {
    let mut palette: Vec<String> = Vec::with_capacity(256);
    // First 16: same as PALETTE_16.
    palette.extend(PALETTE_16.iter().map(|s| (*s).to_string()));
    // 216 colors (16-231): 6x6x6 RGB cube.
    let levels = [0u8, 95, 135, 175, 215, 255];
    for r in &levels {
        for g in &levels {
            for b in &levels {
                palette.push(format!("#{r:02x}{g:02x}{b:02x}"));
            }
        }
    }
    // 24 grayscale colors (232-255).
    for i in 0..24u8 {
        let v = 8 + i * 10;
        palette.push(format!("#{v:02x}{v:02x}{v:02x}"));
    }
    palette
}

/// Compute the color SVG geometry (width, height) from declared cols/rows
/// using saturating arithmetic so no multiplication or addition can overflow.
///
/// Exposed so tests can verify geometry without duplicating magic constants
/// (Finding #6).
#[must_use]
pub fn color_svg_geometry(cols: u16, rows: u16) -> (u32, u32) {
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
fn color_text_y_for_row(row: u16) -> u32 {
    let padding = u32::from(PADDING);
    let cell_h = u32::from(CELL_HEIGHT);
    let row_plus1 = u32::from(row).saturating_add(1);
    padding
        .saturating_add(row_plus1.saturating_mul(cell_h))
        .saturating_sub(3)
}

/// Render a screen capture with ANSI escape sequences as a color-preserving SVG.
///
/// Parses SGR (Select Graphic Rendition) escape sequences to extract
/// foreground/background colors and text attributes (bold, underline),
/// then renders each text segment as a colored SVG `<text>` element.
///
/// Background rectangles are emitted **before** text elements in document
/// order so they appear underneath text without opacity overlay artifacts.
/// Text elements use `xml:space="preserve"` so leading/trailing spaces are
/// faithfully rendered.
///
/// Extra input lines beyond the declared row count are truncated (not
/// rendered) so the SVG content matches the declared geometry contract.
///
/// @requirement REQ-TUTORIAL-CAPTURE-008
#[must_use]
pub fn render_color_svg(lines: &[String], metadata: &ColorSvgMetadata) -> String {
    let palette256 = palette_256();
    let declared_rows = metadata.rows.max(1);
    let (svg_width, svg_height) = color_svg_geometry(metadata.cols, declared_rows);

    let mut svg = String::new();
    write_color_header(&mut svg, svg_width, svg_height, metadata);
    write_color_background(&mut svg, svg_width, svg_height);
    // Emit background rects and text elements for each line.
    // Extra lines beyond declared_rows are truncated.
    let max_row_idx = declared_rows as usize;
    for (i, line) in lines.iter().enumerate().take(max_row_idx) {
        let row = u16::try_from(i).unwrap_or(u16::MAX);
        let y = color_text_y_for_row(row);
        let segments = parse_ansi_segments(line, &palette256);
        // Pass 1: emit background rectangles for segments with custom bg.
        emit_background_rects(&mut svg, &segments, y);
        // Pass 2: emit text elements.
        emit_text_elements(&mut svg, &segments, y);
    }
    write_color_metadata_comment(&mut svg, metadata);
    svg.push_str("</svg>\n");
    svg
}

/// Write the SVG header with embedded metadata.
fn write_color_header(svg: &mut String, width: u32, height: u32, metadata: &ColorSvgMetadata) {
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    let _ = writeln!(svg, "  <title>{}</title>", escape_xml_text(&metadata.label));
    let _ = writeln!(
        svg,
        "  <desc>jefe-tutorial-capture color SVG: label={}, cols={}, rows={}, theme={}, jefe_version={}{}</desc>",
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
fn write_color_background(svg: &mut String, width: u32, height: u32) {
    let _ = writeln!(
        svg,
        r#"  <rect width="{width}" height="{height}" fill="{DEFAULT_BG}"/>"#
    );
}

/// Compute the terminal display width of text using `unicode-width`.
/// Wide CJK characters count as 2 cells; zero-width characters (combining
/// marks) count as 0. This ensures SVG cell positioning matches actual
/// terminal rendering.
fn terminal_display_width(text: &str) -> u32 {
    use unicode_width::UnicodeWidthChar;
    let mut width: u32 = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        width = width.saturating_add(u32::try_from(w).unwrap_or(u32::MAX));
    }
    width
}

/// Emit background rectangles for segments with custom background colors.
///
/// Backgrounds are emitted before text elements so they render underneath
/// without opacity overlay artifacts.
fn emit_background_rects(svg: &mut String, segments: &[AnsiSegment], y: u32) {
    let mut current_x = u32::from(PADDING);
    for seg in segments {
        if seg.text.is_empty() {
            continue;
        }
        let display_width = terminal_display_width(&seg.text);
        if display_width == 0 {
            continue;
        }
        let has_bg = seg.bg_color != DEFAULT_BG;
        if has_bg {
            let bg_width = display_width.saturating_mul(u32::from(CELL_WIDTH));
            let bg_y = y.saturating_sub(u32::from(CELL_HEIGHT).saturating_sub(3));
            let _ = writeln!(
                svg,
                r#"  <rect x="{current_x}" y="{bg_y}" width="{bg_width}" height="{}" fill="{}"/>"#,
                CELL_HEIGHT, seg.bg_color
            );
        }
        current_x = current_x.saturating_add(display_width.saturating_mul(u32::from(CELL_WIDTH)));
    }
}

/// Emit text elements for each segment with appropriate colors and attributes.
///
/// Text elements use `xml:space="preserve"` so leading/trailing spaces are
/// faithfully rendered in the SVG.
fn emit_text_elements(svg: &mut String, segments: &[AnsiSegment], y: u32) {
    let mut current_x = u32::from(PADDING);
    for seg in segments {
        let text = escape_xml_text(&seg.text);
        if text.is_empty() {
            continue;
        }
        let display_width = terminal_display_width(&seg.text);
        let _ = write!(
            svg,
            r#"  <text x="{current_x}" y="{y}" xml:space="preserve" font-family="{FONT_FAMILY}" font-size="{FONT_SIZE}" fill="{}""#,
            seg.fg_color
        );
        if seg.bold {
            svg.push_str(r#" font-weight="bold""#);
        }
        if seg.underline {
            svg.push_str(r#" text-decoration="underline""#);
        }
        let _ = writeln!(svg, r"><tspan>{text}</tspan></text>");
        current_x = current_x.saturating_add(display_width.saturating_mul(u32::from(CELL_WIDTH)));
    }
}

/// A text segment with resolved color and attributes.
#[derive(Debug, Clone)]
struct AnsiSegment {
    text: String,
    fg_color: String,
    bg_color: String,
    bold: bool,
    underline: bool,
}

impl AnsiSegment {
    fn new_default() -> Self {
        Self {
            text: String::new(),
            fg_color: DEFAULT_FG.to_string(),
            bg_color: DEFAULT_BG.to_string(),
            bold: false,
            underline: false,
        }
    }

    fn reset(&mut self) {
        self.fg_color = DEFAULT_FG.to_string();
        self.bg_color = DEFAULT_BG.to_string();
        self.bold = false;
        self.underline = false;
    }
}

/// Parse ANSI escape sequences from a line and produce colored segments.
///
/// Handles:
/// - **CSI sequences** (`ESC [`): SGR (`m`) parameters are applied; other CSI
///   final bytes (e.g. cursor movement `H`, `J`) are consumed and discarded
///   so they do not pollute the visible text.
/// - **OSC sequences** (`ESC ]`): consumed until `BEL` (0x07) or `ESC `
///   (string terminator).
/// - **Standalone escape sequences** (`ESC` followed by a single non-`[`/
///   non-`]` byte): the `ESC` and the following byte are consumed.
/// - **Malformed sequences** (truncated `ESC [` or `ESC ]`): consumed to end
///   of input so partial bytes are not rendered as visible text.
fn parse_ansi_segments(line: &str, palette256: &[String]) -> Vec<AnsiSegment> {
    let mut segments: Vec<AnsiSegment> = Vec::new();
    let mut current = AnsiSegment::new_default();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Escape sequence detected. Flush pending text first.
            if !current.text.is_empty() {
                segments.push(current.clone());
                current.text.clear();
            }
            if i + 1 >= bytes.len() {
                // Truncated: ESC at end of input — consume it.
                break;
            }
            match bytes[i + 1] {
                b'[' => {
                    // CSI sequence: delegate to the dedicated consumer that
                    // collects parameter bytes and applies SGR when the final
                    // byte is `m`. Non-SGR CSI sequences (cursor movement,
                    // erase, etc.) are consumed and discarded.
                    i = consume_csi_after_esc_bracket(bytes, i + 2, &mut current, palette256);
                }
                b']' => {
                    // OSC sequence: consume until BEL (0x07) or ST (ESC \).
                    i += 2;
                    i = consume_osc(bytes, i);
                }
                _ => {
                    // Standalone escape: ESC + one byte. Consume both.
                    i += 2;
                }
            }
        } else {
            // Regular character.
            let ch = bytes[i];
            if ch < 0x80 {
                current.text.push(ch as char);
                i += 1;
            } else {
                // UTF-8 multi-byte: collect the full character.
                let len = utf8_len(ch);
                let avail = len.min(bytes.len() - i);
                if let Ok(s) = std::str::from_utf8(&bytes[i..i + avail]) {
                    current.text.push_str(s);
                }
                i += avail;
            }
        }
    }
    if !current.text.is_empty() {
        segments.push(current);
    }
    segments
}

/// Consume a CSI parameter string starting at position `i` (right after
/// `ESC [` was consumed). Applies SGR if the final byte is `m`. Returns the
/// new position.
fn consume_csi_after_esc_bracket(
    bytes: &[u8],
    start: usize,
    current: &mut AnsiSegment,
    palette256: &[String],
) -> usize {
    let mut param_str = String::new();
    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        if b == 0x1b {
            // Unexpected ESC inside CSI — back up.
            i -= 1;
            break;
        }
        if (0x40..=0x7e).contains(&b) {
            // Final byte.
            if b == b'm' {
                apply_sgr_params(&param_str, current, palette256);
            }
            // Non-SGR CSI sequences (cursor movement, erase, etc.) are
            // consumed and discarded — they do not affect color or text.
            break;
        }
        // Parameter or intermediate byte: collect for potential SGR parse.
        // The minus sign is included so negative RGB components (e.g.
        // `38;2;300;-1;500`) are preserved for clamping rather than
        // silently dropping the sign and treating -1 as 1.
        if b.is_ascii_digit() || b == b';' || b == b':' || b == b'?' || b == b'-' {
            param_str.push(b as char);
        }
    }
    i
}

/// Consume an OSC sequence (started after `ESC ]` was consumed). Returns the
/// position after the terminator (BEL or ST).
fn consume_osc(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == 0x07 {
            // BEL terminator.
            return i + 1;
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
            // String terminator: ESC followed by a backslash.
            return i + 2;
        }
        i += 1;
    }
    // Truncated OSC — consumed to end of input.
    i
}

/// Determine the expected UTF-8 byte length from the leading byte.
fn utf8_len(first_byte: u8) -> usize {
    if first_byte < 0xC0 {
        1
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

/// Apply SGR (Select Graphic Rendition) parameters to the current state.
fn apply_sgr_params(param_str: &str, current: &mut AnsiSegment, palette256: &[String]) {
    let params: Vec<i32> = param_str
        .split(';')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect();
    if params.is_empty() {
        current.reset();
        return;
    }
    let mut idx = 0;
    while idx < params.len() {
        let code = params[idx];
        apply_single_sgr(code, &params, &mut idx, current, palette256);
        idx += 1;
    }
}

/// Apply a single SGR code, consuming extended-color parameters.
fn apply_single_sgr(
    code: i32,
    params: &[i32],
    idx: &mut usize,
    current: &mut AnsiSegment,
    palette256: &[String],
) {
    match code {
        0 => current.reset(),
        1 => current.bold = true,
        4 => current.underline = true,
        22 => current.bold = false,
        24 => current.underline = false,
        30..=37 => {
            let color_idx = usize::try_from(code - 30).unwrap_or(0);
            if current.bold && color_idx < 8 {
                current.fg_color.clone_from(&palette256[color_idx + 8]);
            } else if color_idx < palette256.len() {
                current.fg_color.clone_from(&palette256[color_idx]);
            }
        }
        38 => apply_extended_fg(params, idx, current, palette256),
        39 => current.fg_color = DEFAULT_FG.to_string(),
        40..=47 => {
            let color_idx = usize::try_from(code - 40).unwrap_or(0);
            if color_idx < palette256.len() {
                current.bg_color.clone_from(&palette256[color_idx]);
            }
        }
        48 => apply_extended_bg(params, idx, current, palette256),
        49 => current.bg_color = DEFAULT_BG.to_string(),
        90..=97 => {
            let color_idx = usize::try_from(code - 90 + 8).unwrap_or(0);
            if color_idx < palette256.len() {
                current.fg_color.clone_from(&palette256[color_idx]);
            }
        }
        100..=107 => {
            let color_idx = usize::try_from(code - 100 + 8).unwrap_or(0);
            if color_idx < palette256.len() {
                current.bg_color.clone_from(&palette256[color_idx]);
            }
        }
        _ => {}
    }
}

/// Apply extended foreground color (256-color or RGB).
///
/// RGB components are clamped to the valid 0-255 range to ensure the
/// emitted hex color is always well-formed.
fn apply_extended_fg(
    params: &[i32],
    idx: &mut usize,
    current: &mut AnsiSegment,
    palette256: &[String],
) {
    if *idx + 1 >= params.len() {
        return;
    }
    match params[*idx + 1] {
        5 => {
            if *idx + 2 < params.len() {
                let n = usize::try_from(params[*idx + 2]).unwrap_or(0);
                if n < 256 {
                    current.fg_color.clone_from(&palette256[n]);
                }
                *idx += 2;
            } else {
                // Truncated `38;5` (missing color index): skip the sub-mode
                // selector so it is not reprocessed as a standalone SGR code.
                *idx += 1;
            }
        }
        2 => {
            if *idx + 4 < params.len() {
                let r = clamp_rgb(params[*idx + 2]);
                let g = clamp_rgb(params[*idx + 3]);
                let b = clamp_rgb(params[*idx + 4]);
                current.fg_color = format!("#{r:02x}{g:02x}{b:02x}");
                *idx += 4;
            } else {
                // Consume the entire malformed extended-color command so its
                // partial RGB values cannot be reinterpreted as SGR codes.
                *idx = params.len().saturating_sub(1);
            }
        }
        _ => {
            *idx += 1;
        }
    }
}

/// Apply extended background color (256-color or RGB).
///
/// RGB components are clamped to the valid 0-255 range.
fn apply_extended_bg(
    params: &[i32],
    idx: &mut usize,
    current: &mut AnsiSegment,
    palette256: &[String],
) {
    if *idx + 1 >= params.len() {
        return;
    }
    match params[*idx + 1] {
        5 => {
            if *idx + 2 < params.len() {
                let n = usize::try_from(params[*idx + 2]).unwrap_or(0);
                if n < 256 {
                    current.bg_color.clone_from(&palette256[n]);
                }
                *idx += 2;
            } else {
                *idx += 1;
            }
        }
        2 => {
            if *idx + 4 < params.len() {
                let r = clamp_rgb(params[*idx + 2]);
                let g = clamp_rgb(params[*idx + 3]);
                let b = clamp_rgb(params[*idx + 4]);
                current.bg_color = format!("#{r:02x}{g:02x}{b:02x}");
                *idx += 4;
            } else {
                *idx = params.len().saturating_sub(1);
            }
        }
        _ => {
            *idx += 1;
        }
    }
}

/// Clamp an SGR RGB component to the valid 0-255 range.
///
/// Uses `try_from` after clamping so the conversion is provably safe without
/// any clippy suppression: `clamp(0, 255)` guarantees the value is in the
/// valid `u8` range before the conversion.
fn clamp_rgb(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or(0)
}

/// Write a metadata comment declaring all fixed parameters for reproducibility.
///
/// **Task #5 (XML comment safety)**: Only fixed compile-time constants
/// appear in this XML comment. Variable metadata (theme, jefe version,
/// scenario hash, declared rows/cols) is intentionally omitted from the
/// comment because those values could contain `--` or `-->` sequences that
/// would break XML comment well-formedness. The variable metadata is
/// retained in the `<title>` and `<desc>` elements where it is properly
/// XML-escaped.
fn write_color_metadata_comment(svg: &mut String, _metadata: &ColorSvgMetadata) {
    let _ = writeln!(
        svg,
        "<!-- color-preserving-svg: cell={CELL_WIDTH}x{CELL_HEIGHT} padding={PADDING} font_size={FONT_SIZE} font_family=\"{FONT_FAMILY}\" -->"
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
            c if matches!(c, '\u{0009}' | '\u{000A}' | '\u{000D}') || c >= '\u{0020}' => {
                result.push(c);
            }
            _ => result.push('\u{FFFD}'),
        }
    }
    result
}

#[cfg(test)]
#[path = "ansi_svg_tests.rs"]
mod tests;
