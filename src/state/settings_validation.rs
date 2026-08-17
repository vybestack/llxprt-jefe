//! Candidate rebuilding and package-config validation for Settings drafts.
//!
//! This module is pure composition over explicit draft, schema, and committed
//! screen-registry inputs. It neither reads ambient Settings nor publishes a
//! replacement declaration authority.

use std::collections::BTreeMap;

use crate::config_owners::builtin_owner_catalog;
use crate::domain::plugin::{FieldKind, SecretReference};
use crate::domain::plugin_config::{ConfigValueError, validate_config};
use crate::domain::{
    CanonicalDecimal, ConfigContractError, Id, OwnerCatalog, OwnerDescriptor, OwnerKind, TypedMap,
    TypedValue,
};
use crate::messages::settings::SelectedPluginConfig;
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::migration::SettingsMigration;
use crate::persistence::settings_document::PublishedSettings;
use crate::persistence::settings_edit::load_settings_base;
use crate::persistence::writer::ExpectedHash;
use crate::persistence::{PluginConfigEditValue, SettingsCandidate, SettingsEdit, SyntaxPath};

use super::settings_types::{
    DraftCandidate, DraftStatus, PluginConfigChange, PluginConfigDiffRow, PluginConfigMigration,
    SettingsDraft,
};

/// Rebuild the complete candidate and the status the edits imply.
pub(super) fn revalidate(
    draft: &mut SettingsDraft,
    schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
    registry: &crate::workbench::ScreenRegistry,
) {
    let edits = draft
        .edited_paths()
        .filter_map(|path| draft.edit(path).cloned())
        .collect::<Vec<_>>();
    let candidate = build_candidate(
        draft.base(),
        &edits,
        draft.base_expected(),
        schemas,
        registry,
    );
    let unchanged = candidate
        .valid()
        .is_some_and(|candidate| candidate.bytes() == draft.base().document().original_bytes());
    draft.set_candidate(candidate);
    if unchanged {
        // Every edit put the document back exactly where it started, so there
        // is nothing unsaved left to warn about.
        draft.forget_edits();
        draft.set_preview(None);
    }
    if !draft.status().needs_recovery() && !draft.status().is_saving() {
        draft.set_status(if draft.is_dirty() {
            DraftStatus::Dirty
        } else {
            DraftStatus::Clean
        });
    }
}

/// Apply the reset-then-edit cutover for one approved plugin-config migration.
pub(super) fn apply_migration_edits(
    draft: &mut SettingsDraft,
    owner: &Id,
    reset_fields: Vec<Id>,
    target_edits: Vec<(Id, PluginConfigEditValue)>,
) {
    for field in reset_fields {
        draft.record(SettingsEdit::Reset(SyntaxPath::PluginConfig {
            plugin: owner.clone(),
            field,
        }));
    }
    for (field, value) in target_edits {
        draft.record(SettingsEdit::PluginConfig {
            plugin: owner.clone(),
            field,
            value,
        });
    }
}

/// Build the complete candidate one edit set describes.
pub(super) fn build_candidate(
    base: &SettingsMigration,
    edits: &[SettingsEdit],
    expected: ExpectedHash,
    schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
    registry: &crate::workbench::ScreenRegistry,
) -> DraftCandidate {
    // The package-aware catalog recognizes installed plugin owners, so their
    // config tables publish into `PublishedSettings.plugins` rather than being
    // preserved byte-for-byte as dormant unknown syntax (issue #390, #391).
    let Ok(catalog) = catalog_with_plugin_schemas(schemas) else {
        return DraftCandidate::Blocked(vec![internal_diagnostic(
            "the compiled owner catalog is unavailable",
        )]);
    };
    match SettingsCandidate::from_edits(base, &catalog, edits, expected) {
        Ok(candidate) => {
            let mut diagnostics =
                super::settings_registry_ops::registry_refusals(&candidate, registry);
            // Validate active selected owners' plugin config against their
            // immutable selected-package ConfigSchema. Dormant and disabled
            // owners are absent from `published.plugins` (dormant) or have
            // `enabled != Some(true)` (disabled), so they are never validated
            // here — their bytes survive untouched (CW11-09).
            diagnostics.extend(plugin_config_refusals(schemas, candidate.published()));
            diagnostics.sort();
            if diagnostics.is_empty() {
                DraftCandidate::Valid(Box::new(candidate))
            } else {
                DraftCandidate::Refused {
                    candidate: Box::new(candidate),
                    diagnostics,
                }
            }
        }
        Err(diagnostics) => DraftCandidate::Blocked(diagnostics),
    }
}

