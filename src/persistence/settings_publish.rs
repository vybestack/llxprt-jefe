//! Closed semantic publication for lossless schema-2 settings documents.

use std::collections::BTreeMap;

use crate::domain::{
    ByteSpan, CanonicalDateTime, CanonicalDecimal, CanonicalSemver, Id, OwnerCatalog,
    OwnerDescriptor, OwnerKind, ProvenanceKind, ProvenanceOrigin, SecretRef, TypedMap, TypedValue,
};

use super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use super::settings_document::SettingsDocument;

const ALLOWED_ROOTS: [&str; 6] = [
    "appearance",
    "workbench",
    "agents",
    "keymap",
    "plugins",
    "extensions",
];

/// Published appearance settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishedAppearance {
    pub theme: Option<String>,
    pub override_agent_theme: Option<bool>,
}

/// Published workbench settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishedWorkbench {
    pub initial_screen: Option<Id>,
    pub enabled_screens: Vec<Id>,
    pub screen_order: Vec<Id>,
    pub layout_overrides: BTreeMap<Id, TypedMap>,
}

/// One active known owner's effective settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedOwner {
    pub enabled: Option<bool>,
    pub version: Option<CanonicalSemver>,
    pub values: TypedMap,
    provenance: BTreeMap<Vec<Id>, Vec<ProvenanceOrigin>>,
}

impl PublishedOwner {
    /// Borrow provenance for one relative typed-value leaf.
    #[must_use]
    pub fn origins(&self, path: &[Id]) -> &[ProvenanceOrigin] {
        self.provenance.get(path).map_or(&[], Vec::as_slice)
    }
}

/// Byte-preserved syntax that is intentionally not published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantSettings {
    pub path: Vec<String>,
    pub span: Option<ByteSpan>,
}

/// Effective, typed schema-2 settings plus skipped dormant syntax.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishedSettings {
    pub appearance: PublishedAppearance,
    pub workbench: PublishedWorkbench,
    pub agents: BTreeMap<Id, PublishedOwner>,
    pub keymap: BTreeMap<Id, BTreeMap<Id, Vec<String>>>,
    pub plugins: BTreeMap<Id, PublishedOwner>,
    pub dormant: Vec<DormantSettings>,
}

pub(super) fn publish(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
) -> Result<PublishedSettings, Vec<Diagnostic>> {
    let Some(root) = document.semantic().as_table() else {
        return Err(vec![type_diagnostic("/", "settings root must be a table")]);
    };
    validate_schema(root)?;
    validate_roots(root)?;

    let mut published = PublishedSettings::default();
    if let Some(value) = root.get("appearance") {
        published.appearance = parse_appearance(value)?;
    }
    if let Some(value) = root.get("workbench") {
        parse_workbench(document, catalog, value, &mut published)?;
    }
    if let Some(value) = root.get("agents") {
        parse_owner_root(document, catalog, value, OwnerKind::Agent, &mut published)?;
    }
    if let Some(value) = root.get("keymap") {
        parse_keymap(document, catalog, value, &mut published)?;
    }
    if let Some(value) = root.get("plugins") {
        parse_owner_root(document, catalog, value, OwnerKind::Plugin, &mut published)?;
    }
    if root.contains_key("extensions") {
        published.dormant.push(dormant(document, &["extensions"]));
    }
    Ok(published)
}

fn validate_schema(root: &toml::map::Map<String, toml::Value>) -> Result<(), Vec<Diagnostic>> {
    if root
        .get("settings_schema")
        .and_then(toml::Value::as_integer)
        == Some(2)
    {
        return Ok(());
    }
    Err(vec![diagnostic(
        CfgCode::E102,
        "/settings_schema",
        "settings_schema must be the integer 2",
    )])
}

fn validate_roots(root: &toml::map::Map<String, toml::Value>) -> Result<(), Vec<Diagnostic>> {
    for key in root.keys() {
        if key != "settings_schema" && !ALLOWED_ROOTS.contains(&key.as_str()) {
            return Err(vec![ownership_diagnostic(
                &format!("/{key}"),
                "settings root is not owned by the schema",
            )]);
        }
    }
    Ok(())
}

