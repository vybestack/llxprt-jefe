//! Plain-text markdown rendering for the detail panes (issue #155).
//!
//! The PR and Issue detail screens previously dumped raw markdown verbatim
//! (`## heading`, `**bold**`, `` `code` ``, triple-backtick fences, literal
//! `<details>`/`<summary>` HTML). This module parses markdown with
//! [`comrak`] (the maintained Rust `cmark-gfm` successor, BSD-2-Clause) and
//! walks the AST into jefe's `DetailContent { text: String }` text lines.
//!
//! Design:
//! - comrak parses (with GFM extensions + `render.r#unsafe` so raw HTML is
//!   surfaced as `HtmlBlock`/`HtmlInline` nodes instead of silently dropped);
//!   a thin renderer walks the AST into indented plain-text lines.
//! - **v1 ships NO color** (see the theme policy in issue #155): glyphs are
//!   plain text only (`*` bullets, `--`/`─` rules, `[x]`/`[ ]` task lists,
//!   rule-framed code blocks using the box-drawing chars `┌└│─` that the rest
//!   of the detail UI already uses, and a `▶` toggle for `<details>`).
//!   "No color" is the actual constraint — non-ASCII box-drawing glyphs match
//!   the existing project convention (separators and `│` gutters are used
//!   throughout the detail panes). Every color would have to be sourced from
//!   theme tokens — never a literal — and that requires upgrading
//!   `DetailContent` to iocraft `MixedText`, which is a separate follow-up.
//! - No raw angle-brackets or triple-backticks ever reach the screen: HTML
//!   `<details>`/`<summary>` become a toggle/label line, `<a>`/`<img>`/etc.
//!   collapse to their text/alt, and everything else is stripped.

use comrak::nodes::{
    AstNode, ListType, NodeCodeBlock, NodeLink, NodeList, NodeTable, NodeTaskItem, NodeValue,
    TableAlignment,
};
use comrak::{Arena, Options, parse_document};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::markdown_html_strip::{contains_open_tag, strip_html_to_text};

/// Render a markdown document into a flat list of plain-text lines.
///
/// Each element of the returned vector is one screen line (no trailing
/// newline). Callers join them with `\n` or feed them directly into a
/// `ContentBuilder`. Empty input yields an empty vector.
#[must_use]
pub fn render_markdown_lines(markdown: &str) -> Vec<String> {
    // Short-circuit on empty OR whitespace-only input: comrak parses the latter
    // into blank-line paragraphs, so without this guard callers would see a
    // non-empty vector of blank lines instead of nothing (breaking the
    // "(no body)" / "(no description)" placeholders).
    if markdown.trim().is_empty() {
        return Vec::new();
    }
    let arena = Arena::new();
    let opts = gfm_options();
    let root = parse_document(&arena, markdown, &opts);
    let mut renderer = MarkdownRenderer::new();
    renderer.render_block_children(root, 0);
    renderer.finish()
}

/// Render a markdown body into indented screen lines with a placeholder
/// fallback.
///
/// This is the shared shape every detail-content builder uses (issue #155):
/// blank rendered lines stay truly empty (no indent-only whitespace rows),
/// non-blank lines get `prefix`, and a body that renders to nothing (empty,
/// whitespace-only, or stripped-to-nothing such as a lone HTML comment)
/// yields exactly one `{prefix}{placeholder}` line so the section is never a
/// silent gap. Centralized here so the PR and Issue builders cannot drift.
#[must_use]
pub fn render_markdown_block(markdown: &str, prefix: &str, placeholder: &str) -> Vec<String> {
    let rendered = render_markdown_lines(markdown);
    if rendered.is_empty() {
        return vec![format!("{prefix}{placeholder}")];
    }
    rendered
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                line
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect()
}

