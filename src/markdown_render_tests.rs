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
fn whitespace_only_input_yields_no_lines() {
    // Whitespace-only must short-circuit so callers' "(no body)" placeholders
    // trigger (comrak would otherwise parse it into blank-line paragraphs).
    assert!(render_markdown_lines("   ").is_empty());
    assert!(render_markdown_lines("\n\n").is_empty());
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
    // The fence top/bottom borders must not carry trailing whitespace.
    assert!(
        !out.lines()
            .any(|l| l.ends_with(' ') && l.contains(CODE_FENCE_TOP)),
        "no trailing whitespace on fence borders: {out:?}"
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

/// A `<details>` block prefixes only the summary (first line) with the toggle
/// glyph; subsequent body lines render as plain text.
#[test]
fn details_toggle_glyph_only_on_summary() {
    let out = render("<details><summary>Title</summary>body1<br>body2</details>");
    let toggle_count = out.lines().filter(|l| l.starts_with("▶")).count();
    assert_eq!(toggle_count, 1, "only one toggle glyph: {out}");
    assert!(out.contains("Title"), "summary present: {out}");
    assert!(out.contains("body1"), "body1 present: {out}");
    assert!(out.contains("body2"), "body2 present: {out}");
    // The body lines must NOT carry the toggle glyph.
    assert!(
        !out.lines()
            .any(|l| l.starts_with("▶") && l.contains("body")),
        "body lines must not have the toggle glyph: {out}"
    );
}

#[test]
fn html_entities_decoded() {
    let out = render("a &amp; b &lt;tag&gt;");
    assert!(out.contains("a & b"), "amp decoded: {out}");
    assert!(out.contains("<tag>"), "lt/gt decoded: {out}");
}

/// Numeric character references (decimal and hex) must decode to their
/// characters, not leak as raw markup.
#[test]
fn numeric_html_entities_decoded() {
    let out = render("&#39; &#10003; &#x41;");
    assert!(out.contains('\''), "decimal 39 decoded: {out}");
    assert!(out.contains('\u{2713}'), "decimal checkmark decoded: {out}");
    assert!(out.contains('A'), "hex 41 decoded: {out}");
    assert!(!out.contains("&#"), "no raw numeric entities: {out}");
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
    assert!(out.contains("\n\n"), "paragraph break preserved: {out:?}");
}

/// Content following an HTML block must compose with a blank separator, the
/// same as every other block renderer (regression for `render_html_block`
/// omitting its trailing `push_blank()`).
#[test]
fn content_after_html_block_is_separated() {
    let out = render("<details><summary>S</summary>body</details>\n\nafter");
    assert!(out.contains("after"), "trailing paragraph present: {out}");
    // A blank line must separate the HTML block from the following paragraph.
    let after_idx = out
        .lines()
        .position(|l| l.contains("after"))
        .unwrap_or(usize::MAX);
    assert_ne!(after_idx, usize::MAX, "after paragraph located: {out}");
    let prev = out
        .lines()
        .nth(after_idx.saturating_sub(1))
        .unwrap_or("<missing>");
    assert!(
        prev.is_empty() || out.lines().take(after_idx).any(str::is_empty),
        "HTML block separated from trailing content by a blank: {out}"
    );
}

#[test]
fn soft_breaks_preserve_author_line_structure() {
    let out = render("body line 0\nbody line 1\nbody line 2");
    assert!(out.contains("body line 0"));
    assert!(out.contains("body line 1"));
    assert!(out.contains("body line 2"));
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

/// A tight list (no blank lines between items in the source) renders its items
/// with NO inter-item blank line, matching GFM tight-list semantics.
#[test]
fn tight_list_has_no_inter_item_blank_lines() {
    let out = render("- a\n- b\n- c");
    let lines: Vec<&str> = out.lines().collect();
    let item_count = lines.iter().filter(|l| l.contains("* ")).count();
    assert_eq!(item_count, 3, "three items rendered: {out}");
    let Some(first) = lines.iter().position(|l| *l == "* a") else {
        panic!("first item present: {out}");
    };
    let second = lines.get(first + 1).copied().unwrap_or("<missing>");
    let third = lines.get(first + 2).copied().unwrap_or("<missing>");
    assert_eq!(second, "* b", "no blank between tight items: {out}");
    assert_eq!(third, "* c", "no blank between tight items: {out}");
}

/// A loose list (blank line between items in the source) separates items with
/// a blank line.
#[test]
fn loose_list_has_inter_item_blank_lines() {
    let out = render("- a\n\n- b");
    let lines: Vec<&str> = out.lines().collect();
    let Some(a) = lines.iter().position(|l| *l == "* a") else {
        panic!("first item present: {out}");
    };
    let blank = lines.get(a + 1).copied().unwrap_or("<missing>");
    let second = lines.get(a + 2).copied().unwrap_or("<missing>");
    assert_eq!(blank, "", "loose list has a blank between items: {out}");
    assert_eq!(second, "* b", "second item after blank: {out}");
}

/// A wrapped list item's continuation lines must align under the marker's
/// content (NOT double-indent at nesting levels > 0). Regression for the
/// wrap_indent + cont_pad double-prefix bug.
#[test]
fn wrapped_list_continuation_aligns_under_content() {
    // A long first item that wraps; the wrapped tail must align under the
    // text after the "* " marker, and must NOT exceed the marker column.
    let md = "- alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu
";
    let out = render(md);
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    let first = lines.first().copied().unwrap_or("<missing>");
    assert!(first.starts_with("* "), "first line has bullet: {out}");
    // Find the wrapped continuation (a non-bulleted line that contains one of
    // the later words).
    let cont = lines
        .iter()
        .find(|l| {
            !l.starts_with('*') && (l.contains("kappa") || l.contains("lambda") || l.contains("mu"))
        })
        .copied();
    if let Some(cont_line) = cont {
        let cont_lead = cont_line.chars().take_while(|c| *c == ' ').count();
        // Continuation aligns at column 2 (under the text after "* ").
        assert_eq!(
            cont_lead, 2,
            "continuation aligns under marker content: {out}"
        );
    }
}

/// Multibyte text must be measured in display columns (char count), not bytes,
/// so it is not wrapped prematurely. A short CJK run well under the soft width
/// stays on one line.
#[test]
fn multibyte_text_not_wrapped_prematurely() {
    // 10 CJK chars = 30 bytes but 10 display columns; well under width 78.
    let md = "中文测试中文测试中";
    let out = render(md);
    let non_empty = out.lines().filter(|l| !l.is_empty()).count();
    assert_eq!(
        non_empty, 1,
        "short multibyte text stays on one line: {out}"
    );
    assert!(out.contains("中文测试中文测试中"));
}

// ── Invariant: one element = one screen line (issue #155 review) ──────

/// The renderer's central invariant: no returned line may contain an
/// embedded newline. HTML block-boundary tags (`</p>`, `<br>`, `</li>`) and
/// inline HTML are the main source of newlines, so they are exercised here.
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

/// `<p>`/`<br>` boundaries must produce SEPARATE lines, so the line count
/// matches the rendered output.
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
    let first_line = lines.iter().find(|l| l.contains("first"));
    let second_line = lines.iter().find(|l| l.contains("second"));
    assert!(
        first_line.is_some_and(|f| second_line.is_some_and(|s| f != s)),
        "paragraphs must be separate lines: {lines:?}"
    );
}

/// A `>` inside a quoted attribute value must NOT terminate the tag early.
#[test]
fn gt_inside_quoted_attribute_does_not_close_tag() {
    let out = render(r#"<span title="1 > 0">ok</span>"#);
    assert!(out.contains("ok"), "tag inner text kept: {out}");
    assert!(!out.contains('"'), "attribute garbage must not leak: {out}");
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
/// without quadratic blowup or panicking.
#[test]
fn unmatched_angle_brackets_do_not_hang() {
    let out = render("<<<<<<<<<<<<<<<<<<<<<<<<text");
    assert!(out.contains("text"), "text survives: {out}");
}

/// An unterminated HTML comment or declaration that comrak routes to the
/// HTML-stripper must consume to end-of-input (NOT loop forever). For
/// free-form text like `before<!-- never closed`, comrak treats the
/// unterminated comment as literal text (so it survives), but the guarantee
/// under test is that rendering ALWAYS terminates — never hangs.
#[test]
fn unterminated_html_comment_does_not_hang() {
    // Free-form input: must terminate (no infinite loop), text survives.
    let out = render("before<!-- never closed");
    assert!(
        out.contains("before"),
        "text before comment survives: {out}"
    );
    // An unterminated declaration must also terminate.
    let out2 = render("x<!DOCTYPE html");
    assert!(
        out2.contains('x'),
        "text before declaration survives: {out2}"
    );
    // A block-level unterminated comment (comrak routes to the HTML stripper)
    // must also terminate, not hang.
    let out3 = render(
        "<!-- never closed
",
    );
    assert!(
        out3.lines().count() < 100,
        "terminated without hang: {out3}"
    );
}
