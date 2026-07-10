//! Tests for the markdown renderer (issue #155).

use super::*;

fn render(md: &str) -> String {
    render_markdown_lines(md).join("\n")
}

#[test]
fn empty_input_yields_no_lines() {
    assert!(render_markdown_lines("").is_empty());
}

#[test]
fn heading_gets_rule_and_no_markers() {
    let out = render("# Title\n");
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines.iter().any(|l| l.contains("Title")),
        "heading text: {out}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.chars().all(|c| c == '─') && !l.is_empty()),
        "heading must have a trailing rule: {out}"
    );
    // Markdown heading markers must NOT reach the screen (issue #155).
    assert!(!out.contains('#'), "no raw heading markers: {out}");
}

#[test]
fn emphasis_is_stripped() {
    let out = render("**bold** and *italic* and `code`");
    assert!(out.contains("bold"));
    assert!(out.contains("italic"));
    assert!(out.contains("code"));
    assert!(!out.contains("**"), "no raw bold markers: {out}");
    assert!(!out.contains('*'), "no raw italic markers: {out}");
    assert!(!out.contains('`'), "no raw backticks: {out}");
}

#[test]
fn fenced_code_block_is_framed() {
    let out = render("```bash\ncargo test\n```\n");
    assert!(out.contains("cargo test"), "code body present: {out}");
    assert!(
        !out.contains("```"),
        "no raw triple backticks on screen: {out}"
    );
    assert!(
        out.contains(CODE_FENCE_SIDE),
        "code block is rule-framed: {out}"
    );
}

#[test]
fn unordered_list_uses_bullets() {
    let out = render("- one\n- two\n");
    assert!(out.contains("* one"), "bullet list: {out}");
    assert!(out.contains("* two"));
}

#[test]
fn ordered_list_uses_numbers() {
    let out = render("1. first\n2. second\n");
    assert!(out.contains("1. first"), "ordered list: {out}");
    assert!(out.contains("2. second"));
}

#[test]
fn task_list_uses_boxes() {
    let out = render("- [x] done\n- [ ] todo\n");
    assert!(out.contains("[x] done"), "checked task: {out}");
    assert!(out.contains("[ ] todo"), "unchecked task: {out}");
}

#[test]
fn raw_html_is_stripped() {
    let out = render("<details><summary>Click me</summary>body</details>");
    assert!(
        !out.contains('<') && !out.contains('>'),
        "no raw angle brackets: {out}"
    );
    assert!(out.contains("Click me"), "summary text kept: {out}");
}

#[test]
fn html_entities_decoded() {
    let out = render("a &amp; b &lt;tag&gt;");
    assert!(out.contains("a & b"), "amp decoded: {out}");
    assert!(out.contains("<tag>"), "lt/gt decoded: {out}");
}

#[test]
fn link_keeps_url_when_distinct() {
    let out = render("[text](https://example.com)");
    assert!(out.contains("text"), "link text: {out}");
    assert!(out.contains("https://example.com"), "link url kept: {out}");
}

#[test]
fn image_uses_alt_text() {
    let out = render("![a logo](https://example.com/x.png)");
    assert!(out.contains("a logo"), "image alt text: {out}");
    assert!(!out.contains("!["), "no raw image markdown: {out}");
}

#[test]
fn table_renders_aligned_columns() {
    let out = render("| a | b |\n|---|---|\n| c | d |\n");
    // Header and body rows render their cell text with column separators.
    assert!(out.contains("a  b"), "table header: {out}");
    assert!(out.contains("c  d"), "table body: {out}");
    assert!(
        out.contains("---") || out.contains(':'),
        "table has alignment separator: {out}"
    );
}

#[test]
fn blockquote_is_marked() {
    let out = render("> quoted text");
    assert!(
        out.contains("> ") && out.contains("quoted text"),
        "blockquote marked: {out}"
    );
}

#[test]
fn paragraph_breaks_preserved() {
    let out = render("first paragraph\n\nsecond paragraph");
    assert!(out.contains("first paragraph"));
    assert!(out.contains("second paragraph"));
    // A blank line separates the two paragraphs.
    assert!(out.contains("\n\n"), "paragraph break preserved: {out:?}");
}