/// Build the comrak options used everywhere in jefe: GFM extensions
/// (strikethrough, tables, task lists, autolinks, footnotes) plus
/// `render.r#unsafe` so raw-HTML nodes appear in the AST and can be converted
/// to text rather than dropped.
fn gfm_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.extension.superscript = true;
    // Surface raw HTML as nodes so the renderer can strip/convert it instead
    // of comrak silently emitting "<!-- raw HTML omitted -->".
    opts.render.r#unsafe = true;
    opts
}

/// Width of the ASCII rule drawn under headings and section labels.
const HEADING_RULE_WIDTH: usize = 40;
/// Glyph used for the bullet of an unordered list item.
const BULLET: &str = "*";
/// Characters used to draw the frame around a fenced code block.
const CODE_FENCE_TOP: &str = "┌";
const CODE_FENCE_BOTTOM: &str = "└";
const CODE_FENCE_SIDE: &str = "│";
const CODE_FENCE_H: char = '─';

/// Indentation applied per nested list level (two spaces).
const LIST_INDENT: &str = "  ";

/// Characters that must never reach the terminal screen: ASCII/C1 control
/// characters (except tab), Unicode bidi override/format characters usable
/// for Trojan Source display spoofing, and the invisible-text spoofing pair
/// U+200B (zero-width space) / U+FEFF (BOM / zero-width no-break space) —
/// neither is needed for emoji sequences. Legitimate Format chars like ZWJ
/// (U+200D), ZWNJ (U+200C), and variation selectors are NOT banned so emoji
/// sequences survive intact.
fn banned_on_screen(c: char) -> bool {
    (c.is_control() && c != '\t')
        || matches!(
            c,
            '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{061C}'
                | '\u{200B}'
                | '\u{FEFF}'
        )
}

/// Accumulating plain-text markdown renderer.
struct MarkdownRenderer {
    lines: Vec<String>,
}

impl MarkdownRenderer {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Return the rendered lines, collapsing any accidentally-doubled blank
    /// lines down to one.
    fn finish(mut self) -> Vec<String> {
        // Drop a trailing blank line if present so sections compose cleanly
        // when joined with separators.
        if self.lines.last().is_some_and(String::is_empty) {
            self.lines.pop();
        }
        self.lines
    }

    fn push(&mut self, line: impl Into<String>) {
        let line = line.into();
        // Central sanitization chokepoint: every emitted screen line passes
        // through here, so control characters (ESC/CSI/NUL/BS…) and Unicode
        // bidi override/format chars (Trojan Source display-spoofing vectors)
        // can never reach the terminal regardless of their source — comrak
        // decodes numeric entities like &#27; in ordinary text nodes itself,
        // and code blocks/raw HTML carry author bytes verbatim. Untrusted
        // GitHub content must not be able to smuggle escape sequences or
        // bidi overrides. Tab is kept (meaningful in code blocks, benign on
        // screen). ZWJ/ZWNJ and variation selectors are kept so emoji
        // sequences survive.
        if line.chars().any(banned_on_screen) {
            self.lines
                .push(line.chars().filter(|c| !banned_on_screen(*c)).collect());
        } else {
            self.lines.push(line);
        }
    }

    /// Push a blank separator line, collapsing consecutive blanks so paragraph
    /// breaks stay single. A blank never OPENS the document either — a
    /// separator before any content separates nothing (e.g. a leading
    /// HTML-comment block that strips to nothing must not leave a stray
    /// blank first line).
    fn push_blank(&mut self) {
        if !self.lines.is_empty() && !self.lines.last().is_some_and(String::is_empty) {
            self.lines.push(String::new());
        }
    }

