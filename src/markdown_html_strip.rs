//! Single-pass HTML-to-text stripping for untrusted markdown content
//! (issue #155).
//!
//! Split out of `markdown_render` (the AST walker) to keep each file under
//! the source-size limit: this module owns the byte-level state machine that
//! guarantees no raw angle-brackets, entities, or control characters survive
//! to the screen, while `markdown_render` owns the comrak AST traversal.
//! The two are one logical pipeline; `markdown_render_tests.rs` covers both
//! through the public `render_markdown_lines` entry point.

/// Strip a raw HTML fragment down to its visible text.
///
/// This is a single-pass, O(n) state machine (issue #155 review: the previous
/// per-`<`/`&` suffix scan was quadratic on malformed/untrusted input). It
/// handles:
///
/// - quoted attributes (`<a title="1 > 0">` does not end the tag at the inner
///   `>`),
/// - HTML comments (`<!-- … -->`) and declarations (`<!…>`) — dropped entirely,
/// - block-level closing tags / `<br>` → `\n` so paragraph/list/row boundaries
///   survive,
/// - a bounded set of named/numeric entities (`&amp;`, `&lt;`, …); anything
///   longer than `MAX_ENTITY_LEN` without a `;` is left literal so an unmatched
///   `&` never triggers a full-suffix scan.
///
/// The output may contain `\n` (one per block boundary); callers MUST split on
/// `\n` before emitting screen lines (see [`MarkdownRenderer::render_html_block`]
/// and [`InlineLines::push_str`], which both split on `\n`).
#[must_use]
pub fn strip_html_to_text(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => i = consume_tag(html, bytes, i, &mut out),
            b'&' => {
                let (next, decoded) = consume_entity(html, i);
                out.push_str(&decoded);
                i = next;
            }
            _ => {
                // Decode the char at this position so the control-character
                // filter covers multi-byte C1 controls (U+0080–U+009F), not
                // just ASCII control bytes. Newlines (block boundaries) and
                // tabs are meaningful and must survive; all other control
                // chars are dropped (defense-in-depth: the module's own
                // contract promises no control characters survive).
                let ch_len = utf8_len(bytes[i]);
                if let Some(ch) = html[i..].chars().next()
                    && (ch == '\n' || ch == '\t' || !ch.is_control())
                    && let Some(slice) = html.get(i..i + ch_len)
                {
                    out.push_str(slice);
                }
                i += ch_len;
            }
        }
    }
    out
}

/// Maximum entity length scanned after a `&` before falling back to a literal
/// `&` (no entity is longer than ~10 chars; bounding this keeps the scanner
/// single-pass for unmatched `&`).
const MAX_ENTITY_LEN: usize = 12;

/// Consume an HTML tag starting at `bytes[start]` (which is `<`), appending a
/// `\n` for block-boundary tags and returning the index just past the tag's
/// closing `>`. Comments, declarations, and unrecognized tags are consumed
/// entirely with no text emitted; an unterminated tag consumes to end-of-input.
fn consume_tag(html: &str, bytes: &[u8], start: usize, out: &mut String) -> usize {
    // Comment or declaration: drop everything through the matching close.
    // An unterminated comment/declaration consumes to end-of-input (otherwise
    // the main loop would never advance `i` and hang).
    if html[start..].starts_with("<!--") {
        return match html[start..].find("-->") {
            Some(p) => start + p + "-->".len(),
            None => bytes.len(),
        };
    }
    // CDATA before the generic declaration path: its content may embed `>`
    // (`<![CDATA[a > b]]>`), so it must be consumed through `]]>` or the
    // remainder would leak to the screen as visible text.
    if html[start..].starts_with("<![CDATA[") {
        return match html[start..].find("]]>") {
            Some(p) => start + p + "]]>".len(),
            None => bytes.len(),
        };
    }
    if start + 1 < bytes.len() && bytes[start + 1] == b'!' {
        return consume_declaration(bytes, start);
    }
    if let Some(end) =
        raw_element_end(html, start, "script").or_else(|| raw_element_end(html, start, "style"))
    {
        return end;
    }

    // Scan the tag name + attributes, respecting quoted attribute values so a
    // `>` inside quotes does not prematurely close the tag. Skip ALL leading
    // whitespace after `<` before starting the name scan so markup like
    // `<  /p>` (multiple spaces) is handled the same as `< /p>`.
    let mut j = start + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    // If the first non-whitespace char is '/', this is a closing tag. Skip
    // past the slash AND any following whitespace so `</ p>` or `< / p >`
    // scans the bare name "p" (not just "/") — `html_tag_introduces_break`
    // treats opening/closing/self-closing forms uniformly via
    // `bare_tag_name`, so the slash itself carries no information here.
    if j < bytes.len() && bytes[j] == b'/' {
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
    }
    let name_start = j;
    let mut name_end = j;
    while j < bytes.len() {
        match bytes[j] {
            b'>' => break,
            b'"' | b'\'' => {
                let quote = bytes[j];
                j += 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
            }
            // Leading whitespace was already skipped, so the first whitespace
            // encountered marks the end of the tag name.
            c if c.is_ascii_whitespace() && name_end == name_start => {
                name_end = j;
            }
            _ => {}
        }
        j += 1;
    }
    if name_end == name_start {
        name_end = j;
    }
    // Clamp name_end to bytes.len(): an unmatched quote can advance j past
    // end-of-input (j == bytes.len() + 1 after the trailing increment), so
    // the slice would panic on unterminated tags like `<ahref="foo`.
    let name_end = name_end.min(bytes.len());
    // Trim so markup with whitespace after `<` (e.g. `< br>`, `< /p >`) still
    // matches its tag name and introduces the block boundary. The closing
    // slash was already skipped above, and `html_tag_introduces_break`
    // normalizes via `bare_tag_name`, so the bare name is passed directly
    // (re-prefixing `/` would only be stripped again).
    let bare = html[name_start..name_end].trim().to_ascii_lowercase();
    if html_tag_introduces_break(&bare) {
        out.push('\n');
    }

    // Advance past the closing '>' if one was found. Clamp to bytes.len() so
    // an unterminated tag consumes to end-of-input as documented.
    j.min(bytes.len()) + usize::from(j < bytes.len() && bytes[j] == b'>')
}

