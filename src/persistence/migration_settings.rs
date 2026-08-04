//! One-way schema-1 settings reader over the lossless document authority.

use crate::domain::{ByteSpan, OwnerCatalog};

use super::super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use super::super::settings_document::{PublishedSettings, SettingsDocument};

/// In-memory result of reading schema-1 or schema-2 settings.
#[derive(Debug, Clone)]
pub struct SettingsMigration {
    document: SettingsDocument,
    published: PublishedSettings,
    migrated: bool,
}

impl SettingsMigration {
    /// Borrow the exact lossless source document.
    #[must_use]
    pub const fn document(&self) -> &SettingsDocument {
        &self.document
    }

    /// Borrow the effective schema-2 settings candidate.
    #[must_use]
    pub const fn published(&self) -> &PublishedSettings {
        &self.published
    }

    /// Report whether schema-1 values were migrated in memory.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.migrated
    }
}

/// Parse schema-2 settings or migrate schema-1 effective values without writing.
pub fn migrate_settings(
    bytes: &[u8],
    catalog: &OwnerCatalog,
) -> Result<SettingsMigration, Vec<Diagnostic>> {
    let document = SettingsDocument::parse(bytes).map_err(|diagnostic| vec![*diagnostic])?;
    let Some(root) = document.semantic().as_table() else {
        return Err(vec![settings_error(
            CfgCode::E003,
            "/",
            "settings root must be a table",
        )]);
    };
    if root.contains_key("settings_schema") {
        let published = document.publish(catalog)?;
        return Ok(SettingsMigration {
            document,
            published,
            migrated: false,
        });
    }
    migrate_schema1(document)
}

/// Build a complete schema-2 settings candidate while preserving dormant syntax.
pub fn format_migrated_settings(
    migration: &SettingsMigration,
    catalog: &OwnerCatalog,
) -> Result<Vec<u8>, Vec<Diagnostic>> {
    if !migration.was_migrated() {
        return migration.document.format_owned(catalog);
    }
    let candidate = schema1_format_candidate(migration)?;
    let validated = migrate_settings(&candidate, catalog)?;
    if validated.was_migrated() {
        return Err(vec![settings_error(
            CfgCode::E102,
            "/settings_schema",
            "formatted settings candidate did not become schema 2",
        )]);
    }
    Ok(candidate)
}

fn schema1_format_candidate(migration: &SettingsMigration) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let document = migration.document();
    let schema = required_node(document, &["schema_version"])?;
    let theme = required_node(document, &["theme"])?;
    let override_node = document.node(&["override_agent_theme"]);
    let mut replacement = schema2_known_block(migration, schema, theme, override_node)?;
    let mut patches = Vec::new();
    for node in root_statements(document) {
        if node.path.len() == 1 && node.path[0] == "schema_version" {
            patches.push((node.statement_span, replacement.clone()));
            replacement.clear();
        } else {
            patches.push((node.statement_span, Vec::new()));
        }
    }
    for table in document.table_nodes() {
        patches.push((
            table.span,
            prefixed_extension_header(document.span_bytes(table.span)),
        ));
    }
    Ok(super::super::settings_document::apply_patches(
        document.original_bytes(),
        patches,
    ))
}

fn schema2_known_block(
    migration: &SettingsMigration,
    schema: &super::super::settings_syntax::SyntaxNode,
    theme: &super::super::settings_syntax::SyntaxNode,
    override_node: Option<&super::super::settings_syntax::SyntaxNode>,
) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let document = migration.document();
    let Some(theme_value) = migration.published().appearance.theme.as_ref() else {
        return Err(vec![settings_error(
            CfgCode::E003,
            "/theme",
            "theme is missing",
        )]);
    };
    let override_value = migration
        .published()
        .appearance
        .override_agent_theme
        .unwrap_or(false);
    let mut block = b"settings_schema = 2".to_vec();
    block.extend_from_slice(statement_suffix(document, schema));
    block.extend_from_slice(b"\n[appearance]\ntheme = ");
    block.extend_from_slice(
        toml::Value::String(theme_value.clone())
            .to_string()
            .as_bytes(),
    );
    block.extend_from_slice(statement_suffix(document, theme));
    block.extend_from_slice(b"override_agent_theme = ");
    block.extend_from_slice(override_value.to_string().as_bytes());
    if let Some(node) = override_node {
        block.extend_from_slice(statement_suffix(document, node));
    } else {
        block.push(b'\n');
    }
    append_root_extensions(document, &mut block);
    Ok(block)
}