    /// Render the block children of `node` at the given indent depth.
    fn render_block_children<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        for child in node.children() {
            self.render_block(child, indent);
        }
    }

    /// Render a single block-level node.
    fn render_block<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        let value = &node.data().value;
        match value {
            // Paragraphs share the unknown-block shape: render inline text
            // ONLY when non-empty (a paragraph that strips to nothing — e.g.
            // emphasis around an HTML comment — must not leave a stray blank
            // line), then a trailing blank separator.
            NodeValue::Paragraph | NodeValue::FootnoteReference(_) => {
                self.render_unknown_block(node, indent);
            }
            // Footnote definitions contain nested BLOCK content (lists,
            // multiple paragraphs, code blocks), so render their children as
            // blocks. Emit a `[^name]:` label line first so the reader sees
            // the footnote's identity before its body.
            NodeValue::FootnoteDefinition(def) => {
                self.push(indent_str(indent, &format!("[^{}]:", def.name)));
                self.render_block_children(node, indent);
                self.push_blank();
            }
            // GFM alerts contain nested BLOCK content, so render their
            // children as blocks instead of flattening to inline text.
            // Alert is currently unreachable — the alerts extension is off,
            // so GitHub `> [!NOTE]` syntax parses as a plain blockquote —
            // but routed correctly in case the extension is ever enabled.
            NodeValue::Alert(_) => {
                self.render_block_children(node, indent);
                self.push_blank();
            }
            NodeValue::Heading(_) => self.render_heading(node, indent),
            NodeValue::ThematicBreak => {
                self.push(rule_line(indent));
                self.push_blank();
            }
            NodeValue::List(list) => {
                self.render_list(node, list, indent);
                self.push_blank();
            }
            NodeValue::CodeBlock(code) => self.render_code_block(code, indent),
            NodeValue::BlockQuote => self.render_block_quote(node, indent),
            NodeValue::HtmlBlock(html) => self.render_html_block(&html.literal, indent),
            NodeValue::Table(table) => self.render_table(node, table, indent),
            _ => self.render_unknown_block(node, indent),
        }
    }

    /// Render a heading as its text plus a trailing rule (issue #155).
    fn render_heading<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        let lines = self.collect_inline(node);
        // Plain heading label (no raw `#` markers) + a trailing rule; the rule
        // alone signals the heading (issue #155: markdown syntax must not reach
        // the screen).
        let heading = lines.join(" ");
        self.push(indent_str(indent, heading.trim()));
        self.push(rule_line(indent));
        self.push_blank();
    }

    /// Render a blockquote by rendering its children at the same indent and
    /// prefixing each resulting line with a quote bar. The `"> "` prefix alone
    /// marks the quote level; adding an indent level on top would double-pad
    /// (`">   text"` instead of `"> text"`).
    fn render_block_quote<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        let start = self.lines.len();
        self.render_block_children(node, indent);
        for line in &mut self.lines[start..] {
            // Decorate content lines only: blank separators stay truly empty
            // (the "blank lines stay empty" contract), so trailing-blank
            // trimming and paragraph breaks keep working through a quote.
            if !line.is_empty() {
                line.insert_str(0, "> ");
            }
        }
    }

    /// Render the inline children of `node` as wrapped, indented text lines.
    fn render_inline_block<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        for src in self.collect_inline(node) {
            for line in wrap_indent(&src, indent) {
                self.push(line);
            }
        }
    }

    /// Fallback for unrecognized block nodes: render their inline text only
    /// when it is non-empty (avoids pushing blank paragraphs).
    fn render_unknown_block<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        let lines = self.collect_inline(node);
        if lines.iter().any(|l| !l.trim().is_empty()) {
            for src in lines {
                for line in wrap_indent(&src, indent) {
                    self.push(line);
                }
            }
            self.push_blank();
        }
    }

    /// Render an ordered or unordered list, tracking item ordinals. The
    /// author's explicit start value is preserved as-is — `0.` lists are
    /// valid CommonMark and GitHub renders them starting at zero (the
    /// ordinal is unused for unordered lists).
    fn render_list<'a>(&mut self, node: &'a AstNode<'a>, list: &NodeList, indent: usize) {
        let mut ordinal = list.start;
        let mut rendered_any = false;
        for item in node.children() {
            match &item.data().value {
                NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                    let marker = Self::list_marker(item, list, &mut ordinal);
                    // Loose lists separate items with a blank line; tight lists
                    // (per GFM) do not. Only emit a separator BETWEEN items.
                    if rendered_any && !list.tight {
                        self.push_blank();
                    }
                    self.render_list_item(item, &marker, indent);
                    rendered_any = true;
                }
                // A nested list nested directly (rare) — render its block form.
                NodeValue::List(nested) => self.render_list(item, nested, indent + 1),
                _ => self.render_block(item, indent),
            }
        }
    }

    /// Build the marker string (`*`, `1.`, `[x]`) for a list item and advance
    /// the ordinal for ordered lists.
    fn list_marker<'a>(item: &'a AstNode<'a>, list: &NodeList, ordinal: &mut usize) -> String {
        // A task item in an ordered list still consumes an ordinal slot so
        // subsequent items keep their correct numbers.
        if let NodeValue::TaskItem(NodeTaskItem { symbol, .. }) = &item.data().value {
            let box_char = symbol.map_or_else(
                || "[ ]",
                |c| {
                    if c.eq_ignore_ascii_case(&'x') {
                        "[x]"
                    } else {
                        "[ ]"
                    }
                },
            );
            if matches!(list.list_type, ListType::Ordered) {
                let m = format!("{ordinal}.");
                *ordinal += 1;
                return format!("{m} {box_char}");
            }
            return box_char.to_string();
        }
        match list.list_type {
            ListType::Ordered => {
                let m = format!("{ordinal}.");
                *ordinal += 1;
                m
            }
            ListType::Bullet => BULLET.to_string(),
        }
    }

    /// Render a single list item: first line carries the marker, nested blocks
    /// and sub-lists are indented under it.
    fn render_list_item<'a>(&mut self, item: &'a AstNode<'a>, marker: &str, indent: usize) {
        // Continuation lines align under the first line's content, which starts
        // after the marker and one space. This is a raw column count (NOT
        // indent levels) and is passed to wrap_indent as the target pad so the
        // wrapper produces the full prefix in one step (previously wrap_indent
        // added its own indent on top of cont_pad, double-indenting wrapped
        // continuation lines at nesting levels > 0).
        let cont_cols = indent * LIST_INDENT.len() + marker.len() + 1;
        let mut first = true;
        for child in item.children() {
            let value = &child.data().value;
            match value {
                NodeValue::List(nested) => {
                    // An item whose FIRST child is a sub-list still needs its
                    // own marker emitted, or the parent renders markerless.
                    if first {
                        let pad = LIST_INDENT.repeat(indent);
                        self.push(format!("{pad}{marker}"));
                        first = false;
                    }
                    // Sub-lists indent one level deeper.
                    self.render_list(child, nested, indent + 1);
                }
                NodeValue::Paragraph => {
                    if !first {
                        // A subsequent paragraph inside the same list item
                        // keeps its paragraph break (otherwise multi-paragraph
                        // items render visually fused).
                        self.push_blank();
                    }
                    let collected = self.collect_inline(child);
                    let mut lines = Vec::new();
                    for src in collected {
                        lines.extend(wrap_indent_cols(&src, cont_cols));
                    }
                    if first {
                        let pad = LIST_INDENT.repeat(indent);
                        if let Some(first_line) = lines.first_mut() {
                            // wrap_indent_cols already prefixed every line
                            // with `cont_cols` spaces; replace that prefix on
                            // the first line with the list indent + marker.
                            // The first line can be SHORTER than the prefix
                            // (a leading break tag yields an empty paragraph
                            // line), so clamp the split to its length —
                            // split_off past the end panics on crafted input.
                            let rest = first_line.split_off(cont_cols.min(first_line.len()));
                            // When rest is empty (leading break tag yields an
                            // empty first line), emit just the marker without
                            // a trailing space.
                            if rest.is_empty() {
                                *first_line = format!("{pad}{marker}");
                            } else {
                                *first_line = format!("{pad}{marker} {rest}");
                            }
                        } else {
                            // First paragraph rendered to nothing (e.g. only
                            // an HTML comment): still emit the bare marker so
                            // the item stays recognizably a list entry.
                            self.push(format!("{pad}{marker}"));
                        }
                        first = false;
                    }
                    for line in lines {
                        self.push(line);
                    }
                }
                _ => {
                    // A non-Paragraph first block (e.g. a code block or
                    // blockquote) still needs the marker emitted so the item
                    // is recognizably a list entry.
                    if first {
                        let pad = LIST_INDENT.repeat(indent);
                        self.push(format!("{pad}{marker}"));
                        first = false;
                    }
                    // Render the block with the marker's continuation indent so
                    // it stays visually nested.
                    self.render_block(child, indent + 1);
                }
            }
        }
        if first {
            // A childless item (e.g. a bare `-` line) still emits its marker
            // so the entry stays visible instead of vanishing from the list.
            let pad = LIST_INDENT.repeat(indent);
            self.push(format!("{pad}{marker}"));
        }
    }

    /// Render a fenced/indented code block as a rule-framed box.
    fn render_code_block(&mut self, code: &NodeCodeBlock, indent: usize) {
        let pad = indent_str(indent, "");
        let info = code.info.trim();
        let lang_label = if info.is_empty() {
            String::new()
        } else {
            format!(" {info}")
        };
        let top = format!("{pad}{CODE_FENCE_TOP}{lang_label}");
        let bottom = format!("{pad}{CODE_FENCE_BOTTOM}");
        self.push(top.trim_end().to_string());
        for line in code.literal.lines() {
            self.push(format!("{pad}{CODE_FENCE_SIDE} {line}"));
        }
        self.push(bottom.trim_end().to_string());
        self.push_blank();
    }

    /// Render a GFM table as aligned columns.
    fn render_table<'a>(&mut self, node: &'a AstNode<'a>, table: &NodeTable, indent: usize) {
        // Whether the first TableRow is a header (only `.first()` of the
        // collected bools was ever read, so a single bool suffices).
        let first_row_is_header = node
            .children()
            .find(|row| matches!(&row.data().value, NodeValue::TableRow(_)))
            .is_some_and(|row| row.data().value == NodeValue::TableRow(true));
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in node.children() {
            if !matches!(&row.data().value, NodeValue::TableRow(_)) {
                continue;
            }
            let mut cells = Vec::new();
            for cell in row.children() {
                if matches!(&cell.data().value, NodeValue::TableCell) {
                    let collected = self.collect_inline(cell);
                    cells.push(collected.join(" "));
                }
            }
            rows.push(cells);
        }
        if rows.is_empty() {
            return;
        }
        let num_cols = table.num_columns.max(1);
        // Compute per-column display widths.
        let mut widths = vec![0usize; num_cols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate().take(num_cols) {
                let w = cell.width();
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
        let pad = indent_str(indent, "");
        for (ri, row) in rows.iter().enumerate() {
            let mut out = String::from(&pad);
            for (i, cell) in row.iter().enumerate().take(num_cols) {
                let align = table
                    .alignments
                    .get(i)
                    .copied()
                    .unwrap_or(TableAlignment::None);
                out.push_str(&pad_cell(cell, widths[i], align));
                out.push_str("  ");
            }
            self.push(out.trim_end().to_string());
            // Emit an alignment separator after the header row. Iterate the
            // declared column count (not the header row's cells) so a sparse
            // header still aligns with wider data rows.
            if ri == 0 && first_row_is_header {
                let mut sep = String::from(&pad);
                for (i, w) in widths.iter().enumerate().take(num_cols) {
                    let align = table
                        .alignments
                        .get(i)
                        .copied()
                        .unwrap_or(TableAlignment::None);
                    sep.push_str(&dashes(*w, align));
                    sep.push_str("  ");
                }
                self.push(sep.trim_end().to_string());
            }
        }
        self.push_blank();
    }

    /// Render a raw-HTML block. `<details>`/`<summary>` collapse to a toggle
    /// label; everything else is reduced to its visible text. The stripped
    /// text is split on newlines (block-boundary tags produce line breaks) so
    /// the one-element-per-screen-line invariant holds.
    fn render_html_block(&mut self, literal: &str, indent: usize) {
        let pad = indent_str(indent, "");
        let stripped = strip_html_to_text(literal);
        let lower = literal.to_ascii_lowercase();
        let is_toggle =
            contains_open_tag(&lower, "<summary") || contains_open_tag(&lower, "<details");
        // Only the first rendered line of a <details> block is the summary, so
        // only it gets the toggle glyph; subsequent lines render as plain text.
        let mut first_toggle_line = is_toggle;
        for line in stripped.split('\n') {
            if line.trim().is_empty() {
                // Consecutive break tags (`<br><br>`) yield empty split
                // lines: preserve them as ONE blank (a paragraph gap, the
                // GitHub-rendered effect) instead of dropping them entirely.
                // push_blank collapses runs and never opens a block with a
                // stray blank (the previous block's trailing blank absorbs
                // it).
                self.push_blank();
                continue;
            }
            if first_toggle_line {
                self.push(format!("{pad}▶ {}", line.trim()));
                first_toggle_line = false;
            } else {
                self.push(format!("{pad}{}", line.trim()));
            }
        }
        // Match the trailing blank separator every other block renderer emits
        // so content following an HTML block composes cleanly.
        self.push_blank();
    }

    /// Collect the inline children of `node` into trimmed text lines,
    /// stripping markdown emphasis and converting HTML/links/code. Soft and
    /// hard line breaks start a new output line so the author's line
    /// structure is preserved (GitHub bodies keep explicit line breaks).
    fn collect_inline<'a>(&mut self, node: &'a AstNode<'a>) -> Vec<String> {
        let mut lines = InlineLines::new();
        for child in node.children() {
            self.render_inline_lines(child, &mut lines);
        }
        // Trim trailing whitespace on each line; drop a single trailing empty
        // line if the content ended on a break.
        lines.trim_ends();
        lines.into_lines()
    }

    /// Render a single inline node, appending plain text to the current line
    /// (`lines.current()`) and starting a new line on soft/hard breaks.
    fn render_inline_lines<'a>(&mut self, node: &'a AstNode<'a>, lines: &mut InlineLines) {
        let value = &node.data().value;
        match value {
            NodeValue::Text(t) => lines.push_str(t.as_ref()),
            NodeValue::Code(code) => lines.push_str(code.literal.as_str()),
            NodeValue::SoftBreak | NodeValue::LineBreak => lines.new_line(),
            // Footnote reference: render as a visible marker so the reader
            // can see the note linkage (comrak strips the brackets otherwise).
            NodeValue::FootnoteReference(fr) => {
                lines.push_str(&format!("[^{}]", fr.name));
            }
            // Dropped inline content (math, raw): emit nothing.
            NodeValue::Math(_) | NodeValue::Raw(_) => {}
            // Link: render text + URL via its dedicated helper.
            NodeValue::Link(link) => self.render_link_lines(link, node, lines),
            // Image: keep the alt text, drop the image itself.
            NodeValue::Image(link) => {
                for child in node.children() {
                    self.render_inline_lines(child, lines);
                }
                let _ = link;
            }
            NodeValue::HtmlInline(html) => lines.push_str(&strip_html_to_text(html)),
            // Everything else (emphasis, escaped, and the catch-all) recurses
            // into the children so nested inline content still renders.
            _ => {
                for child in node.children() {
                    self.render_inline_lines(child, lines);
                }
            }
        }
    }

    /// Render a link into the current line buffer: show its text, and append
    /// the URL when it differs from the visible text (so destinations are not
    /// lost).
    fn render_link_lines<'a>(
        &mut self,
        link: &NodeLink,
        node: &'a AstNode<'a>,
        lines: &mut InlineLines,
    ) {
        let mut text = InlineLines::new();
        for child in node.children() {
            self.render_inline_lines(child, &mut text);
        }
        let text_str = text.join();
        if !text_str.is_empty() {
            lines.push_str(&text_str);
        }
        let url = link.url.trim();
        if !url.is_empty() && url != text_str {
            if !lines.current().is_empty() && !lines.current().ends_with(' ') {
                lines.push_str(" ");
            }
            lines.push_str(url);
        }
    }
}

