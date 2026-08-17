//! Lossless settings document retaining original bytes and a semantic overlay.

use crate::domain::{ByteSpan, Id, OwnerCatalog, OwnerKind};

use super::diagnostic::{
    ARRAY_LIMIT, CfgCode, Diagnostic, DiagnosticPath, FILE_LIMIT, MAP_LIMIT, NESTING_LIMIT,
    STRING_LIMIT, Severity,
};
use super::settings_publish::publish;
use super::settings_syntax::{SyntaxNode, SyntaxOverlay};
use super::sha256::Sha256;

pub use super::settings_publish::{
    DormantSettings, PublishedAppearance, PublishedOwner, PublishedSettings,
    PublishedWorkbenchSettings,
};

/// Parsed settings document whose original bytes are the formatting authority.
#[derive(Debug, Clone)]
pub struct SettingsDocument {
    original: Vec<u8>,
    sha256: Sha256,
    semantic: toml::Value,
    syntax: SyntaxOverlay,
}

impl SettingsDocument {
    /// Parse one bounded TOML document without performing I/O or rewriting bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, Box<Diagnostic>> {
        if bytes.len() > FILE_LIMIT {
            return Err(limit_diagnostic(
                ByteSpan::new(0, bytes.len() as u64),
                "settings document exceeds the file limit",
            ));
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            syntax_diagnostic(
                Some(ByteSpan::new(
                    error.valid_up_to() as u64,
                    bytes.len() as u64,
                )),
                "settings document is not UTF-8",
            )
        })?;
        let semantic = text.parse::<toml::Value>().map_err(|error| {
            let span = error
                .span()
                .map(|range| ByteSpan::new(range.start as u64, range.end as u64));
            syntax_diagnostic(span, "settings document has invalid TOML syntax")
        })?;
        validate_value(&semantic, 1, "/")?;
        let syntax = super::settings_syntax::scan(bytes)
            .map_err(|error| syntax_diagnostic(error.span, &error.detail))?;
        Ok(Self {
            original: bytes.to_vec(),
            sha256: Sha256::digest(bytes),
            semantic,
            syntax,
        })
    }

    /// Borrow the exact source bytes.
    #[must_use]
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }

    /// Return the digest of the exact source bytes.
    #[must_use]
    pub const fn sha256(&self) -> Sha256 {
        self.sha256
    }

    /// Find an assignment by its decoded dotted path.
    #[must_use]
    pub fn node(&self, path: &[&str]) -> Option<&SyntaxNode> {
        self.syntax.nodes.iter().find(|node| {
            node.path.len() == path.len()
                && node
                    .path
                    .iter()
                    .zip(path)
                    .all(|(left, right)| left == right)
        })
    }

    /// Return exact source bytes covered by a valid parser-produced span.
    #[must_use]
    pub fn span_bytes(&self, span: ByteSpan) -> &[u8] {
        let Ok(start) = usize::try_from(span.start) else {
            return &[];
        };
        let Ok(end) = usize::try_from(span.end) else {
            return &[];
        };
        self.original.get(start..end).unwrap_or_default()
    }

    /// Borrow comment spans in source order.
    #[must_use]
    pub fn comment_spans(&self) -> &[ByteSpan] {
        &self.syntax.comments
    }

    /// Publish only active known owners into the closed typed settings model.
    pub fn publish(&self, catalog: &OwnerCatalog) -> Result<PublishedSettings, Vec<Diagnostic>> {
        publish(self, catalog)
    }

    /// Canonicalize active owned assignments while preserving every other byte.
    pub fn format_owned(&self, catalog: &OwnerCatalog) -> Result<Vec<u8>, Vec<Diagnostic>> {
        self.publish(catalog)?;
        let mut patches = Vec::new();
        for node in &self.syntax.nodes {
            if owned_assignment(&node.path, self.semantic(), catalog) {
                let Some(value) = value_at_path(self.semantic(), &node.path) else {
                    continue;
                };
                let key = trim_ascii(self.span_bytes(node.key_span));
                let mut replacement = Vec::with_capacity(key.len() + 3 + value.to_string().len());
                replacement.extend_from_slice(key);
                replacement.extend_from_slice(b" = ");
                replacement.extend_from_slice(value.to_string().as_bytes());
                patches.push((
                    ByteSpan::new(node.key_span.start, node.value_span.end),
                    replacement,
                ));
            }
        }
        let candidate = apply_patches(&self.original, patches);
        let validated = Self::parse(&candidate).map_err(|diagnostic| vec![*diagnostic])?;
        validated.publish(catalog)?;
        Ok(candidate)
    }

    /// Borrow the semantic TOML tree used by the closed settings publisher.
    #[must_use]
    pub(super) fn semantic(&self) -> &toml::Value {
        &self.semantic
    }

    pub(super) fn assignment_span(&self, path: &[&str]) -> Option<ByteSpan> {
        self.node(path).map(|node| node.value_span)
    }

    pub(super) fn table_span(&self, path: &[&str]) -> Option<ByteSpan> {
        self.syntax.tables.iter().find_map(|table| {
            (table.path.len() == path.len()
                && table
                    .path
                    .iter()
                    .zip(path)
                    .all(|(left, right)| left == right))
            .then_some(table.span)
        })
    }

    pub(super) fn syntax_nodes(&self) -> &[SyntaxNode] {
        &self.syntax.nodes
    }

    pub(super) fn table_nodes(&self) -> &[super::settings_syntax::TableNode] {
        &self.syntax.tables
    }
}