/// The statements written before the first table header.
///
/// A root statement is not the same thing as a single-segment path: a dotted key
/// such as `future.value = 1` is written at the root and has to move with the
/// rest of the root, but its path has two segments. Selecting by path length
/// would leave it behind, where the emitted `[appearance]` header would then
/// swallow it and change what it means.
fn root_statements(
    document: &SettingsDocument,
) -> impl Iterator<Item = &super::super::settings_syntax::SyntaxNode> {
    let boundary = document
        .table_nodes()
        .iter()
        .map(|table| table.span.start)
        .min()
        .unwrap_or_else(|| document.original_bytes().len() as u64);
    document
        .syntax_nodes()
        .iter()
        .filter(move |node| node.statement_span.start < boundary)
}

fn append_root_extensions(document: &SettingsDocument, block: &mut Vec<u8>) {
    let unknown = root_statements(document).filter(|node| {
        !matches!(
            node.path.first().map(String::as_str),
            Some("schema_version" | "theme" | "override_agent_theme")
        ) || node.path.len() > 1
    });
    let mut statements = unknown.peekable();
    if statements.peek().is_some() {
        block.extend_from_slice(b"\n[extensions.schema1]\n");
        for node in statements {
            block.extend_from_slice(document.span_bytes(node.statement_span));
        }
    }
}

fn required_node<'a>(
    document: &'a SettingsDocument,
    path: &[&str],
) -> Result<&'a super::super::settings_syntax::SyntaxNode, Vec<Diagnostic>> {
    document.node(path).ok_or_else(|| {
        vec![settings_error(
            CfgCode::E002,
            &format!("/{}", path.join("/")),
            "required schema-1 syntax node is missing",
        )]
    })
}

fn statement_suffix<'a>(
    document: &'a SettingsDocument,
    node: &super::super::settings_syntax::SyntaxNode,
) -> &'a [u8] {
    document.span_bytes(ByteSpan::new(node.value_span.end, node.statement_span.end))
}

fn prefixed_extension_header(header: &[u8]) -> Vec<u8> {
    let prefix = if header.starts_with(b"[[") {
        b"[[extensions.schema1.".as_slice()
    } else {
        b"[extensions.schema1.".as_slice()
    };
    let offset = if header.starts_with(b"[[") { 2 } else { 1 };
    let mut prefixed = prefix.to_vec();
    prefixed.extend_from_slice(header.get(offset..).unwrap_or_default());
    prefixed
}

fn migrate_schema1(document: SettingsDocument) -> Result<SettingsMigration, Vec<Diagnostic>> {
    let Some(root) = document.semantic().as_table() else {
        return Err(vec![settings_error(
            CfgCode::E003,
            "/",
            "settings root must be a table",
        )]);
    };
    if root.get("schema_version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(vec![settings_error(
            CfgCode::E102,
            "/schema_version",
            "schema_version must be the integer 1",
        )]);
    }
    let theme = root
        .get("theme")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            vec![settings_error(
                CfgCode::E003,
                "/theme",
                "schema-1 theme must be a string",
            )]
        })?
        .to_owned();
    let override_agent_theme = match root.get("override_agent_theme") {
        Some(value) => Some(value.as_bool().ok_or_else(|| {
            vec![settings_error(
                CfgCode::E003,
                "/override_agent_theme",
                "schema-1 override_agent_theme must be a boolean",
            )]
        })?),
        None => Some(false),
    };
    let mut published = PublishedSettings::default();
    published.appearance.theme = Some(theme);
    published.appearance.override_agent_theme = override_agent_theme;
    for key in root.keys() {
        if !matches!(
            key.as_str(),
            "schema_version" | "theme" | "override_agent_theme"
        ) {
            published
                .dormant
                .push(super::super::settings_document::DormantSettings {
                    path: vec![key.clone()],
                    span: document
                        .table_span(&[key.as_str()])
                        .or_else(|| document.assignment_span(&[key.as_str()])),
                });
        }
    }
    Ok(SettingsMigration {
        document,
        published,
        migrated: true,
    })
}

fn settings_error(code: CfgCode, path: &str, detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        "correct the settings document and retry validation",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}
