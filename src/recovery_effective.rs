//! Canonical, redacted rendering of published effective settings.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::domain::{Id, ProvenanceKind, TypedMap, TypedValue};
use crate::persistence::migration::SettingsMigration;
use crate::persistence::settings_document::{PublishedOwner, PublishedSettings};

pub(super) fn render(
    migration: &SettingsMigration,
    selected_path: &str,
    include_provenance: bool,
) -> Result<String, String> {
    let mut root = toml::map::Map::new();
    root.insert("settings_schema".to_owned(), toml::Value::Integer(2));
    insert_appearance(&mut root, migration.published());
    insert_workbench(&mut root, migration)?;
    insert_owners(
        &mut root,
        "agents",
        "repository_defaults",
        &migration.published().agents,
    )?;
    insert_keymap(&mut root, migration.published());
    insert_owners(
        &mut root,
        "plugins",
        "config",
        &migration.published().plugins,
    )?;

    let mut rendered = toml::to_string_pretty(&toml::Value::Table(root))
        .map_err(|_| "cannot serialize canonical effective settings".to_owned())?;
    if include_provenance {
        append_provenance(&mut rendered, migration, selected_path);
    }
    Ok(rendered)
}

fn insert_appearance(root: &mut toml::map::Map<String, toml::Value>, settings: &PublishedSettings) {
    let mut appearance = toml::map::Map::new();
    if let Some(theme) = &settings.appearance.theme {
        appearance.insert("theme".to_owned(), toml::Value::String(theme.clone()));
    }
    if let Some(value) = settings.appearance.override_agent_theme {
        appearance.insert(
            "override_agent_theme".to_owned(),
            toml::Value::Boolean(value),
        );
    }
    if !appearance.is_empty() {
        root.insert("appearance".to_owned(), toml::Value::Table(appearance));
    }
}

fn insert_workbench(
    root: &mut toml::map::Map<String, toml::Value>,
    migration: &SettingsMigration,
) -> Result<(), String> {
    let settings = migration.published();
    let mut workbench = toml::map::Map::new();
    if let Some(screen) = &settings.workbench.initial_screen {
        workbench.insert(
            "initial_screen".to_owned(),
            toml::Value::String(screen.to_string()),
        );
    }
    insert_id_array(
        &mut workbench,
        "enabled_screens",
        &settings.workbench.enabled_screens,
        migration
            .document()
            .node(&["workbench", "enabled_screens"])
            .is_some(),
    );
    insert_id_array(
        &mut workbench,
        "screen_order",
        &settings.workbench.screen_order,
        migration
            .document()
            .node(&["workbench", "screen_order"])
            .is_some(),
    );
    if !settings.workbench.layout_overrides.is_empty() {
        let layouts = settings
            .workbench
            .layout_overrides
            .iter()
            .map(|(owner, values)| {
                typed_map_to_toml(values).map(|value| (owner.to_string(), value))
            })
            .collect::<Result<toml::map::Map<_, _>, _>>()?;
        workbench.insert("layout_overrides".to_owned(), toml::Value::Table(layouts));
    }
    if !workbench.is_empty() {
        root.insert("workbench".to_owned(), toml::Value::Table(workbench));
    }
    Ok(())
}

fn insert_id_array(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    values: &[Id],
    explicitly_present: bool,
) {
    if explicitly_present || !values.is_empty() {
        table.insert(
            key.to_owned(),
            toml::Value::Array(
                values
                    .iter()
                    .map(|value| toml::Value::String(value.to_string()))
                    .collect(),
            ),
        );
    }
}

fn insert_owners(
    root: &mut toml::map::Map<String, toml::Value>,
    root_name: &str,
    values_name: &str,
    owners: &BTreeMap<Id, PublishedOwner>,
) -> Result<(), String> {
    if owners.is_empty() {
        return Ok(());
    }
    let mut table = toml::map::Map::new();
    for (owner_id, owner) in owners {
        let mut value = toml::map::Map::new();
        if let Some(enabled) = owner.enabled {
            value.insert("enabled".to_owned(), toml::Value::Boolean(enabled));
        }
        if let Some(version) = &owner.version {
            value.insert(
                "version".to_owned(),
                toml::Value::String(version.to_string()),
            );
        }
        if !owner.values.is_empty() {
            value.insert(values_name.to_owned(), typed_map_to_toml(&owner.values)?);
        }
        table.insert(owner_id.to_string(), toml::Value::Table(value));
    }
    root.insert(root_name.to_owned(), toml::Value::Table(table));
    Ok(())
}

fn insert_keymap(root: &mut toml::map::Map<String, toml::Value>, settings: &PublishedSettings) {
    if settings.keymap.is_empty() {
        return;
    }
    let owners = settings
        .keymap
        .iter()
        .map(|(owner_id, actions)| {
            let actions = actions
                .iter()
                .map(|(action_id, chords)| {
                    let chords = chords
                        .iter()
                        .map(|chord| toml::Value::String(chord.clone()))
                        .collect();
                    (action_id.to_string(), toml::Value::Array(chords))
                })
                .collect();
            (owner_id.to_string(), toml::Value::Table(actions))
        })
        .collect();
    root.insert("keymap".to_owned(), toml::Value::Table(owners));
}

