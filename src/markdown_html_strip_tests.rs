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
    let out = strip_html_to_text("a\u{1b}[31mred\u{0}b\u{7f}\u{9b}c");
    assert_eq!(out, "a[31mredbc", "ESC/NUL/DEL/C1-CSI removed: {out:?}");
    // Newlines (block boundaries) and tabs are meaningful and must survive.
    assert_eq!(
        strip_html_to_text("line1\nline2"),
        "line1\nline2",
        "newlines preserved"
    );
    assert_eq!(strip_html_to_text("a\tb"), "a\tb", "tabs preserved exactly");
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
    // Text before the unterminated tag survives (the opening `<div` is a
    // block boundary, so it contributes a trailing newline).
    let out2 = strip_html_to_text("text<div class=\"x");
    assert_eq!(
        out2, "text\n",
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
    // A rejected entity passes through literally (consume_entity emits the
    // `&` and the rest is copied as text), so the exact output IS the input.
    assert_eq!(
        strip_html_to_text("a&#13;b"),
        "a&#13;b",
        "&#13; (CR) must NOT decode; the entity stays literal"
    );
}

/// Opening block-level tags must break the same as their closing forms:
/// `Text<p>Para</p>` yields "Text" and "Para" on separate lines instead of
/// fusing them ("TextPara").
#[test]
fn opening_block_tags_break_lines() {
    assert_eq!(
        strip_html_to_text("Text<p>Para</p>"),
        "Text\nPara\n",
        "opening <p> breaks before the paragraph"
    );
    assert_eq!(
        strip_html_to_text("a<div>b</div>"),
        "a\nb\n",
        "opening <div> breaks"
    );
    // Inline tags still do NOT break.
    assert_eq!(
        strip_html_to_text("a<b>bold</b>c"),
        "aboldc",
        "inline tags introduce no break"
    );
}

/// `&nbsp;` decodes to U+00A0 (non-breaking space), consistent with the
/// numeric form `&#xA0;`.
#[test]
fn nbsp_decodes_to_nonbreaking_space() {
    assert_eq!(
        strip_html_to_text("a&nbsp;b"),
        "a\u{00A0}b",
        "&nbsp; is U+00A0"
    );
    assert_eq!(
        strip_html_to_text("a&#xA0;b"),
        "a\u{00A0}b",
        "&#xA0; matches"
    );
}

/// A multi-byte character straddling the entity-scan window boundary must
/// not panic (crafted untrusted markdown must never panic). The prefix
/// lengths are derived from `MAX_ENTITY_LEN` so the window boundary lands
/// mid-character even if the constant changes: the `&` occupies 1 byte, so
/// `MAX_ENTITY_LEN - 2` filler chars put the boundary 1 byte into the
/// 3-byte CJK char, and `MAX_ENTITY_LEN - 3` puts it 2 bytes into the
/// 4-byte emoji.
#[test]
fn entity_window_on_multibyte_boundary_does_not_panic() {
    let input = format!("&{}中", "a".repeat(MAX_ENTITY_LEN - 2));
    let out = strip_html_to_text(&input);
    assert_eq!(out, input, "unterminated entity passes through: {out:?}");
    // Same shape with a 4-byte emoji on the boundary.
    let input2 = format!("&{}😀", "a".repeat(MAX_ENTITY_LEN - 3));
    let out2 = strip_html_to_text(&input2);
    assert_eq!(out2, input2, "emoji boundary passes through: {out2:?}");
}

/// A CDATA section must be dropped entirely — its content can contain `>`
/// (`<![CDATA[a > b]]>`), so terminating at the first `>` leaks the
/// remainder (" b]]>") to the screen as visible text. Unterminated CDATA
/// consumes to end-of-input like unterminated comments.
#[test]
fn cdata_sections_are_dropped_entirely() {
    assert_eq!(
        strip_html_to_text("<![CDATA[a > b]]>after"),
        "after",
        "CDATA content with an embedded '>' is dropped through ']]>'"
    );
    assert_eq!(
        strip_html_to_text("before<![CDATA[x]]>after"),
        "beforeafter",
        "simple CDATA is dropped"
    );
    assert_eq!(
        strip_html_to_text("text<![CDATA[never closed"),
        "text",
        "unterminated CDATA consumes to end-of-input"
    );
}

/// Definition-list / form / table-section block tags also introduce line
/// breaks so adjacent blocks don't fuse (review batch: address, caption,
/// dd/dt/dl, fieldset, legend, thead/tbody/tfoot were missing). The raw
/// strip may emit consecutive `\n`s (opening AND closing forms break);
/// the renderer collapses those into single blank gaps downstream, so the
/// invariant here is "distinct blocks land on distinct lines".
#[test]
fn definition_and_table_section_tags_break_lines() {
    let dl = strip_html_to_text("<dl><dt>term</dt><dd>def</dd></dl>");
    let dl_lines: Vec<&str> = dl.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(dl_lines, ["term", "def"], "dt/dd split lines: {dl:?}");

    let addr = strip_html_to_text("a<address>b</address>c");
    let addr_lines: Vec<&str> = addr.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(addr_lines, ["a", "b", "c"], "address splits: {addr:?}");

    let tbl = strip_html_to_text("<thead><tr>h</tr></thead><tbody><tr>d</tr></tbody>");
    let tbl_lines: Vec<&str> = tbl.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(tbl_lines, ["h", "d"], "thead/tbody split: {tbl:?}");
}

/// Script/style bodies are not visible prose and may contain markup-like
/// bytes; consume through the matching close instead of leaking code/CSS.
#[test]
fn raw_script_and_style_elements_are_dropped_as_units() {
    assert_eq!(
        strip_html_to_text("before<script>if (a < b) x = '</not-script>';</SCRIPT>after"),
        "beforeafter"
    );
    assert_eq!(
        strip_html_to_text("a<STYLE type='text/css'>x::after { content: '<b>'; }</style>b"),
        "ab"
    );
    assert_eq!(
        strip_html_to_text(r#"x<script data-value="a > b">alert(1)</script>y"#),
        "xy",
        "quoted > must not end the script opening tag"
    );
}
