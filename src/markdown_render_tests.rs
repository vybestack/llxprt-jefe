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
    // Both fence borders must be present and free of trailing whitespace.
    let top = out.lines().find(|l| l.contains(CODE_FENCE_TOP));
    let bottom = out.lines().find(|l| l.contains(CODE_FENCE_BOTTOM));
    assert!(
        top.is_some_and(|l| !l.ends_with(' ')),
        "fence top present without trailing whitespace: {out:?}"
    );
    assert!(
        bottom.is_some_and(|l| !l.ends_with(' ')),
        "fence bottom present without trailing whitespace: {out:?}"
    );
}

#[test]
fn unordered_list_uses_bullets() {
    let out = render("- one\n- two\n");
    assert!(out.contains("* one"), "bullet list: {out}");
    assert!(out.contains("* two"));
}

/// An explicit `0.` start is preserved (valid CommonMark; GitHub renders it
/// starting at zero) instead of being coerced to 1.
#[test]
fn ordered_list_preserves_zero_start() {
    let out = render(
        "0. zero
1. one",
    );
    assert!(out.contains("0. zero"), "zero start preserved: {out}");
    assert!(out.contains("1. one"), "increment from zero: {out}");
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

/// A task item in an ordered list keeps its ordinal so subsequent items number
/// correctly.
#[test]
fn ordered_task_list_increments_ordinal() {
    let out = render("1. [ ] first\n2. [x] second\n3. [ ] third\n");
    assert!(out.contains("1. [ ] first"), "first item: {out}");
    assert!(
        out.contains("2. [x] second"),
        "second item keeps number 2: {out}"
    );
    assert!(
        out.contains("3. [ ] third"),
        "third item keeps number 3: {out}"
    );
}

/// A list item whose first child is not a paragraph (e.g. a code block) still
/// emits its marker so the item is recognizably a list entry.
#[test]
fn list_item_with_code_block_first_child_emits_marker() {
    let out = render("- ```bash\ncargo test\n```\n");
    assert!(
        out.lines().any(|l| l.starts_with('*')),
        "bullet marker emitted before the code block: {out}"
    );
    assert!(out.contains("cargo test"), "code body present: {out}");
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

/// Numeric references to control characters (ESC, NUL, BS, C1 range) never
/// decode: emitting them could corrupt terminal state or smuggle escape
/// sequences from untrusted GitHub content.
#[test]
fn control_character_entities_not_decoded() {
    let out = render("a&#27;b &#x1b;c &#0;d &#8;e &#x9b;f");
    assert!(!out.contains('\u{1b}'), "no ESC in output: {out:?}");
    assert!(!out.contains('\u{0}'), "no NUL in output: {out:?}");
    assert!(!out.contains('\u{8}'), "no BS in output: {out:?}");
    assert!(!out.contains('\u{9b}'), "no C1 CSI in output: {out:?}");
    // The surrounding text still renders.
    for t in ["a", "b", "c", "d", "e", "f"] {
        assert!(out.contains(t), "text {t} preserved: {out:?}");
    }
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

/// Center/right alignment separators mirror the GFM source shape and span
/// the full column width (`:---:` / `---:`), never a misplaced mid-colon.
#[test]
fn table_alignment_separators_match_gfm_shape() {
    let out = render(
        "| a | b | c |
|:-:|--:|---|
| x | y | z |
",
    );
    assert!(
        out.contains(":-:") || out.contains(":--"),
        "center separator has colons on both ends: {out}"
    );
    let Some(sep_line) = out.lines().find(|l| l.contains(':')) else {
        panic!("alignment separator line missing: {out}");
    };
    for part in sep_line.split_whitespace() {
        let inner = part.trim_start_matches(':').trim_end_matches(':');
        assert!(
            inner.chars().all(|c| c == '-'),
            "separator segment is colons around dashes only: {part:?} in {out}"
        );
    }
}

/// A childless list item (bare `-` with nothing after it, no children in the
/// AST) still emits its marker instead of vanishing from the list.
#[test]
fn childless_list_item_emits_marker() {
    let out = render(
        "- first
-
- third",
    );
    let marker_lines = out
        .lines()
        .filter(|l| l.trim_start().starts_with('*'))
        .count();
    assert!(
        marker_lines >= 3,
        "all three items render markers (childless included): {out:?}"
    );
}

#[test]
fn blockquote_is_marked() {
    let out = render("> quoted text");
    assert!(
        out.lines().any(|l| l == "> quoted text"),
        "blockquote must render exactly '> quoted text' (quote bar, no extra indent): {out}"
    );
}

/// Blank separator lines inside/after a blockquote stay truly empty — the
/// quote bar decorates content lines only, so a document ending in a quote
/// trims its trailing blank and multi-paragraph quotes keep `>`-free breaks.
#[test]
fn blockquote_blank_lines_stay_empty() {
    let out = render(
        "> para one
>
> para two",
    );
    assert!(
        !out.lines().any(|l| l.trim() == ">"),
        "no bare '> ' decorated blank lines: {out:?}"
    );
    let Some(last) = out.lines().last() else {
        panic!("blockquote output must not be empty");
    };
    assert!(
        !last.trim().is_empty(),
        "trailing blank after a quote is trimmed: {out:?}"
    );
}

/// A table whose data row has fewer cells than the declared columns still
/// emits an alignment separator spanning all declared columns so the header
/// lines up with the declared width.
#[test]
fn table_separator_spans_declared_columns() {
    // Three columns declared; the (single) body row fills all three so the
    // table parses, and the separator must carry a dash run per column.
    let out = render(
        "| a | b | c |
|---|---|---|
| 1 | 2 | 3 |
",
    );
    let sep_line = out
        .lines()
        .find(|l| l.contains("---"))
        .unwrap_or("<no separator>");
    let dash_runs = sep_line.matches("---").count();
    assert!(
        dash_runs >= 3,
        "separator spans declared columns: {sep_line}"
    );
}

/// A self-closing `<br/>` (no space before the slash) still introduces a line
/// break, the same as `<br>` and `<br />`.
#[test]
fn self_closing_br_introduces_break() {
    let out = render("line one<br/>line two");
    assert!(
        out.contains("line one") && out.contains("line two"),
        "both halves present: {out}"
    );
    assert!(
        !out.contains("line oneline two"),
        "self-closing br must break the line: {out}"
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
    // The line immediately before the following paragraph must be blank so
    // the HTML block never renders glued to trailing content.
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
        prev.is_empty(),
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

/// A multi-paragraph list item keeps its paragraph break: the hard-break
/// blank inside the item must survive wrapping instead of being swallowed
/// (regression: whitespace-only source lines produced zero words and were
/// silently dropped by wrap_indent_cols).
#[test]
fn multi_paragraph_list_item_keeps_paragraph_break() {
    let out = render(
        "- first para

  second para",
    );
    let lines: Vec<&str> = out.lines().collect();
    let Some(first) = lines.iter().position(|l| l.contains("first para")) else {
        panic!("first paragraph present: {out}");
    };
    assert!(
        lines
            .iter()
            .skip(first + 1)
            .any(|l| l.contains("second para")),
        "second paragraph present: {out}"
    );
    let between: Vec<&&str> = lines
        .iter()
        .skip(first + 1)
        .take_while(|l| !l.contains("second para"))
        .collect();
    assert!(
        between.iter().any(|l| l.trim().is_empty()),
        "paragraph break preserved between paragraphs: {out}"
    );
}

/// Closing block-level tags (`</div>`) inside stripped HTML introduce line
/// breaks so adjacent blocks don't fuse into one line.
#[test]
fn html_div_boundaries_break_lines() {
    let out = render("<div>alpha</div><div>beta</div>");
    let alpha = out.lines().position(|l| l.trim() == "alpha");
    let beta = out.lines().position(|l| l.trim() == "beta");
    assert!(
        alpha.is_some() && beta.is_some() && alpha != beta,
        "div contents must land on distinct lines: {out}"
    );
}

/// An item whose first child is a sub-list still emits the parent marker so
/// the parent item never renders markerless.
#[test]
fn item_with_leading_sublist_keeps_parent_marker() {
    // comrak parses the indented dash as a sub-list that is the parent
    // item's first child when there is no leading paragraph text.
    let out = render(
        "-
  - nested",
    );
    let lines: Vec<&str> = out.lines().collect();
    let Some(parent) = lines.iter().position(|l| l.trim() == "*") else {
        panic!("parent marker emitted: {out:?}");
    };
    assert!(
        lines
            .iter()
            .skip(parent + 1)
            .any(|l| l.contains("* nested")),
        "nested item renders under the parent marker: {out:?}"
    );
}

/// Prose merely *mentioning* details/summary-like text must not trigger the
/// toggle glyph; a real `<details>` open tag must.
#[test]
fn details_toggle_requires_real_open_tag() {
    let real = render("<details><summary>More</summary>body</details>");
    assert!(
        real.lines().any(|l| l.trim_start().starts_with('▶')),
        "real <details> gets the toggle glyph: {real:?}"
    );
    let fake = render("<div>&lt;detailsish thing&gt; explained</div>");
    assert!(
        !fake.lines().any(|l| l.trim_start().starts_with('▶')),
        "non-tag mention must not toggle: {fake:?}"
    );
}

/// A wrapped list item's continuation lines must align under the marker's
/// content (NOT double-indent at nesting levels > 0). Regression for the
/// wrap_indent + cont_pad double-prefix bug.
#[test]
fn wrapped_list_continuation_aligns_under_content() {
    // A long first item that exceeds the soft width so it actually wraps; the
    // wrapped tail must align under the text after the "* " marker.
    let md = "- alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi
";
    let out = render(md);
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    let first = lines.first().copied().unwrap_or("<missing>");
    assert!(first.starts_with("* "), "first line has bullet: {out}");
    // There must be a wrapped continuation line carrying a later word.
    let Some(cont_line) = lines
        .iter()
        .find(|l| {
            !l.starts_with('*') && (l.contains("sigma") || l.contains("phi") || l.contains("tau"))
        })
        .copied()
    else {
        panic!("a wrapped continuation line must exist: {out}");
    };
    let cont_lead = cont_line.chars().take_while(|c| *c == ' ').count();
    // Continuation aligns at column 2 (under the text after "* ").
    assert_eq!(
        cont_lead, 2,
        "continuation aligns under marker content: {out}"
    );
}

/// Multibyte text must be measured in display columns (char count), not bytes,
/// so it is not wrapped prematurely. A short CJK run well under the soft width
/// stays on one line.
#[test]
fn multibyte_text_not_wrapped_prematurely() {
    // 9 CJK chars = 27 bytes / 18 display columns; well under width 78.
    let md = "中文测试中文测试中";
    let out = render(md);
    let non_empty = out.lines().filter(|l| !l.is_empty()).count();
    assert_eq!(
        non_empty, 1,
        "short multibyte text stays on one line: {out}"
    );
    assert!(out.contains("中文测试中文测试中"));
}

/// Wrap decisions measure DISPLAY columns (unicode-width), not codepoints:
/// fifteen 3-char CJK words are 45 codepoints but 90 columns (plus gaps), so
/// they must wrap at the soft width instead of staying on one over-wide line.
#[test]
fn wide_chars_wrap_at_display_width() {
    let md = vec!["中文字"; 15].join(" ");
    let out = render(&md);
    let non_empty = out.lines().filter(|l| !l.is_empty()).count();
    assert!(
        non_empty >= 2,
        "15 double-width words (104 cols) must wrap: {out}"
    );
}

/// Table column alignment pads by display width: a header with a wide CJK
/// cell must produce data-row padding that lines the columns up in terminal
/// columns, not codepoints.
#[test]
fn table_columns_align_by_display_width() {
    let md = "| 中文中文 | b |
| --- | --- |
| x | y |";
    let out = render(md);
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    let Some(data_row) = lines.iter().find(|l| l.trim_start().starts_with('x')) else {
        panic!("data row present: {out}");
    };
    // "中文中文" is 8 display columns; the x cell pads to 8 columns
    // (x + 7 spaces) and the two-space column gap follows → 9 spaces total.
    let expected = format!("x{}y", " ".repeat(9));
    assert!(
        data_row.contains(&expected),
        "data cell must pad to the wide header's display width: {data_row:?}"
    );
}

/// A list item whose first paragraph BEGINS with a break tag renders without
/// panicking: the wrapped first line is empty, and the marker splice must not
/// `split_off` past its length (crafted untrusted markdown must never panic).
#[test]
fn list_item_starting_with_break_does_not_panic() {
    for md in ["- <br>after", "- <br><br>x", "1. <br>y"] {
        let out = render(md);
        assert!(
            out.contains("after") || out.contains('x') || out.contains('y'),
            "content after the break renders: {out:?}"
        );
    }
}

/// Consecutive `<br>` tags inside an HTML block render a paragraph gap (one
/// blank line) between the surrounding text instead of being dropped.
#[test]
fn consecutive_br_in_html_block_keeps_paragraph_gap() {
    let out = render("<div>alpha<br><br>beta</div>");
    let lines: Vec<&str> = out.lines().collect();
    let Some(alpha) = lines.iter().position(|l| l.contains("alpha")) else {
        panic!("alpha rendered: {out}");
    };
    let Some(beta) = lines.iter().position(|l| l.contains("beta")) else {
        panic!("beta rendered: {out}");
    };
    assert!(
        lines[alpha + 1..beta].iter().any(|l| l.trim().is_empty()),
        "blank line between alpha and beta: {out}"
    );
}

/// A malformed tag with whitespace after `<` (e.g. `< br>`) still introduces
/// the block boundary its trimmed tag name implies.
#[test]
fn whitespace_after_angle_bracket_still_breaks() {
    let md = "<div>alpha< br>beta</div>";
    let out = render(md);
    let Some(alpha_line) = out.lines().position(|l| l.contains("alpha")) else {
        panic!("alpha rendered: {out}");
    };
    let Some(beta_line) = out.lines().position(|l| l.contains("beta")) else {
        panic!("beta rendered: {out}");
    };
    assert_ne!(
        alpha_line, beta_line,
        "`< br>` must introduce a line break: {out}"
    );
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
/// A declaration carrying a quoted `>` (e.g. `<!DOCTYPE html "foo>bar">`)
/// consumes through its real close instead of truncating at the inner `>`
/// and leaking the quoted remainder as text.
#[test]
fn declaration_with_quoted_gt_fully_consumed() {
    let out = render("<div><!DOCTYPE html \"foo>bar\">text</div>");
    assert!(out.contains("text"), "content after declaration: {out:?}");
    assert!(
        !out.contains("bar"),
        "quoted declaration content must not leak: {out:?}"
    );
}

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

// ── CHANGE 1: raw control characters in strip_html_to_text (issue #155) ──

/// The HTML strip state machine must not pass through literal control
/// characters (ESC, NUL, C1 CSI) from raw text — defense-in-depth so the
/// module's own guarantee holds independently of MarkdownRenderer::push.
#[test]
fn strip_html_drops_raw_control_characters() {
    let out = crate::markdown_html_strip::strip_html_to_text("a\u{1b}[31mred\u{0}b\u{9b}c");
    assert_eq!(out, "a[31mredbc", "ESC/NUL/C1-CSI removed: {out:?}");
    // Newlines (block boundaries) and tabs are meaningful and must survive.
    assert_eq!(
        crate::markdown_html_strip::strip_html_to_text("line1\nline2"),
        "line1\nline2",
        "newlines preserved"
    );
    assert!(
        crate::markdown_html_strip::strip_html_to_text("a\tb").contains('\t'),
        "tabs preserved"
    );
}

// ── CHANGE 2: HTML5 semantic block elements (issue #155) ──────────────

/// Closing tags for HTML5 semantic block elements (`</section>`, `</article>`,
/// …) must introduce a line break so adjacent sections don't fuse.
#[test]
fn html5_semantic_block_tags_break_lines() {
    let out = render("<section>alpha</section><section>beta</section>");
    let alpha = out.lines().position(|l| l.trim() == "alpha");
    let beta = out.lines().position(|l| l.trim() == "beta");
    assert!(
        alpha.is_some() && beta.is_some() && alpha != beta,
        "section contents must land on distinct lines: {out}"
    );
}

// ── CHANGE 3: Unicode bidi control characters in push (issue #155) ────

/// The sanitization chokepoint (`MarkdownRenderer::push`) must strip Unicode
/// bidi override/format characters (Trojan Source attack vectors) while
/// preserving legitimate Format chars like ZWJ used in emoji sequences.
#[test]
fn bidi_control_chars_stripped_zwj_preserved() {
    // Bidi overrides in a paragraph: the rendered output must not contain the
    // bidi chars but must keep the surrounding visible text.
    let out = render("user\u{202E}txt.exe\u{202C}");
    assert!(!out.contains('\u{202E}'), "RLO stripped: {out:?}");
    assert!(!out.contains('\u{202C}'), "PDF stripped: {out:?}");
    assert!(out.contains("user"), "text before bidi preserved: {out:?}");
    assert!(
        out.contains("txt.exe"),
        "text after bidi preserved: {out:?}"
    );

    // Other bidi/format chars from the ban list:
    let out2 = render("\u{2066}data\u{2069}\u{200E}\u{200F}\u{061C}");
    for c in ['\u{2066}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}'] {
        assert!(!out2.contains(c), "bidi char {c:?} stripped: {out2:?}");
    }
    assert!(out2.contains("data"), "text preserved: {out2:?}");

    // ZWJ (U+200D) must survive so emoji sequences are not broken.
    let emoji = "\u{1F469}\u{200D}\u{1F4BB}"; // 👩‍💻
    let out3 = render(emoji);
    assert!(
        out3.contains('\u{200D}'),
        "ZWJ preserved for emoji: {out3:?}"
    );
}

// ── CHANGE A: multiple leading whitespace before tag name (issue #155) ──

/// Multiple spaces between `<` and the tag name must still produce a block
/// boundary. The scanner must skip ALL leading whitespace before the tag
/// name, not just one char (regression: `<  br>` lost its line break).
#[test]
fn multiple_spaces_before_tag_name_still_break() {
    // `<  br>` (two spaces) must introduce a line break.
    assert_eq!(
        crate::markdown_html_strip::strip_html_to_text("a<  br>b"),
        "a\nb",
        "double-space `<  br>` must break"
    );
    // `<  /p>` (two spaces) after text must introduce a newline.
    assert_eq!(
        crate::markdown_html_strip::strip_html_to_text("alpha<  /p>"),
        "alpha\n",
        "double-space `<  /p>` must break"
    );
    // Existing doc case: single spaces `< /p >` must still break.
    assert_eq!(
        crate::markdown_html_strip::strip_html_to_text("alpha< /p >"),
        "alpha\n",
        "single-space `< /p >` must still break"
    );
    // Even with many spaces, the tag name must be found.
    assert_eq!(
        crate::markdown_html_strip::strip_html_to_text("x<    br>y"),
        "x\ny",
        "four-space `<    br>` must break"
    );
}