fn owned_assignment(path: &[String], semantic: &toml::Value, catalog: &OwnerCatalog) -> bool {
    match path {
        [root] if root == "settings_schema" => true,
        [root, field]
            if root == "appearance"
                && matches!(field.as_str(), "theme" | "override_agent_theme") =>
        {
            true
        }
        [root, field]
            if root == "workbench"
                && matches!(
                    field.as_str(),
                    "initial_screen" | "enabled_screens" | "screen_order"
                ) =>
        {
            true
        }
        [root, field] if root == "workbench" && field == "layout_overrides" => {
            all_table_owners_known(semantic, path, catalog, OwnerKind::Screen)
        }
        [root, field, owner, ..] if root == "workbench" && field == "layout_overrides" => {
            owner_is(catalog, owner, OwnerKind::Screen)
        }
        [root, owner, ..] if root == "agents" => owner_is(catalog, owner, OwnerKind::Agent),
        [root, owner, ..] if root == "plugins" => owner_is(catalog, owner, OwnerKind::Plugin),
        [root, owner, ..] if root == "keymap" => known_owner(catalog, owner),
        _ => false,
    }
}

fn owner_is(catalog: &OwnerCatalog, text: &str, kind: OwnerKind) -> bool {
    Id::parse(text)
        .ok()
        .and_then(|id| catalog.get(&id))
        .is_some_and(|owner| owner.kind == kind)
}

fn known_owner(catalog: &OwnerCatalog, text: &str) -> bool {
    Id::parse(text)
        .ok()
        .and_then(|id| catalog.get(&id))
        .is_some()
}

fn all_table_owners_known(
    semantic: &toml::Value,
    path: &[String],
    catalog: &OwnerCatalog,
    kind: OwnerKind,
) -> bool {
    value_at_path(semantic, path)
        .and_then(toml::Value::as_table)
        .is_some_and(|owners| owners.keys().all(|owner| owner_is(catalog, owner, kind)))
}

fn value_at_path<'a>(value: &'a toml::Value, path: &[String]) -> Option<&'a toml::Value> {
    path.iter()
        .try_fold(value, |current, key| current.as_table()?.get(key))
}

