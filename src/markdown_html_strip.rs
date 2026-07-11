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
    if start + 1 < bytes.len() && bytes[start + 1] == b'!' {
        // Declarations can carry quoted values containing `>` (e.g.
        // `<!DOCTYPE html "foo>bar">`), so scan with quote awareness like
        // the tag path below instead of stopping at the first `>`.
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
        return bytes.len();
    }

    // Scan the tag name + attributes, respecting quoted attribute values so a
    // `>` inside quotes does not prematurely close the tag. Skip ALL leading
    // whitespace after `<` before starting the name scan so markup like
    // `<  /p>` (multiple spaces) is handled the same as `< /p>`.
    let mut j = start + 1;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
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
    // matches its tag name and introduces the block boundary.
    let name = html[name_start..name_end].trim().to_ascii_lowercase();
    if html_tag_introduces_break(&name) {
        out.push('\n');
    }
    // Advance past the closing '>' if one was found. Clamp to bytes.len() so
    // an unterminated tag consumes to end-of-input as documented.
    j.min(bytes.len()) + usize::from(j < bytes.len() && bytes[j] == b'>')
}

/// Consume an entity starting at `bytes[start]` (`&`). Returns `(next_index,
/// decoded_text)`. Recognized entities decode to their character; an
/// unrecognized/unterminated entity returns the literal `&` so no `<`/`&`
/// suffix scan is ever needed.
fn consume_entity(html: &str, start: usize) -> (usize, String) {
    let tail = &html[start..];
    // Bound the lookup so a stray `&` near end-of-input never scans the whole
    // remaining string repeatedly.
    let window_end = tail.len().min(MAX_ENTITY_LEN);
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

/// True when `haystack` (already lowercased) contains a real opening tag —
/// `needle` is the literal `<name` prefix (e.g. `"<summary"`) — followed by
/// whitespace, `>`, or `/`, rather than a mere substring like
/// `<summary-widget>` or prose mentioning `<detailsish`. Taking the prefixed
/// needle as a static literal avoids a per-call `String` allocation.
pub fn contains_open_tag(haystack: &str, needle: &str) -> bool {
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

/// True for the block-level closing tags and `<br>`/`<hr>` whose removal
/// should yield a line break so paragraph/list/row/section boundaries survive
/// the HTML strip (e.g. `<div>a</div><div>b</div>` on one source line).
fn html_tag_introduces_break(name: &str) -> bool {
    matches!(
        name,
        "br" | "br/"
            | "/br"
            | "/br/"
            | "hr"
            | "hr/"
            | "/p"
            | "/li"
            | "/tr"
            | "/summary"
            | "/details"
            | "/div"
            | "/table"
            | "/blockquote"
            | "/pre"
            | "/ul"
            | "/ol"
            | "/h1"
            | "/h2"
            | "/h3"
            | "/h4"
            | "/h5"
            | "/h6"
            | "/section"
            | "/article"
            | "/header"
            | "/footer"
            | "/main"
            | "/aside"
            | "/nav"
            | "/figure"
            | "/figcaption"
    )
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
        "&nbsp;" => Some(" "),
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
    // `&`. Tab is not meaningful mid-line here either, so only fully printable
    // characters pass through. Unicode noncharacters (U+FDD0–U+FDEF, and
    // U+xFFFE/U+xFFFF in every plane) are also rejected — they are not useful
    // visible text and char::from_u32 admits them while is_control() does not.
    if c.is_control() {
        return None;
    }
    let cp = c as u32;
    if (0xFDD0..=0xFDEF).contains(&cp) || (cp & 0xFFFE) == 0xFFFE {
        return None;
    }
    Some(c.to_string())
}

#[cfg(test)]
#[path = "markdown_html_strip_tests.rs"]
mod tests;
