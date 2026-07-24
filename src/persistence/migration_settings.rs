//! One-way schema-1 settings reader over the lossless document authority.

use crate::domain::OwnerCatalog;

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