fn parse_appearance(value: &toml::Value) -> Result<PublishedAppearance, Vec<Diagnostic>> {
    let table = required_table(value, "/appearance")?;
    validate_fields(table, "/appearance", &["theme", "override_agent_theme"])?;
    let theme = optional_string(table, "theme", "/appearance/theme")?;
    let override_agent_theme = optional_bool(
        table,
        "override_agent_theme",
        "/appearance/override_agent_theme",
    )?;
    Ok(PublishedAppearance {
        theme,
        override_agent_theme,
    })
}

fn parse_workbench(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
    value: &toml::Value,
    published: &mut PublishedSettings,
) -> Result<(), Vec<Diagnostic>> {
    let table = required_table(value, "/workbench")?;
    validate_fields(
        table,
        "/workbench",
        &[
            "initial_screen",
            "enabled_screens",
            "screen_order",
            "layout_overrides",
        ],
    )?;
    published.workbench.initial_screen = optional_id(table, "initial_screen")?;
    published.workbench.enabled_screens = optional_id_array(table, "enabled_screens")?;
    published.workbench.screen_order = optional_id_array(table, "screen_order")?;
    if let Some(layouts) = table.get("layout_overrides") {
        parse_layouts(document, catalog, layouts, published)?;
    }
    Ok(())
}

fn parse_layouts(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
    value: &toml::Value,
    published: &mut PublishedSettings,
) -> Result<(), Vec<Diagnostic>> {
    let table = required_table(value, "/workbench/layout_overrides")?;
    for (owner_text, value) in table {
        let owner_id = parse_id(owner_text, "/workbench/layout_overrides")?;
        let Some(owner) = catalog.get(&owner_id) else {
            published.dormant.push(dormant(
                document,
                &["workbench", "layout_overrides", owner_text],
            ));
            continue;
        };
        require_owner_kind(owner, OwnerKind::Screen)?;
        let mut values = owner.defaults.clone();
        merge_typed_map(
            &mut values,
            toml_to_typed_map(value, "/workbench/layout_overrides")?,
        );
        published
            .workbench
            .layout_overrides
            .insert(owner_id, values);
    }
    Ok(())
}

fn parse_owner_root(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
    value: &toml::Value,
    kind: OwnerKind,
    published: &mut PublishedSettings,
) -> Result<(), Vec<Diagnostic>> {
    let root_name = owner_root_name(kind);
    let table = required_table(value, &format!("/{root_name}"))?;
    for (owner_text, value) in table {
        let owner_id = parse_id(owner_text, &format!("/{root_name}"))?;
        let Some(descriptor) = catalog.get(&owner_id) else {
            published
                .dormant
                .push(dormant(document, &[root_name, owner_text]));
            continue;
        };
        require_owner_kind(descriptor, kind)?;
        let owner = parse_known_owner(document, root_name, owner_text, value, descriptor)?;
        match kind {
            OwnerKind::Agent => {
                published.agents.insert(owner_id, owner);
            }
            OwnerKind::Plugin => {
                published.plugins.insert(owner_id, owner);
            }
            OwnerKind::Screen => {}
        }
    }
    Ok(())
}

fn parse_known_owner(
    document: &SettingsDocument,
    root_name: &str,
    owner_text: &str,
    value: &toml::Value,
    descriptor: &OwnerDescriptor,
) -> Result<PublishedOwner, Vec<Diagnostic>> {
    let path = format!("/{root_name}/{owner_text}");
    let table = required_table(value, &path)?;
    let value_field = if descriptor.kind == OwnerKind::Agent {
        "repository_defaults"
    } else {
        "config"
    };
    // Agents own exactly two fields; `parse_owner_version` returns early for
    // non-plugin owners, so `version` is deliberately absent here rather than
    // padding the array to a matching length.
    let fields: &[&str] = if descriptor.kind == OwnerKind::Agent {
        &["enabled", "repository_defaults"]
    } else {
        &["enabled", "version", "config"]
    };
    validate_fields(table, &path, fields)?;
    let enabled = optional_bool(table, "enabled", &format!("{path}/enabled"))?;
    let version = parse_owner_version(table, descriptor)?;
    let mut values = descriptor.defaults.clone();
    let mut provenance = default_provenance(&values);
    if let Some(raw_values) = table.get(value_field) {
        let overrides = toml_to_typed_map(raw_values, &format!("{path}/{value_field}"))?;
        merge_typed_map(&mut values, overrides.clone());
        add_selected_provenance(
            &mut provenance,
            &overrides,
            document.assignment_span(&[root_name, owner_text, value_field]),
        );
    }
    Ok(PublishedOwner {
        enabled,
        version,
        values,
        provenance,
    })
}