/// Build the owner catalog with plugin owners recognized from their config
/// schemas (issue #391 CW11-06/07).
///
/// The schemas map is keyed by owner id; every key is a plugin owner the
/// catalog must recognize so its config table publishes rather than being
/// preserved as dormant unknown syntax. The version stored on the descriptor
/// is a placeholder: publishing reads the version from the TOML document, not
/// from the catalog, so the descriptor version does not affect candidate
/// building or config validation here.
fn catalog_with_plugin_schemas(
    schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
) -> Result<OwnerCatalog, ConfigContractError> {
    let mut catalog = builtin_owner_catalog()?;
    for (owner, versions) in schemas {
        let Some(selected) = versions.first() else {
            continue;
        };
        catalog.insert(OwnerDescriptor {
            owner_id: owner.clone(),
            version: selected.version.clone(),
            kind: OwnerKind::Plugin,
            defaults: std::collections::BTreeMap::new(),
            secret_paths: std::collections::BTreeSet::new(),
        })?;
    }
    Ok(catalog)
}

/// The sorted diagnostics that block Save when an active selected owner's
/// plugin config is invalid against its immutable ConfigSchema (CW11-07).
///
/// Only owners that are both **published** (in `plugins`, not dormant) and
/// **active** (`enabled == Some(true)`) are validated. Absent and disabled
/// owners are skipped — their values survive byte-for-byte without schema
/// validation (CW11-09). The sole validator is `domain::plugin_config`.
pub(super) fn plugin_config_refusals(
    schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
    published: &PublishedSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (owner, versions) in schemas {
        let Some(plugin_owner) = published.plugins.get(owner) else {
            // Dormant: the owner is absent from published settings, so its
            // bytes survive without validation.
            continue;
        };
        if plugin_owner.enabled != Some(true) {
            // Disabled: the owner is published but not active, so its values
            // are a dormant choice rather than a live configuration.
            continue;
        }
        let selected = plugin_owner.version.as_ref().map_or_else(
            || versions.first(),
            |version| {
                versions
                    .iter()
                    .find(|candidate| candidate.version == *version)
            },
        );
        let Some(selected) = selected else {
            diagnostics.push(missing_plugin_schema_diagnostic(owner));
            continue;
        };
        for error in validate_config(&selected.schema, &plugin_owner.values) {
            diagnostics.push(config_error_diagnostic(owner, &error));
        }
    }
    diagnostics.sort();
    diagnostics
}

fn missing_plugin_schema_diagnostic(owner: &Id) -> Diagnostic {
    let path = format!("/plugins/{}", owner.as_str());
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::new(&path),
        None,
        "select an installed package version before saving",
    );
    "the selected package version has no installed configuration schema"
        .clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

/// One config validation failure as a sorted diagnostic the Settings screen
/// shows adjacent to its field and in the summary.
fn config_error_diagnostic(owner: &Id, error: &ConfigValueError) -> Diagnostic {
    let path = format!(
        "/plugins/{}/config/{}",
        owner.as_str(),
        error.field.as_str()
    );
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::new(&path),
        None,
        config_error_correction(error),
    );
    diagnostic.redacted_detail = config_error_detail(error.reason);
    diagnostic
}

/// The operator-facing correction for one config validation failure.
fn config_error_correction(error: &ConfigValueError) -> String {
    format!(
        "correct the {} field so it satisfies the selected package's config schema",
        error.field.as_str()
    )
}

/// The redacted detail for one config validation failure kind.
fn config_error_detail(reason: crate::domain::plugin_config::ConfigValueErrorKind) -> String {
    use crate::domain::plugin_config::ConfigValueErrorKind;
    match reason {
        ConfigValueErrorKind::Required => "a required visible field has no value".to_owned(),
        ConfigValueErrorKind::Type => "the value has the wrong field type".to_owned(),
        ConfigValueErrorKind::BelowMinimum => {
            "the value or its length is below the minimum".to_owned()
        }
        ConfigValueErrorKind::AboveMaximum => {
            "the value or its length is above the maximum".to_owned()
        }
        ConfigValueErrorKind::Choice => "the value is not one of the declared choices".to_owned(),
        ConfigValueErrorKind::Duplicate => "the list contains a duplicate entry".to_owned(),
        ConfigValueErrorKind::Unknown => "the field is not declared by the schema".to_owned(),
    }
}

/// Load one settings base, or the diagnostics that stop it being editable.
pub(super) fn load_base(
    bytes: Option<&[u8]>,
    schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
) -> Result<SettingsMigration, Vec<Diagnostic>> {
    let catalog = catalog_with_plugin_schemas(schemas).map_err(|_| {
        vec![internal_diagnostic(
            "the compiled owner catalog is unavailable",
        )]
    })?;
    load_settings_base(bytes, &catalog)
}

fn internal_diagnostic(detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::root(),
        None,
        "reinstall Jefe: the compiled configuration contract is malformed",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}