/// One assignment a lossless editor may set, replace, or remove.
///
/// The decoded `path` is what locates an existing assignment, and the two text
/// fields are what is written when nothing is there yet. They are separate
/// because the document does not record how a key *would* have been spelled:
/// `keymap` quotes its action keys and declares `[keymap."<context>"]`, while
/// `appearance` writes bare keys under `[appearance]`. Deriving either from the
/// path would need a per-root special case in the patcher, which is exactly the
/// knowledge each editor already has.
pub(super) struct Assignment<'a> {
    /// Decoded path of the assignment, for example `["appearance", "theme"]`.
    pub path: &'a [&'a str],
    /// Exact header written when the owning table is absent, for example
    /// `[appearance]`.
    pub table_header: &'a str,
    /// Exact key text written when the assignment is absent, for example
    /// `theme`.
    pub key_text: &'a str,
}

/// Set, replace, or remove exactly one assignment, preserving every other byte.
///
/// `Some(value)` replaces an existing value span or inserts a new statement;
/// `None` removes an existing statement and is a no-op when there is none.
/// Why one assignment could not be patched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PatchRefusal {
    /// An ancestor of the leaf is written as one value — an inline table — so
    /// the leaf has no syntax of its own to replace, and adding a table header
    /// for it would redefine what the inline table already defines.
    InlineAncestor,
}

pub(super) fn patch_assignment(
    document: &SettingsDocument,
    assignment: &Assignment<'_>,
    value: Option<&[u8]>,
) -> Result<Vec<u8>, PatchRefusal> {
    if let Some(node) = document.node(assignment.path) {
        return Ok(match value {
            // Writing the value that is already there must not disturb how it
            // was written: `'green-screen'` and `"green-screen"` are the same
            // value, and replacing one with the other would make an edit that
            // undoes itself still count as a change to the file.
            Some(value) if same_value(document.span_bytes(node.value_span), value) => {
                document.original_bytes().to_vec()
            }
            Some(value) => apply_patches(
                document.original_bytes(),
                vec![(node.value_span, value.to_vec())],
            ),
            None => apply_patches(
                document.original_bytes(),
                vec![(node.statement_span, Vec::new())],
            ),
        });
    }
    if has_inline_ancestor(document, assignment.path) {
        return Err(PatchRefusal::InlineAncestor);
    }
    let Some(value) = value else {
        return Ok(document.original_bytes().to_vec());
    };
    Ok(insert_assignment(document, assignment, value))
}

/// Remove the whole `[table]` block spelling `path`, if the document has one.
///
/// A block is its header plus every statement up to the next header, which is
/// exactly the syntax that defines the subtree. Removing it is how a subtree
/// leaf written in header form is replaced: the caller then writes the
/// replacement as an ordinary assignment, so a tree has one spelling after an
/// edit rather than two definitions of the same key.
///
/// A document with no such header is returned unchanged.
pub(super) fn remove_table_block(document: &SettingsDocument, path: &[&str]) -> Vec<u8> {
    if document.table_span(path).is_none() {
        return document.original_bytes().to_vec();
    }
    // Only the syntax this block *owns* is removed: its own header line, the
    // header line of every nested table under it, and each of their
    // assignments. Deleting the byte range between two headers instead would
    // take the comments and blank lines sitting in front of the next one,
    // which belong to it and not to what is being replaced.
    let mut owned: Vec<ByteSpan> = document
        .table_nodes()
        .iter()
        .filter(|node| owns(&node.path, path))
        .map(|node| line_span(document, node.span))
        .chain(
            document
                .syntax_nodes()
                .iter()
                .filter(|node| owns(&node.path, path))
                .map(|node| line_span(document, node.statement_span)),
        )
        .collect();
    // A header's line and its first assignment's statement can cover some of
    // the same bytes, and splicing one range twice would cut the document in
    // the middle of what the other already removed.
    owned.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<ByteSpan> = Vec::new();
    for span in owned {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => {
                *last = ByteSpan::new(last.start, last.end.max(span.end));
            }
            _ => merged.push(span),
        }
    }
    apply_patches(
        document.original_bytes(),
        merged.into_iter().map(|span| (span, Vec::new())).collect(),
    )
}