fn typed_map_to_toml(values: &TypedMap) -> Result<toml::Value, String> {
    values
        .iter()
        .map(|(key, value)| typed_value_to_toml(value).map(|value| (key.to_string(), value)))
        .collect::<Result<toml::map::Map<_, _>, _>>()
        .map(toml::Value::Table)
}

fn typed_value_to_toml(value: &TypedValue) -> Result<toml::Value, String> {
    match value {
        TypedValue::String(value) => Ok(toml::Value::String(value.clone())),
        TypedValue::Bool(value) => Ok(toml::Value::Boolean(*value)),
        TypedValue::Integer(value) => Ok(toml::Value::Integer(*value)),
        TypedValue::Decimal(value) => value
            .as_str()
            .parse::<f64>()
            .map(toml::Value::Float)
            .map_err(|_| "cannot render canonical decimal".to_owned()),
        TypedValue::Datetime(value) => value
            .as_str()
            .parse::<toml::value::Datetime>()
            .map(toml::Value::Datetime)
            .map_err(|_| "cannot render canonical datetime".to_owned()),
        TypedValue::List(values) => values
            .iter()
            .map(typed_value_to_toml)
            .collect::<Result<Vec<_>, _>>()
            .map(toml::Value::Array),
        TypedValue::Map(values) => typed_map_to_toml(values),
        TypedValue::SecretRef(_) => Ok(toml::Value::String("<redacted>".to_owned())),
    }
}

fn append_provenance(output: &mut String, migration: &SettingsMigration, selected_path: &str) {
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("\n# provenance\n");
    append_appearance_provenance(output, migration, selected_path);
    append_owner_provenance(
        output,
        "agents",
        "repository_defaults",
        &migration.published().agents,
        selected_path,
    );
    append_owner_provenance(
        output,
        "plugins",
        "config",
        &migration.published().plugins,
        selected_path,
    );
    for dormant in &migration.published().dormant {
        let path = format!("/{}", dormant.path.join("/"));
        append_source_origin(
            output,
            &path,
            selected_path,
            dormant.span,
            "<dormant-redacted>",
        );
    }
}

fn append_appearance_provenance(
    output: &mut String,
    migration: &SettingsMigration,
    selected_path: &str,
) {
    let prefix: &[&str] = if migration.was_migrated() {
        &[]
    } else {
        &["appearance"]
    };
    for field in ["theme", "override_agent_theme"] {
        let mut source_path = prefix.to_vec();
        source_path.push(field);
        if let Some(node) = migration.document().node(&source_path) {
            append_source_origin(
                output,
                &format!("/appearance/{field}"),
                selected_path,
                Some(node.value_span),
                "selected_document",
            );
        }
    }
}

fn append_owner_provenance(
    output: &mut String,
    root_name: &str,
    values_name: &str,
    owners: &BTreeMap<Id, PublishedOwner>,
    selected_path: &str,
) {
    for (owner_id, owner) in owners {
        let mut leaves = Vec::new();
        collect_leaves(&owner.values, &mut Vec::new(), &mut leaves);
        for path in leaves {
            let semantic_path = path.iter().map(Id::as_str).collect::<Vec<_>>().join("/");
            let full_path = format!("/{root_name}/{owner_id}/{values_name}/{semantic_path}");
            for origin in owner.origins(&path) {
                match origin.kind {
                    ProvenanceKind::BuiltInDefault => {
                        let _ = writeln!(output, "# {full_path} built_in_default");
                    }
                    ProvenanceKind::SelectedDocument => append_source_origin(
                        output,
                        &full_path,
                        origin.canonical_path.as_deref().unwrap_or(selected_path),
                        origin.span,
                        "selected_document",
                    ),
                }
            }
        }
    }
}

fn collect_leaves(values: &TypedMap, prefix: &mut Vec<Id>, leaves: &mut Vec<Vec<Id>>) {
    for (key, value) in values {
        prefix.push(key.clone());
        if let TypedValue::Map(nested) = value {
            collect_leaves(nested, prefix, leaves);
        } else {
            leaves.push(prefix.clone());
        }
        let _ = prefix.pop();
    }
}

fn append_source_origin(
    output: &mut String,
    semantic_path: &str,
    source_path: &str,
    span: Option<crate::domain::ByteSpan>,
    kind: &str,
) {
    if let Some(span) = span {
        let _ = writeln!(
            output,
            "# {semantic_path} {kind} path={source_path} span={}..{}",
            span.start, span.end
        );
    } else {
        let _ = writeln!(output, "# {semantic_path} {kind} path={source_path}");
    }
}