fn parse_owner_version(
    table: &toml::map::Map<String, toml::Value>,
    descriptor: &OwnerDescriptor,
) -> Result<Option<CanonicalSemver>, Vec<Diagnostic>> {
    if descriptor.kind != OwnerKind::Plugin {
        return Ok(None);
    }
    let Some(value) = table.get("version") else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(vec![type_diagnostic(
            "/plugins/version",
            "plugin version must be a string",
        )]);
    };
    CanonicalSemver::parse(value).map(Some).map_err(|_| {
        vec![type_diagnostic(
            "/plugins/version",
            "plugin version is not canonical SemVer",
        )]
    })
}

fn parse_keymap(
    document: &SettingsDocument,
    catalog: &OwnerCatalog,
    value: &toml::Value,
    published: &mut PublishedSettings,
) -> Result<(), Vec<Diagnostic>> {
    let table = required_table(value, "/keymap")?;
    for (owner_text, value) in table {
        let owner_id = parse_id(owner_text, "/keymap")?;
        let Some(owner) = catalog.get(&owner_id) else {
            published
                .dormant
                .push(dormant(document, &["keymap", owner_text]));
            continue;
        };
        require_owner_kind(owner, OwnerKind::Screen)?;
        let bindings = required_table(value, &format!("/keymap/{owner_text}"))?;
        let mut actions = BTreeMap::new();
        for (action_text, chords) in bindings {
            let action_id = parse_id(action_text, "/keymap/action")?;
            actions.insert(action_id, string_array(chords, "/keymap/chords")?);
        }
        published.keymap.insert(owner_id, actions);
    }
    Ok(())
}

fn toml_to_typed_map(value: &toml::Value, path: &str) -> Result<TypedMap, Vec<Diagnostic>> {
    let table = required_table(value, path)?;
    let mut result = TypedMap::new();
    for (key, value) in table {
        let id = parse_id(key, path)?;
        result.insert(id, toml_to_typed(value, &format!("{path}/{key}"))?);
    }
    Ok(result)
}

fn toml_to_typed(value: &toml::Value, path: &str) -> Result<TypedValue, Vec<Diagnostic>> {
    match value {
        toml::Value::String(value) => Ok(TypedValue::String(value.clone())),
        toml::Value::Integer(value) => Ok(TypedValue::Integer(*value)),
        toml::Value::Float(value) => CanonicalDecimal::parse(&value.to_string())
            .map(TypedValue::Decimal)
            .map_err(|_| {
                vec![type_diagnostic(
                    path,
                    "float is not a canonical finite decimal",
                )]
            }),
        toml::Value::Boolean(value) => Ok(TypedValue::Bool(*value)),
        toml::Value::Datetime(value) => CanonicalDateTime::parse(&value.to_string())
            .map(TypedValue::Datetime)
            .map_err(|_| vec![type_diagnostic(path, "datetime is not canonical")]),
        toml::Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| toml_to_typed(value, &format!("{path}/{index}")))
            .collect::<Result<Vec<_>, _>>()
            .map(TypedValue::List),
        toml::Value::Table(values) => table_to_typed(values, path),
    }
}

fn table_to_typed(
    values: &toml::map::Map<String, toml::Value>,
    path: &str,
) -> Result<TypedValue, Vec<Diagnostic>> {
    if values.len() == 1
        && let Some(secret) = values.get("secret_ref")
    {
        let Some(secret) = secret.as_str() else {
            return Err(vec![type_diagnostic(
                path,
                "secret_ref must be an identifier string",
            )]);
        };
        let id = parse_id(secret, path)?;
        return Ok(TypedValue::SecretRef(SecretRef { id }));
    }
    toml_to_typed_map(&toml::Value::Table(values.clone()), path).map(TypedValue::Map)
}

fn merge_typed_map(target: &mut TypedMap, source: TypedMap) {
    for (key, value) in source {
        if let (Some(TypedValue::Map(target_map)), TypedValue::Map(source_map)) =
            (target.get_mut(&key), &value)
        {
            merge_typed_map(target_map, source_map.clone());
        } else {
            target.insert(key, value);
        }
    }
}

fn default_provenance(values: &TypedMap) -> BTreeMap<Vec<Id>, Vec<ProvenanceOrigin>> {
    let mut result = BTreeMap::new();
    collect_provenance(
        values,
        &[],
        ProvenanceOrigin {
            kind: ProvenanceKind::BuiltInDefault,
            canonical_path: None,
            span: None,
        },
        &mut result,
    );
    result
}