/// Accumulator for inline-rendered text lines, used to preserve the author's
/// soft/hard line breaks across the AST walk. The current line is held
/// separately so appends never need an `Option` unwrap, and the whole is a
/// newtype so the recursive renderer takes `&mut InlineLines` (a `push`-bearing
/// owner) without tripping `clippy::ptr_arg` on a raw `&mut Vec`.
struct InlineLines {
    /// Completed lines before the current one.
    prev: Vec<String>,
    /// The line currently being appended to.
    current: String,
}

impl InlineLines {
    fn new() -> Self {
        Self {
            prev: Vec::new(),
            current: String::new(),
        }
    }

    /// Append text to the current line. Any embedded newline (e.g. from
    /// stripping an inline `<br>`/`</p>`) flushes the current line and starts
    /// a new one, preserving the one-element-per-screen-line invariant.
    fn push_str(&mut self, s: &str) {
        let mut iter = s.split('\n');
        if let Some(first) = iter.next() {
            self.current.push_str(first);
        }
        for piece in iter {
            self.new_line();
            self.current.push_str(piece);
        }
    }

    /// Start a new (empty) line for the next inline content.
    fn new_line(&mut self) {
        let next = String::new();
        let prev = std::mem::replace(&mut self.current, next);
        self.prev.push(prev);
    }