/// The span extended to the end of the line it sits on.
///
/// A header carries no newline of its own, so removing one without its line
/// ending would leave a blank line where a line used to be. A statement span
/// that already ends on its newline is complete, and extending it again would
/// take the line after it.
fn line_span(document: &SettingsDocument, span: ByteSpan) -> ByteSpan {
    let bytes = document.original_bytes();
    let Ok(mut end) = usize::try_from(span.end) else {
        return span;
    };
    if end > 0 && bytes.get(end - 1) == Some(&b'\n') {
        return span;
    }
    while end < bytes.len() && bytes[end] != b'\n' {
        end += 1;
    }
    if end < bytes.len() {
        end += 1;
    }
    ByteSpan::new(span.start, end as u64)
}

/// Whether `path` is `prefix` itself or something nested inside it.
fn owns(path: &[String], prefix: &[&str]) -> bool {
    path.len() >= prefix.len()
        && path
            .iter()
            .zip(prefix)
            .all(|(left, right)| left.as_str() == *right)
}

/// Whether an ancestor of this leaf is written as one value.
///
/// `appearance = { theme = "x" }` gives `appearance` a value span of its own, so
/// `appearance.theme` has no syntax to replace and cannot be given a table
/// header without redefining what the inline table already defines. Saying so is
/// the only honest answer: silently doing nothing would lose the user's edit,
/// and writing the header would produce a document that no longer parses.
fn has_inline_ancestor(document: &SettingsDocument, path: &[&str]) -> bool {
    (1..path.len()).any(|depth| document.node(&path[..depth]).is_some())
}

/// Whether two value fragments denote the same TOML value.
///
/// A fragment is not a document, so each is parsed as the right-hand side of a
/// throwaway assignment. A fragment that will not parse is treated as different,
/// which is the safe answer: the patch then goes ahead and the whole candidate
/// is validated afterwards.
fn same_value(existing: &[u8], replacement: &[u8]) -> bool {
    match (parse_fragment(existing), parse_fragment(replacement)) {
        (Some(existing), Some(replacement)) => existing == replacement,
        _ => false,
    }
}

fn parse_fragment(fragment: &[u8]) -> Option<toml::Value> {
    let text = std::str::from_utf8(fragment).ok()?;
    let document = format!("value = {text}").parse::<toml::Value>().ok()?;
    document.as_table()?.get("value").cloned()
}

fn insert_assignment(
    document: &SettingsDocument,
    assignment: &Assignment<'_>,
    value: &[u8],
) -> Vec<u8> {
    let Some((_, table_path)) = assignment.path.split_last() else {
        return document.original_bytes().to_vec();
    };
    // Every value reaching here is rendered by `toml::Value::to_string`, so it
    // is UTF-8 by construction. Refusing rather than lossily transcoding means
    // a value this could not render leaves the document exactly as it was
    // instead of being written as something subtly different.
    let Ok(rendered) = std::str::from_utf8(value) else {
        return document.original_bytes().to_vec();
    };
    let statement = format!("{} = {rendered}\n", assignment.key_text);
    if let Some(table) = document.table_span(table_path) {
        // A table's own statements are the ones between its header and the next
        // header. Selecting by path prefix instead would reach into a nested
        // table — `[workbench.layout_overrides.x]` is under `workbench` — and
        // put the assignment inside it, where it means something else entirely.
        let boundary = document
            .table_nodes()
            .iter()
            .map(|node| node.span.start)
            .filter(|start| *start > table.start)
            .min()
            .unwrap_or_else(|| document.original_bytes().len() as u64);
        let end = document
            .syntax_nodes()
            .iter()
            .filter(|node| node.statement_span.start >= table.end)
            .filter(|node| node.statement_span.end <= boundary)
            .map(|node| node.statement_span.end)
            .max()
            .unwrap_or(table.end);
        // The new assignment has to start its own line. A header never ends in
        // a newline, and neither does the last statement of a file that has no
        // trailing one, so the byte actually there is what decides.
        let starts_a_line = usize::try_from(end)
            .ok()
            .and_then(|end| end.checked_sub(1))
            .and_then(|index| document.original_bytes().get(index))
            == Some(&b'\n');
        let prefix = if starts_a_line { "" } else { "\n" };
        return apply_patches(
            document.original_bytes(),
            vec![(
                ByteSpan::new(end, end),
                format!("{prefix}{statement}").into_bytes(),
            )],
        );
    }
    let mut block = Vec::new();
    if !document.original_bytes().ends_with(b"\n") {
        block.push(b'\n');
    }
    block.extend_from_slice(format!("{}\n{statement}", assignment.table_header).as_bytes());
    let end = document.original_bytes().len() as u64;
    apply_patches(
        document.original_bytes(),
        vec![(ByteSpan::new(end, end), block)],
    )
}