/// Return the byte index after a raw-text element's closing tag when `start`
/// begins `<script...>` or `<style...>`. Their bodies are code/CSS, not visible
/// prose; consume through the matching close so embedded markup cannot leak.
fn raw_element_end(html: &str, start: usize, tag: &str) -> Option<usize> {
    let tail = html.get(start..)?;
    let open_end = quoted_tag_end(tail.as_bytes(), 1)?;
    let opening = tail.get(1..open_end)?;
    let open_name = opening.split_ascii_whitespace().next()?;
    if !open_name.trim_end_matches('/').eq_ignore_ascii_case(tag) {
        return None;
    }
    let close = format!("</{tag}>");
    let body = tail.get(open_end + 1..)?;
    Some(
        find_ascii_case_insensitive(body, &close)
            .map_or(html.len(), |p| start + open_end + 1 + p + close.len()),
    )
}

/// Find a tag's closing `>`, ignoring delimiters inside quoted attributes.
fn quoted_tag_end(bytes: &[u8], mut i: usize) -> Option<usize> {
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return Some(i),
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// ASCII-case-insensitive substring search for fixed HTML tag literals.
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Consume a declaration (`<!…>`) starting at `bytes[start]`, returning the
/// index just past its closing `>`. Declarations can carry quoted values
/// containing `>` (e.g. `<!DOCTYPE html "foo>bar">`), so scan with quote
/// awareness instead of stopping at the first `>`; an unterminated
/// declaration consumes to end-of-input.
fn consume_declaration(bytes: &[u8], start: usize) -> usize {
    let mut j = start + 2;
    while j < bytes.len() {
        match bytes[j] {
            b'>' => return j + 1,
            b'"' | b'\'' => {
                let quote = bytes[j];
                j += 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
            }
            _ => {}
        }
        j += 1;
    }
    bytes.len()
}

/// Consume an entity starting at `bytes[start]` (`&`). Returns `(next_index,
/// decoded_text)`. Recognized entities decode to their character; an
/// unrecognized/unterminated entity returns the literal `&` so no `<`/`&`
/// suffix scan is ever needed.
fn consume_entity(html: &str, start: usize) -> (usize, String) {
    let tail = &html[start..];
    // Bound the lookup so a stray `&` near end-of-input never scans the whole
    // remaining string repeatedly. The cap is a byte offset that can land
    // inside a multi-byte char (`&aaaaaaaaaa中`), so back off to a char
    // boundary before slicing — indexing mid-char panics on crafted input.
    let mut window_end = tail.len().min(MAX_ENTITY_LEN);
    while window_end > 0 && !tail.is_char_boundary(window_end) {
        window_end -= 1;
    }
    let Some(rel_semi) = tail[..window_end].find(';') else {
        return (start + 1, "&".to_string());
    };
    let entity = &tail[..=rel_semi];
    if let Some(decoded) = decode_entity(entity) {
        (start + rel_semi + 1, decoded)
    } else {
        (start + 1, "&".to_string())
    }
}

/// True when `haystack` (already lowercased) contains a real opening tag.
///
/// `needle` is the literal `<name` prefix (e.g. `"<summary"`) — followed by
/// whitespace, `>`, or `/`, rather than a mere substring like
/// `<summary-widget>` or prose mentioning `<detailsish`. Taking the prefixed
/// needle as a static literal avoids a per-call `String` allocation.
#[must_use]
pub fn contains_open_tag(haystack: &str, needle: &str) -> bool {
    // The lowercased-haystack precondition is enforced in debug builds so a
    // future mixed-case caller fails fast instead of silently missing tags.
    debug_assert!(
        !haystack.chars().any(|c| c.is_ascii_uppercase()),
        "contains_open_tag requires a pre-lowercased haystack"
    );
    // find("") returns Some(0) and would never advance the cursor — guard
    // against an empty needle so the loop below cannot spin forever.
    if needle.is_empty() {
        return false;
    }
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find(needle) {
        let after = search_from + rel + needle.len();
        match haystack.as_bytes().get(after) {
            None => return false,
            Some(b'>' | b'/') => return true,
            Some(c) if c.is_ascii_whitespace() => return true,
            _ => search_from = after,
        }
    }
    false
}

/// True for block-level tags (opening, closing, or self-closing form) whose
/// removal should yield a line break so paragraph/list/row/section boundaries
/// survive the HTML strip — `Text<p>Para</p>` breaks both before and after
/// "Para", and `<div>a</div><div>b</div>` on one source line splits per div.
/// Consecutive boundaries (`</p><p>`) collapse downstream: the renderer's
/// blank-line handling merges empty split segments into one gap.
fn html_tag_introduces_break(name: &str) -> bool {
    matches!(
        bare_tag_name(name),
        "br" | "hr"
            | "p"
            | "li"
            | "tr"
            | "summary"
            | "details"
            | "div"
            | "table"
            | "blockquote"
            | "pre"
            | "ul"
            | "ol"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "aside"
            | "nav"
            | "figure"
            | "figcaption"
            | "address"
            | "caption"
            | "dd"
            | "dt"
            | "dl"
            | "fieldset"
            | "legend"
            | "thead"
            | "tbody"
            | "tfoot"
    )
}

/// Strip the closing-tag prefix (`/p` → `p`) and self-closing suffix
/// (`br/` → `br`) so `html_tag_introduces_break` matches the OPENING,
/// CLOSING, and SELF-CLOSING form of every block-level tag uniformly —
/// `Text<p>Para</p>` must break before "Para", not only after it.
fn bare_tag_name(name: &str) -> &str {
    let name = name.strip_prefix('/').unwrap_or(name);
    name.strip_suffix('/').unwrap_or(name)
}

/// Return the byte length of the UTF-8 sequence starting at the given lead
/// byte (clamped to the remaining slice by the caller).
fn utf8_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte >> 5 == 0b110 {
        2
    } else if byte >> 4 == 0b1110 {
        3
    } else if byte >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// Decode the handful of HTML entities bots commonly emit.
fn decode_entity(entity: &str) -> Option<String> {
    // Named entities with explicit mappings.
    if let Some(named) = match entity {
        "&amp;" => Some("&"),
        "&lt;" => Some("<"),
        "&gt;" => Some(">"),
        "&quot;" => Some("\""),
        "&apos;" => Some("'"),
        // U+00A0, matching the numeric form &#xA0; (the renderer's
        // whitespace-based wrapper treats it as whitespace anyway, so this
        // is about consistency, not wrap semantics).
        "&nbsp;" => Some("\u{00A0}"),
        "&mdash;" => Some("\u{2014}"),
        "&ndash;" => Some("\u{2013}"),
        "&hellip;" => Some("\u{2026}"),
        "&bullet;" => Some("\u{2022}"),
        "&check;" => Some("\u{2713}"),
        "&times;" => Some("\u{00d7}"),
        _ => None,
    } {
        return Some(named.to_string());
    }
    // Numeric character references: decimal (&#NNN;) or hex (&#xHH;).
    let inner = entity.strip_prefix('&')?.strip_suffix(';')?;
    let code = if let Some(hex) = inner
        .strip_prefix("#x")
        .or_else(|| inner.strip_prefix("#X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        inner.strip_prefix('#')?.parse::<u32>().ok()?
    };
    let c = char::from_u32(code)?;
    // Returns None for control characters (&#27; ESC, &#0; NUL, C1 set…) so
    // they never decode to output; consume_entity treats None as a literal
    // `&`. Tab and newline are the two control chars that ARE meaningful
    // (newline = block boundary, tab = visible alignment), matching the
    // literal-character policy in strip_html_to_text's main loop; CR and
    // all other control chars stay rejected. Unicode noncharacters
    // (U+FDD0–U+FDEF, and U+xFFFE/U+FFFF in every plane) are also rejected —
    // they are not useful visible text and char::from_u32 admits them while
    // is_control() does not.
    if c.is_control() && c != '\t' && c != '\n' {
        return None;
    }
    let cp = c as u32;
    if (0xFDD0..=0xFDEF).contains(&cp) || (cp & 0xFFFE) == 0xFFFE {
        return None;
    }
    Some(c.to_string())
}