    /// The current line being appended to.
    fn current(&self) -> &str {
        &self.current
    }

    /// Trim trailing whitespace from every line and drop all consecutive
    /// trailing empty lines.
    fn trim_ends(&mut self) {
        for line in &mut self.prev {
            *line = line.trim_end().to_string();
        }
        self.current = self.current.trim_end().to_string();
        // Drop ALL trailing empty lines (consecutive trailing breaks produce
        // several), not just one, so no stray blank separators leak out.
        while self.current.is_empty() && !self.prev.is_empty() {
            self.current = self.prev.pop().unwrap_or_default();
        }
    }

    /// Join all lines with single spaces (used when collapsing a link's alt
    /// text or a table cell into one string).
    fn join(&self) -> String {
        let mut out = self.prev.join(" ");
        if !out.is_empty() && !self.current.is_empty() {
            out.push(' ');
        }
        out.push_str(&self.current);
        out
    }

    /// Consume into the underlying line vector (prev ++ current).
    fn into_lines(self) -> Vec<String> {
        let mut lines = self.prev;
        lines.push(self.current);
        lines
    }
}

/// Build a horizontal rule line, indented by `indent` levels.
fn rule_line(indent: usize) -> String {
    let pad = LIST_INDENT.repeat(indent);
    let dashes = CODE_FENCE_H.to_string().repeat(HEADING_RULE_WIDTH);
    format!("{pad}{dashes}")
}

