//! Stack-overflow regression tests for the markdown renderer's recursion
//! depth bound (issue #155 review remediation).
//!
//! Untrusted GitHub content can contain deeply-nested markdown (blockquotes,
//! lists, emphasis) that — without a depth guard — overflows the process
//! stack via the mutually-recursive block-rendering walk. These tests prove
//! the `MAX_RENDER_DEPTH` guard prevents a crash and emits a visible
//! fallback line instead.
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