pub(super) fn apply_patches(original: &[u8], mut patches: Vec<(ByteSpan, Vec<u8>)>) -> Vec<u8> {
    patches.sort_by_key(|(span, _)| std::cmp::Reverse((span.start, span.end)));
    let mut candidate = original.to_vec();
    for (span, replacement) in patches {
        let (Ok(start), Ok(end)) = (usize::try_from(span.start), usize::try_from(span.end)) else {
            continue;
        };
        if start <= end && end <= candidate.len() {
            candidate.splice(start..end, replacement);
        }
    }
    candidate
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn validate_value(value: &toml::Value, depth: usize, path: &str) -> Result<(), Box<Diagnostic>> {
    if depth > NESTING_LIMIT {
        return Err(limit_diagnostic_at(path, "TOML nesting exceeds the limit"));
    }
    match value {
        toml::Value::String(value) if value.len() > STRING_LIMIT => {
            Err(limit_diagnostic_at(path, "string exceeds the byte limit"))
        }
        toml::Value::Array(values) => validate_array(values, depth, path),
        toml::Value::Table(values) => validate_table(values, depth, path),
        _ => Ok(()),
    }
}

fn validate_array(values: &[toml::Value], depth: usize, path: &str) -> Result<(), Box<Diagnostic>> {
    if values.len() > ARRAY_LIMIT {
        return Err(limit_diagnostic_at(path, "array exceeds the element limit"));
    }
    for (index, value) in values.iter().enumerate() {
        validate_value(value, depth + 1, &format!("{path}/{index}"))?;
    }
    Ok(())
}

fn validate_table(
    values: &toml::map::Map<String, toml::Value>,
    depth: usize,
    path: &str,
) -> Result<(), Box<Diagnostic>> {
    if values.len() > MAP_LIMIT {
        return Err(limit_diagnostic_at(path, "map exceeds the entry limit"));
    }
    for (key, value) in values {
        validate_value(value, depth + 1, &format!("{path}/{key}"))?;
    }
    Ok(())
}

fn syntax_diagnostic(span: Option<ByteSpan>, detail: &str) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E002,
        Severity::Error,
        DiagnosticPath::root(),
        span,
        "correct the TOML syntax without rewriting dormant content",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    Box::new(diagnostic)
}

fn limit_diagnostic(span: ByteSpan, detail: &str) -> Box<Diagnostic> {
    let mut diagnostic = *limit_diagnostic_at("/", detail);
    diagnostic.span = Some(span);
    Box::new(diagnostic)
}

fn limit_diagnostic_at(path: &str, detail: &str) -> Box<Diagnostic> {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E008,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        "reduce the value to the documented inclusive limit",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    Box::new(diagnostic)
}