/// Prefix a line with `indent` levels of list indentation. Returns the
/// indented line.
fn indent_str(indent: usize, content: &str) -> String {
    let prefix = LIST_INDENT.repeat(indent);
    format!("{prefix}{content}")
}

/// Word-wrap a single logical line of text to a soft width and prefix each
/// output line with `indent` levels of indentation. The terminal renderer
/// (`ScrollableText`) hard-truncates over-wide lines, so wrapping here only
/// improves readability and never breaks scroll math.
fn wrap_indent(text: &str, indent: usize) -> Vec<String> {
    wrap_indent_cols(text, indent * LIST_INDENT.len())
}

/// Word-wrap a single logical line of text to a soft width and prefix each
/// output line with `pad_cols` raw space columns (used by list rendering,
/// which aligns continuation content under the marker at a raw column count
/// rather than an indent level).
fn wrap_indent_cols(text: &str, pad_cols: usize) -> Vec<String> {
    const SOFT_WIDTH: usize = 78;
    let max = SOFT_WIDTH.saturating_sub(pad_cols).max(20);
    let pad = " ".repeat(pad_cols);
    let mut out = Vec::new();
    for source_line in text.split('\n') {
        // Empty AND whitespace-only source lines preserve the paragraph break
        // (previously whitespace-only lines produced zero words and were
        // silently swallowed, collapsing paragraph spacing inside list items).
        // The break line is TRULY empty — padding it would emit an invisible
        // whitespace-only line that downstream indent helpers would then
        // prefix into visible stray whitespace.
        if source_line.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in source_line.split_whitespace() {
            // Compare in terminal display columns (unicode-width), not bytes
            // or codepoints, so CJK/emoji (2 columns each) wrap correctly.
            let current_cols = current.width();
            let word_cols = word.width();
            if word_cols > max {
                // Character-level fallback: a single whitespace-free word
                // (long CJK run, long URL) wider than `max` must be broken
                // into chunks of at most `max` display columns so the
                // downstream hard-truncator never fires.
                if !current.is_empty() {
                    out.push(format!("{pad}{current}"));
                    current.clear();
                }
                let chunks: Vec<String> = chunk_word(word, max);
                for (i, chunk) in chunks.iter().enumerate() {
                    if i + 1 < chunks.len() {
                        out.push(format!("{pad}{chunk}"));
                    } else {
                        current.clone_from(chunk);
                    }
                }
            } else if current.is_empty() {
                current.push_str(word);
            } else if current_cols + 1 + word_cols <= max {
                current.push(' ');
                current.push_str(word);
            } else {
                out.push(format!("{pad}{current}"));
                current = word.to_string();
            }
        }
        // Defensive: the trim().is_empty() guard above means every non-blank
        // source line yields at least one word, so `current` is normally
        // non-empty here. The `out.is_empty()` arm merely guarantees the
        // function never returns an empty vec for non-empty input.
        if !current.is_empty() || out.is_empty() {
            out.push(format!("{pad}{current}"));
        }
    }
    out
}

