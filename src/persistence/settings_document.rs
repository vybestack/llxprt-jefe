//! Lossless settings document retaining original bytes and a semantic overlay.

use crate::domain::ByteSpan;

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
    pub fn publish(
        &self,
        catalog: &crate::domain::OwnerCatalog,
    ) -> Result<PublishedSettings, Vec<Diagnostic>> {
        publish(self, catalog)
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