pub(super) fn parse_plugin_config_edit(
    kind: FieldKind,
    text: &str,
) -> Result<PluginConfigEditValue, &'static str> {
    match kind {
        FieldKind::String | FieldKind::Enum | FieldKind::Path => {
            Ok(PluginConfigEditValue::String(text.to_owned()))
        }
        FieldKind::Integer => text
            .parse::<i64>()
            .map(PluginConfigEditValue::Integer)
            .map_err(|_| "enter an integer"),
        FieldKind::FiniteNumber => CanonicalDecimal::parse(text)
            .map(PluginConfigEditValue::FiniteNumber)
            .map_err(|_| "enter a finite canonical number"),
        FieldKind::StringList => Ok(PluginConfigEditValue::StringList(
            text.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        )),
        FieldKind::SecretReference => SecretReference::parse(text)
            .map(PluginConfigEditValue::SecretReference)
            .map_err(|_| "enter an environment variable name"),
        FieldKind::Boolean => Err("boolean fields toggle in place"),
    }
}

pub(super) fn migration_requirement(
    draft: &SettingsDraft,
    source_schemas: &BTreeMap<Id, SelectedPluginConfig>,
    installed_schemas: &BTreeMap<Id, Vec<SelectedPluginConfig>>,
    approved: &BTreeMap<Id, crate::domain::CanonicalSemver>,
    exit_after_save: bool,
) -> Option<PluginConfigMigration> {
    let target_settings = draft.candidate().described()?.published();
    let source_settings = draft.base().published();
    for (owner, source) in source_schemas {
        let Some(target_owner) = target_settings.plugins.get(owner) else {
            continue;
        };
        if target_owner.enabled != Some(true) {
            continue;
        }
        let Some(target_version) = target_owner.version.as_ref() else {
            continue;
        };
        if target_version == &source.version || approved.get(owner) == Some(target_version) {
            continue;
        }
        let Some(target) = selected_config(installed_schemas, owner, target_version) else {
            continue;
        };
        if !target.can_migrate || target.schema.schema_version() == source.schema.schema_version() {
            continue;
        }
        return Some(PluginConfigMigration {
            owner: owner.clone(),
            source_package_version: source.version.clone(),
            target_package_version: target_version.clone(),
            from_schema_version: source.schema.schema_version(),
            to_schema_version: target.schema.schema_version(),
            source_config: source_settings
                .plugins
                .get(owner)
                .map_or_else(TypedMap::new, |plugin| plugin.values.clone()),
            draft_token: draft.token(),
            exit_after_save,
        });
    }
    None
}

fn selected_config<'a>(
    schemas: &'a BTreeMap<Id, Vec<SelectedPluginConfig>>,
    owner: &Id,
    version: &crate::domain::CanonicalSemver,
) -> Option<&'a SelectedPluginConfig> {
    schemas
        .get(owner)?
        .iter()
        .find(|selected| selected.version == *version)
}

pub(super) fn selected_schema<'a>(
    schemas: &'a BTreeMap<Id, Vec<SelectedPluginConfig>>,
    owner: &Id,
    version: &crate::domain::CanonicalSemver,
) -> Option<&'a crate::domain::plugin::ConfigSchema> {
    selected_config(schemas, owner, version).map(|selected| &selected.schema)
}

pub(super) fn redacted_config_diff(
    owner: &Id,
    source: &TypedMap,
    target: &TypedMap,
) -> Vec<PluginConfigDiffRow> {
    let mut paths = source
        .keys()
        .chain(target.keys())
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|field| {
            let change = match (source.get(&field), target.get(&field)) {
                (None, Some(_)) => PluginConfigChange::Added,
                (Some(_), None) => PluginConfigChange::Removed,
                (Some(left), Some(right)) if left != right => PluginConfigChange::Changed,
                _ => return None,
            };
            Some(PluginConfigDiffRow {
                path: format!("/plugins/{owner}/config/{field}"),
                change,
            })
        })
        .collect()
}

pub(super) fn plugin_config_edit_value(value: TypedValue) -> Option<PluginConfigEditValue> {
    match value {
        TypedValue::Bool(value) => Some(PluginConfigEditValue::Boolean(value)),
        TypedValue::String(value) => Some(PluginConfigEditValue::String(value)),
        TypedValue::Integer(value) => Some(PluginConfigEditValue::Integer(value)),
        TypedValue::Decimal(value) => Some(PluginConfigEditValue::FiniteNumber(value)),
        TypedValue::List(values) => values
            .into_iter()
            .map(|value| match value {
                TypedValue::String(value) => Some(value),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(PluginConfigEditValue::StringList),
        TypedValue::SecretRef(reference) => {
            Some(PluginConfigEditValue::SecretReference(reference.env))
        }
        TypedValue::Datetime(_) | TypedValue::Map(_) => None,
    }
}
