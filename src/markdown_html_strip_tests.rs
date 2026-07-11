//! Tests for the byte-level HTML-strip state machine (issue #155).
//!
//! Split out of `markdown_render_tests.rs` (source-size limit): these tests
//! exercise `strip_html_to_text` / `contains_open_tag` directly, while the
//! renderer-level tests continue to cover the same pipeline through
//! `render_markdown_lines`.

use super::*;

/// The HTML strip state machine must not pass through literal control
/// characters (ESC, NUL, C1 CSI) from raw text — defense-in-depth so the
/// module's own guarantee holds independently of MarkdownRenderer::push.
#[test]
fn strip_html_drops_raw_control_characters() {
    let out = strip_html_to_text("a\u{1b}[31mred\u{0}b\u{9b}c");
    assert_eq!(out, "a[31mredbc", "ESC/NUL/C1-CSI removed: {out:?}");
    // Newlines (block boundaries) and tabs are meaningful and must survive.
    assert_eq!(
        strip_html_to_text("line1\nline2"),
        "line1\nline2",
        "newlines preserved"
    );
    assert!(strip_html_to_text("a\tb").contains('\t'), "tabs preserved");
}

/// Multiple spaces between `<` and the tag name must still produce a block
/// boundary. The scanner must skip ALL leading whitespace before the tag
/// name, not just one char (regression: `<  br>` lost its line break).
#[test]
fn multiple_spaces_before_tag_name_still_break() {
    // `<  br>` (two spaces) must introduce a line break.
    assert_eq!(
        strip_html_to_text("a<  br>b"),
        "a\nb",
        "double-space `<  br>` must break"
    );
    // `<  /p>` (two spaces) after text must introduce a newline.
    assert_eq!(
        strip_html_to_text("alpha<  /p>"),
        "alpha\n",
        "double-space `<  /p>` must break"
    );
    // Existing doc case: single spaces `< /p >` must still break.
    assert_eq!(
        strip_html_to_text("alpha< /p >"),
        "alpha\n",
        "single-space `< /p >` must still break"
    );
    // Even with many spaces, the tag name must be found.
    assert_eq!(
        strip_html_to_text("x<    br>y"),
        "x\ny",
        "four-space `<    br>` must break"
    );
}

/// An empty needle must return `false` immediately rather than infinite-loop.
/// `find("")` returns `Some(0)` without advancing the search cursor, so
/// without this guard the loop spins forever.
#[test]
fn contains_open_tag_empty_needle_terminates() {
    assert!(!contains_open_tag("abc", ""));
}

/// A tag with an unmatched quote and no whitespace before it must not panic.
#[test]
fn unmatched_quote_in_tag_does_not_panic() {
    // Unmatched quote, no whitespace → unterminated tag → empty output.
    let out = strip_html_to_text("<ahref=\"foo");
    assert_eq!(
        out, "",
        "unterminated tag with unmatched quote yields empty: {out:?}"
    );
    // Text before the unterminated tag survives.
    let out2 = strip_html_to_text("text<div class=\"x");
    assert_eq!(
        out2, "text",
        "text before unterminated tag survives: {out2:?}"
    );
}

/// Numeric character references that resolve to Unicode noncharacters
/// (U+FFFF, U+FFFE, U+FDD0–U+FDEF, U+xFFFE/U+xFFFF in every plane) must NOT
/// decode. An unrecognized entity decodes to a literal `&` so the output
/// equals the original input string for these cases. Normal entities like
/// `&#65;` still decode to their characters.
#[test]
fn numeric_entity_noncharacters_rejected() {
    // U+FFFF → noncharacter, decode_entity returns None → literal '&'.
    let out = strip_html_to_text("a&#xFFFF;b");
    assert_eq!(out, "a&#xFFFF;b", "U+FFFF noncharacter rejected: {out:?}");

    // U+FDD0 → noncharacter.
    let out2 = strip_html_to_text("a&#xFDD0;b");
    assert_eq!(out2, "a&#xFDD0;b", "U+FDD0 noncharacter rejected: {out2:?}");

    // U+FFFE → noncharacter (decimal form).
    let out3 = strip_html_to_text("a&#65534;b");
    assert_eq!(out3, "a&#65534;b", "U+FFFE noncharacter rejected: {out3:?}");

    // Normal entity still decodes.
    let out4 = strip_html_to_text("&#65;");
    assert_eq!(out4, "A", "normal entity still decodes: {out4:?}");
}

/// Whitespace between the `/` and the tag name in a closing tag (e.g.
/// `</ p>` or `< / p >`) must still produce a block boundary. Regression:
/// the name scan stopped at the whitespace after `/`, yielding a name of
/// just `"/"`, which `html_tag_introduces_break` rejected.
#[test]
fn whitespace_after_slash_still_breaks() {
    assert_eq!(
        strip_html_to_text("alpha</ p>"),
        "alpha\n",
        "`</ p>` must introduce a block boundary"
    );
    assert_eq!(
        strip_html_to_text("alpha< / p >"),
        "alpha\n",
        "`< / p >` must introduce a block boundary"
    );
}

/// `&#10;` (LF) and `&#9;` (TAB) must decode the same way their literal
/// counterparts do — `strip_html_to_text` preserves literal `\n`/`\t`, so the
/// entity forms should too. `\r` (`&#13;`) must NOT decode (it is not a
/// meaningful block boundary; callers split on `\n` only).
#[test]
fn newline_and_tab_entities_decode() {
    assert_eq!(
        strip_html_to_text("a&#10;b"),
        "a\nb",
        "&#10; decodes to newline (block boundary)"
    );
    assert_eq!(strip_html_to_text("a&#9;b"), "a\tb", "&#9; decodes to tab");
    assert!(
        !strip_html_to_text("a&#13;b").contains('\r'),
        "&#13; (CR) must NOT decode"
    );
}
