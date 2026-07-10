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
//! - **v1 ships NO color** (see the theme policy in issue #155): ASCII glyphs
//!   only (`*` bullets, `--` rules, `[x]`/`[ ]` task lists, framed code
//!   blocks). Every color would have to be sourced from theme tokens — never a
//!   literal — and that requires upgrading `DetailContent` to iocraft
//!   `MixedText`, which is a separate follow-up.
//! - No raw angle-brackets or triple-backticks ever reach the screen: HTML
//!   `<details>`/`<summary>` become a toggle/label line, `<a>`/`<img>`/etc.
//!   collapse to their text/alt, and everything else is stripped.

use comrak::nodes::{
    AstNode, ListType, NodeCodeBlock, NodeLink, NodeList, NodeTable, NodeTaskItem, NodeValue,
    TableAlignment,
};
use comrak::{Arena, Options, parse_document};

/// Render a markdown document into a flat list of plain-text lines.
///
/// Each element of the returned vector is one screen line (no trailing
/// newline). Callers join them with `\n` or feed them directly into a
/// `ContentBuilder`. Empty input yields an empty vector.
#[must_use]
pub fn render_markdown_lines(markdown: &str) -> Vec<String> {
    if markdown.is_empty() {
        return Vec::new();
    }
    let arena = Arena::new();
    let opts = gfm_options();
    let root = parse_document(&arena, markdown, &opts);
    let mut renderer = MarkdownRenderer::new();
    renderer.render_block_children(root, 0);
    renderer.finish()
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
        self.lines.push(line.into());
    }

    /// Push a blank separator line, collapsing consecutive blanks so paragraph
    /// breaks stay single.
    fn push_blank(&mut self) {
        if !self.lines.last().is_some_and(String::is_empty) {
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
            NodeValue::Paragraph
            | NodeValue::FootnoteDefinition(_)
            | NodeValue::FootnoteReference(_)
            | NodeValue::Alert(_) => {
                self.render_inline_block(node, indent);
                self.push_blank();
            }
            NodeValue::Heading(_) => self.render_heading(node, indent),
            NodeValue::ThematicBreak => {
                self.push(rule_line(indent));
                self.push_blank();
            }
            NodeValue::List(list) => self.render_list(node, list, indent),
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

    /// Render a blockquote by rendering its children one indent deeper and
    /// prefixing each resulting line with a quote bar.
    fn render_block_quote<'a>(&mut self, node: &'a AstNode<'a>, indent: usize) {
        let quote_indent = indent + 1;
        let start = self.lines.len();
        self.render_block_children(node, quote_indent);
        for line in &mut self.lines[start..] {
            line.insert_str(0, "> ");
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

    /// Render an ordered or unordered list, tracking item ordinals.
    fn render_list<'a>(&mut self, node: &'a AstNode<'a>, list: &NodeList, indent: usize) {
        let mut ordinal = list.start.max(1);
        for item in node.children() {
            match &item.data().value {
                NodeValue::Item(_) | NodeValue::TaskItem(_) => {
                    let marker = Self::list_marker(item, list, &mut ordinal);
                    self.render_list_item(item, &marker, indent);
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
        if let NodeValue::TaskItem(NodeTaskItem { symbol, .. }) = &item.data().value {
            let box_char = symbol.map_or_else(
                || "[ ]",
                |c| {
                    if c.is_whitespace() || c == '\u{0}' {
                        "[ ]"
                    } else {
                        "[x]"
                    }
                },
            );
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
        let marker_pad = marker.len() + 1;
        let mut first = true;
        for child in item.children() {
            let value = &child.data().value;
            match value {
                NodeValue::List(nested) => {
                    // Sub-lists indent one level deeper.
                    self.render_list(child, nested, indent + 1);
                }
                NodeValue::Paragraph => {
                    let collected = self.collect_inline(child);
                    let mut lines = Vec::new();
                    for src in collected {
                        lines.extend(wrap_indent(&src, indent));
                    }
                    if first {
                        if let Some(first_line) = lines.first_mut() {
                            let pad = LIST_INDENT.repeat(indent);
                            *first_line = format!("{pad}{marker} {first_line}");
                        }
                        first = false;
                    } else {
                        for line in &mut lines {
                            let pad = indent_str(indent + marker_pad, line);
                            *line = pad;
                        }
                    }
                    for line in lines {
                        self.push(line);
                    }
                }
                _ => {
                    // Any other block under a list item: render with the
                    // marker's continuation indent so it stays visually nested.
                    self.render_block(child, indent + 1);
                }
            }
        }
        // Tight lists (per GFM) omit the inter-item blank line; loose lists add one.
        let tight = matches!(&item.data().value, NodeValue::Item(n) if n.tight)
            | matches!(&item.data().value, NodeValue::TaskItem(_));
        let _ = tight; // marker-based; spacing handled by paragraph renderer.
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
        let top = format!("{pad}{CODE_FENCE_TOP}{lang_label} ");
        let bottom = format!("{pad}{CODE_FENCE_BOTTOM} ");
        self.push(top);
        for line in code.literal.lines() {
            self.push(format!("{pad}{CODE_FENCE_SIDE} {line}"));
        }
        self.push(bottom);
        self.push_blank();
    }

    /// Render a GFM table as aligned columns.
    fn render_table<'a>(&mut self, node: &'a AstNode<'a>, table: &NodeTable, indent: usize) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut is_header = Vec::new();
        for row in node.children() {
            if !matches!(&row.data().value, NodeValue::TableRow(_)) {
                continue;
            }
            is_header.push(row.data().value == NodeValue::TableRow(true));
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
                let w = cell.chars().count();
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
            // Emit an alignment separator after the header row.
            if ri == 0 && is_header.first().is_some_and(|h| *h) {
                let mut sep = String::from(&pad);
                for (i, _) in row.iter().enumerate().take(num_cols) {
                    let align = table
                        .alignments
                        .get(i)
                        .copied()
                        .unwrap_or(TableAlignment::None);
                    sep.push_str(&dashes(widths[i], align));
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
        let is_toggle = lower.contains("<summary") || lower.contains("<details");
        for line in stripped.split('\n') {
            if line.trim().is_empty() {
                continue;
            }
            if is_toggle {
                self.push(format!("{pad}▶ {}", line.trim()));
            } else {
                self.push(format!("{pad}{line}"));
            }
        }
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
            // Dropped inline content (footnotes, math, raw): emit nothing.
            NodeValue::FootnoteReference(_) | NodeValue::Math(_) | NodeValue::Raw(_) => {}
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

    /// Trim trailing whitespace from every line and drop a trailing empty line.
    fn trim_ends(&mut self) {
        for line in &mut self.prev {
            *line = line.trim_end().to_string();
        }
        self.current = self.current.trim_end().to_string();
        if self.current.is_empty() && !self.prev.is_empty() {
            // Drop the now-empty trailing current line.
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
    const SOFT_WIDTH: usize = 78;
    let pad_len = indent * LIST_INDENT.len();
    let max = SOFT_WIDTH.saturating_sub(pad_len).max(20);
    let pad = " ".repeat(pad_len);
    let mut out = Vec::new();
    for source_line in text.split('\n') {
        if source_line.is_empty() {
            out.push(pad.clone());
            continue;
        }
        let mut current = String::new();
        for word in source_line.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.len() + 1 + word.len() <= max {
                current.push(' ');
                current.push_str(word);
            } else {
                out.push(format!("{pad}{current}"));
                current = word.to_string();
            }
        }
        if !current.is_empty() || out.is_empty() {
            out.push(format!("{pad}{current}"));
        }
    }
    out
}

/// Pad/align a single table cell to the column width.
fn pad_cell(cell: &str, width: usize, align: TableAlignment) -> String {
    let len = cell.chars().count();
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

/// Build the dashed separator for a table column.
fn dashes(width: usize, align: TableAlignment) -> String {
    let min = width.max(3);
    match align {
        TableAlignment::Center => {
            let left = min / 2;
            let right = min - left;
            format!(":{}:{}", "-".repeat(left), "-".repeat(right))
        }
        TableAlignment::Right => format!("{}:", "-".repeat(min)),
        TableAlignment::Left | TableAlignment::None => "-".repeat(min),
    }
}

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
/// `\n` before emitting screen lines (see [`MarkdownRenderer::push_multiline`]).
fn strip_html_to_text(html: &str) -> String {
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
                let ch_len = utf8_len(bytes[i]);
                if let Some(slice) = html.get(i..i + ch_len) {
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
    if html[start..].starts_with("<!--") {
        return start + html[start..].find("-->").map_or(0, |p| p + "-->".len());
    }
    if start + 1 < bytes.len() && bytes[start + 1] == b'!' {
        return start + html[start..].find('>').map_or(0, |p| p + 1);
    }

    // Scan the tag name + attributes, respecting quoted attribute values so a
    // `>` inside quotes does not prematurely close the tag.
    let mut j = start + 1;
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
            c if c.is_ascii_whitespace() && name_end == start + 1 => {
                name_end = j;
            }
            _ => {}
        }
        j += 1;
    }
    if name_end == start + 1 {
        name_end = j;
    }
    let name = html[start + 1..name_end].to_ascii_lowercase();
    if html_tag_introduces_break(&name) {
        out.push('\n');
    }
    // Advance past the closing '>' if one was found.
    j + usize::from(j < bytes.len() && bytes[j] == b'>')
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

/// True for the block-level closing tags and `<br>` whose removal should yield
/// a line break so paragraph/list/row boundaries survive the HTML strip.
fn html_tag_introduces_break(name: &str) -> bool {
    matches!(
        name,
        "br" | "/br" | "/p" | "/li" | "/tr" | "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6"
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
    Some(match entity {
        "&amp;" => "&".to_string(),
        "&lt;" => "<".to_string(),
        "&gt;" => ">".to_string(),
        "&quot;" => "\"".to_string(),
        "&#39;" | "&apos;" => "'".to_string(),
        "&nbsp;" => " ".to_string(),
        "&mdash;" => "—".to_string(),
        "&ndash;" => "–".to_string(),
        "&hellip;" => "…".to_string(),
        "&bullet;" => "•".to_string(),
        "&check;" | "&#10003;" => "✓".to_string(),
        "&times;" => "×".to_string(),
        _ => return None,
    })
}

#[cfg(test)]
#[path = "markdown_render_tests.rs"]
mod tests;
