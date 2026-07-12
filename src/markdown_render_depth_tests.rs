//! Stack-overflow regression tests for the markdown renderer's recursion
//! depth bound (issue #155 review remediation).
//!
//! Untrusted GitHub content can contain deeply-nested markdown (blockquotes,
//! lists, emphasis) that — without a depth guard — overflows the process
//! stack via mutually-recursive rendering. These tests prove the
//! `MAX_RENDER_DEPTH` guard prevents a crash: block recursion emits a visible
//! fallback line, while inline recursion truncates safely at the limit.
//!
//! Split out of `markdown_render_tests.rs` to keep each test module under
//! the source-file length policy.

use super::*;

/// A body of 20,000 nested blockquotes must NOT crash the process. Before
/// the depth guard, this overflowed the stack via
/// `render_block_children → render_block → render_block_quote`.
///
/// After the guard, the renderer stops recursing at `MAX_RENDER_DEPTH` and
/// emits `"(content nested too deeply)"` as a visible fallback.
#[test]
fn deeply_nested_blockquotes_do_not_overflow_stack() {
    let body = format!("{}deep", "> ".repeat(20_000));
    let lines = render_markdown_lines(&body);
    assert!(!lines.is_empty(), "deeply nested content must still render");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("(content nested too deeply)")),
        "expected at least one depth-limit marker line, got: {lines:?}"
    );
}

/// A deeply nested list must NOT crash. List nesting recurses through
/// `render_list → render_list_item → render_list`, the same depth funnel.
/// Each level is indented 2 spaces deeper than the previous (the
/// minimum CommonMark indent for sub-list nesting). 500 levels is far
/// above `MAX_RENDER_DEPTH` (100) but small enough to build quickly.
#[test]
fn deeply_nested_lists_do_not_overflow_stack() {
    let mut body = String::new();
    let mut indent = String::new();
    for _ in 0..500 {
        body.push_str(&indent);
        body.push_str("- x\n");
        indent.push_str("  ");
    }
    body.push_str("deep");
    let lines = render_markdown_lines(&body);
    assert!(!lines.is_empty(), "deeply nested list must still render");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("(content nested too deeply)")),
        "expected at least one depth-limit marker line for nested list"
    );
}

/// Deeply nested INLINE structures (emphasis chains, bracket nests) must not
/// overflow the stack either: before the inline depth guard, 15k unclosed
/// `*a ` runs and 8k matched `**` pairs both aborted the process. Content
/// nested beyond `MAX_RENDER_DEPTH` is decoration-only nesting and is
/// dropped; the fully-nested `**…**x` case therefore legitimately renders to
/// nothing (the whole text sits past the bound) — the invariant under test
/// is "no crash", not output shape.
#[test]
fn deeply_nested_inline_emphasis_does_not_overflow_stack() {
    let mut unclosed = String::new();
    for _ in 0..15_000 {
        unclosed.push_str("*a ");
    }
    unclosed.push('b');
    let lines = render_markdown_lines(&unclosed);
    assert!(
        !lines.is_empty(),
        "unclosed-emphasis run must still render its visible text"
    );

    let mut brackets = String::new();
    for _ in 0..15_000 {
        brackets.push('[');
    }
    brackets.push('x');
    for _ in 0..15_000 {
        brackets.push(']');
    }
    let bracket_lines = render_markdown_lines(&brackets);
    assert!(
        !bracket_lines.is_empty(),
        "bracket nest must still render its visible text"
    );

    let mut matched = String::new();
    for _ in 0..8_000 {
        matched.push_str("**");
    }
    matched.push('x');
    for _ in 0..8_000 {
        matched.push_str("**");
    }
    // Must not crash; the text is nested past MAX_RENDER_DEPTH so the
    // rendered output is empty by design.
    let matched_lines = render_markdown_lines(&matched);
    assert!(
        matched_lines.is_empty(),
        "fully-nested matched emphasis sits past MAX_RENDER_DEPTH and renders to nothing by design, got: {matched_lines:?}"
    );
}

/// A blockquote nested inside a list item must keep the parent's indent
/// BEFORE the quote bar (`  > text`), not bolt the bar onto column 0 ahead
/// of the indent (`>   text`), so nested quotes stay visually aligned under
/// their list item.
#[test]
fn blockquote_nested_in_list_keeps_indent_before_quote_bar() {
    let lines = render_markdown_lines("- item\n\n  > quoted in list");
    assert!(
        lines.iter().any(|l| l.contains("> quoted in list")),
        "quote bar directly precedes the text: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .filter(|l| l.contains("quoted in list"))
            .all(|l| l.starts_with(' ')),
        "nested quote line starts with the list indent, not the bar: {lines:?}"
    );
}

/// Bidi override stripping must hold in NESTED inline/block contexts — link
/// text, emphasis, inline code, code blocks, and headings all reach the
/// screen through the same `push` chokepoint, so none of them may leak a
/// bidi override (Trojan Source vector).
#[test]
fn bidi_controls_stripped_in_nested_contexts() {
    let cases = [
        "[user\u{202E}txt](https://example.com)",
        "*emph\u{202E}asis*",
        "`code\u{202E}span`",
        "```\nblock\u{202E}code\n```",
        "# head\u{202E}ing",
        "> quo\u{202E}te",
        "- item\u{202E}text",
    ];
    for case in cases {
        let out = render_markdown_lines(case).join("\n");
        assert!(
            !out.contains('\u{202E}'),
            "RLO stripped from {case:?}: {out:?}"
        );
    }
}

/// An empty ATX heading (`#` with no text) must not emit a stray
/// horizontal rule — the label line would be blank, so the rule would
/// float with nothing above it.
#[test]
fn empty_heading_emits_no_stray_rule() {
    for case in ["#", "# ", "##  \n"] {
        let lines = render_markdown_lines(case);
        assert!(
            lines.is_empty(),
            "empty heading {case:?} renders nothing: {lines:?}"
        );
    }
    // A heading with text keeps its label + rule.
    let with_text = render_markdown_lines("# title");
    assert!(
        with_text.iter().any(|l| l.contains("title")),
        "non-empty heading keeps its label: {with_text:?}"
    );
    assert!(
        with_text.iter().any(|l| l.contains('\u{2500}')),
        "non-empty heading keeps its rule: {with_text:?}"
    );
}
