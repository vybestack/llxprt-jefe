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
    DormantSettings, PublishedAppearance, PublishedOwner, PublishedSettings, PublishedWorkbench,
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