fn add_selected_provenance(
    provenance: &mut BTreeMap<Vec<Id>, Vec<ProvenanceOrigin>>,
    values: &TypedMap,
    span: Option<ByteSpan>,
) {
    collect_provenance(
        values,
        &[],
        ProvenanceOrigin {
            kind: ProvenanceKind::SelectedDocument,
            canonical_path: None,
            span,
        },
        provenance,
    );
}

fn collect_provenance(
    values: &TypedMap,
    prefix: &[Id],
    origin: ProvenanceOrigin,
    output: &mut BTreeMap<Vec<Id>, Vec<ProvenanceOrigin>>,
) {
    for (key, value) in values {
        let mut path = prefix.to_vec();
        path.push(key.clone());
        if let TypedValue::Map(nested) = value {
            collect_provenance(nested, &path, origin.clone(), output);
        } else {
            output.entry(path).or_default().push(origin.clone());
        }
    }
}

fn optional_id(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<Id>, Vec<Diagnostic>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(vec![type_diagnostic(
            &format!("/workbench/{key}"),
            "value must be an identifier string",
        )]);
    };
    parse_id(value, &format!("/workbench/{key}")).map(Some)
}

fn optional_id_array(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Vec<Id>, Vec<Diagnostic>> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    string_array(value, &format!("/workbench/{key}"))?
        .iter()
        .map(|value| parse_id(value, &format!("/workbench/{key}")))
        .collect()
}

fn string_array(value: &toml::Value, path: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
    let Some(values) = value.as_array() else {
        return Err(vec![type_diagnostic(
            path,
            "value must be an array of strings",
        )]);
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| vec![type_diagnostic(path, "array element must be a string")])
        })
        .collect()
}

fn optional_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    path: &str,
) -> Result<Option<String>, Vec<Diagnostic>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(|value| Some(value.to_owned()))
        .ok_or_else(|| vec![type_diagnostic(path, "value must be a string")])
}

fn optional_bool(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    path: &str,
) -> Result<Option<bool>, Vec<Diagnostic>> {
    let Some(value) = table.get(key) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| vec![type_diagnostic(path, "value must be a boolean")])
}

fn required_table<'a>(
    value: &'a toml::Value,
    path: &str,
) -> Result<&'a toml::map::Map<String, toml::Value>, Vec<Diagnostic>> {
    value
        .as_table()
        .ok_or_else(|| vec![type_diagnostic(path, "value must be a table")])
}

fn validate_fields(
    table: &toml::map::Map<String, toml::Value>,
    path: &str,
    allowed: &[&str],
) -> Result<(), Vec<Diagnostic>> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(vec![ownership_diagnostic(
                &format!("{path}/{key}"),
                "field is not owned by this active owner",
            )]);
        }
    }
    Ok(())
}

fn require_owner_kind(
    descriptor: &OwnerDescriptor,
    expected: OwnerKind,
) -> Result<(), Vec<Diagnostic>> {
    if descriptor.kind == expected {
        return Ok(());
    }
    Err(vec![ownership_diagnostic(
        &format!("/owners/{}", descriptor.owner_id),
        "owner kind does not match the settings subtree",
    )])
}

fn parse_id(value: &str, path: &str) -> Result<Id, Vec<Diagnostic>> {
    Id::parse(value).map_err(|_| vec![type_diagnostic(path, "value is not a valid Id")])
}

fn owner_root_name(kind: OwnerKind) -> &'static str {
    match kind {
        OwnerKind::Agent => "agents",
        OwnerKind::Plugin => "plugins",
        OwnerKind::Screen => "workbench",
    }
}

fn dormant(document: &SettingsDocument, path: &[&str]) -> DormantSettings {
    DormantSettings {
        path: path.iter().map(|value| (*value).to_owned()).collect(),
        span: document.table_span(path),
    }
}

fn type_diagnostic(path: &str, detail: &str) -> Diagnostic {
    diagnostic(CfgCode::E003, path, detail)
}

fn ownership_diagnostic(path: &str, detail: &str) -> Diagnostic {
    diagnostic(CfgCode::E005, path, detail)
}

fn diagnostic(code: CfgCode, path: &str, detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        code,
        Severity::Error,
        DiagnosticPath::new(path),
        None,
        "correct the selected settings document",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}