/// Break a whitespace-free word into chunks of at most `max` display columns,
/// measuring each character's width with `UnicodeWidthChar`. Never splits a
/// character (works on grapheme-ish boundaries = char boundaries). A zero-
/// width control char contributes 0 columns so it never starts a chunk alone;
/// the loop tracks accumulated width and flushes when adding the next char
/// would exceed `max`.
fn chunk_word(word: &str, max: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut buf = String::new();
    let mut cols = 0usize;
    for ch in word.chars() {
        let w = ch.width().unwrap_or(0);
        if cols + w > max && !buf.is_empty() {
            chunks.push(std::mem::take(&mut buf));
            cols = 0;
        }
        buf.push(ch);
        cols += w;
    }
    if !buf.is_empty() || chunks.is_empty() {
        chunks.push(buf);
    }
    chunks
}

/// Pad/align a single table cell to the column width (terminal display
/// columns via unicode-width, so wide CJK/emoji cells align correctly).
fn pad_cell(cell: &str, width: usize, align: TableAlignment) -> String {
    let len = cell.width();
    if len >= width {
        return cell.to_string();
    }
    let pad = width - len;
    match align {
        TableAlignment::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
        }
        TableAlignment::Right => format!("{}{}", " ".repeat(pad), cell),
        TableAlignment::Left | TableAlignment::None => format!("{}{}", cell, " ".repeat(pad)),
    }
}

/// Build the dashed separator for a table column. Every alignment spans
/// exactly `max(width, 3)` display columns and mirrors the GFM source shape
/// (`:---:` center, `---:` right, `---` left/none) so the separator reads as
/// the alignment it declares.
fn dashes(width: usize, align: TableAlignment) -> String {
    let min = width.max(3);
    match align {
        TableAlignment::Center => {
            format!(":{}:", "-".repeat(min.saturating_sub(2)))
        }
        TableAlignment::Right => format!("{}:", "-".repeat(min.saturating_sub(1))),
        TableAlignment::Left | TableAlignment::None => "-".repeat(min),
    }
}

#[cfg(test)]
#[path = "markdown_render_tests.rs"]
mod tests;