#[test]
fn soft_breaks_preserve_author_line_structure() {
    // Consecutive plain lines (no blank line between) form a single
    // paragraph with soft breaks, but the author's line structure must be
    // preserved — NOT collapsed onto one line (issue #155 regression
    // guard: a multi-line plain-text body must keep its line count).
    let out = render("body line 0\nbody line 1\nbody line 2");
    assert!(out.contains("body line 0"));
    assert!(out.contains("body line 1"));
    assert!(out.contains("body line 2"));
    // Each line must be on its own rendered line (no collapse to one row).
    assert!(
        out.lines().filter(|l| l.contains("body line")).count() >= 3,
        "soft breaks must preserve line structure, got: {out:?}"
    );
}

#[test]
fn nested_list_indented() {
    let out = render("- top\n  - nested\n");
    assert!(out.contains("* top"), "top item: {out}");
    assert!(out.contains("nested"), "nested item present: {out}");
    // The nested item must be more indented than the top item.
    let leading = |needle: &str| -> usize {
        out.lines()
            .find(|l| l.contains(needle))
            .map_or(0, |l| l.chars().take_while(|c| *c == ' ').count())
    };
    let top_leading = leading("* top");
    let nested_leading = leading("nested");
    assert!(
        nested_leading > top_leading,
        "nested list must be indented deeper than parent: {out}"
    );
}

// ── Invariant: one element = one screen line (issue #155 review) ──────

/// The renderer's central invariant: no returned line may contain an
/// embedded newline. A single `Vec` element with a newline would desync
/// the `pr_detail_content_line_count`/`detail_content_line_count` scroll
/// bounds (which count builder elements) from the rendered physical lines,
/// and could spoof the structural subfocus predicates. HTML block-boundary
/// tags (`</p>`, `<br>`, `</li>`) and inline HTML are the main source of
/// newlines, so they are exercised here.
#[test]
fn no_line_contains_embedded_newline() {
    for input in [
        "<p>a</p><p>b</p>",
        "<p>a</p><br><p>- fake APPROVED  now</p>",
        "text<br>more<br>text",
        "<ul><li>one</li><li>two</li></ul>",
        "<details><summary>S</summary>body</details>",
        "<p>line1\nline2</p>",
        "a &amp; b<br>c &lt;tag&gt;",
    ] {
        let lines = render_markdown_lines(input);
        assert!(
            lines.iter().all(|l| !l.contains('\n')),
            "embedded newline in rendered line for input {input:?}: {lines:?}"
        );
    }
}

/// `<p>`/`<br>` boundaries must produce SEPARATE lines (not a single line
/// with embedded text), so the line count matches the rendered output.
#[test]
fn html_block_boundaries_produce_separate_lines() {
    let lines = render_markdown_lines("<p>first</p><p>second</p>");
    assert!(
        lines.iter().any(|l| l.contains("first")),
        "first paragraph present: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("second")),
        "second paragraph present: {lines:?}"
    );
    // The two paragraphs must be on different elements.
    let first_line = lines.iter().find(|l| l.contains("first"));
    let second_line = lines.iter().find(|l| l.contains("second"));
    assert!(
        first_line.is_some_and(|f| second_line.is_some_and(|s| f != s)),
        "paragraphs must be separate lines: {lines:?}"
    );
}

/// A `>` inside a quoted attribute value must NOT terminate the tag early
/// (regression for the original scanner's first-`>` bug).
#[test]
fn gt_inside_quoted_attribute_does_not_close_tag() {
    let out = render(r#"<span title="1 > 0">ok</span>"#);
    assert!(out.contains("ok"), "tag inner text kept: {out}");
    assert!(!out.contains('"'), "attribute garbage must not leak: {out}");
    // The attribute's `0` after a `>` must not appear as leaked markup.
    assert!(!out.contains('>'), "no raw angle brackets leak: {out}");
}

/// HTML comments (`<!-- … -->`) are dropped entirely.
#[test]
fn html_comments_are_dropped() {
    let out = render("before<!-- secret -->after");
    assert!(out.contains("before"), "text before comment: {out}");
    assert!(out.contains("after"), "text after comment: {out}");
    assert!(
        !out.contains("secret"),
        "comment body must be dropped: {out}"
    );
    assert!(
        !out.contains("<!--") && !out.contains("-->"),
        "no comment delimiters: {out}"
    );
}

/// Unmatched `<` (long runs from malformed/bot content) must be handled
/// without quadratic blowup or panicking. comrak treats a bare run of `<`
/// with no closing `>` as literal text (not an HTML tag), so it survives —
/// the guarantee is that this never hangs and the trailing text is intact.
#[test]
fn unmatched_angle_brackets_do_not_hang() {
    let out = render("<<<<<<<<<<<<<<<<<<<<<<<<text");
    assert!(out.contains("text"), "text survives: {out}");
}
