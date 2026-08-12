//! Lossless settings-document behavior tests.

use crate::domain::ByteSpan;

use super::diagnostic::{CfgCode, FILE_LIMIT};
use super::settings_document::SettingsDocument;
use super::sha256::Sha256;

#[test]
fn parse_retains_original_bytes_hash_comments_order_and_quoting() {
    let original = br#"# heading
settings_schema = 2

[appearance]
theme = 'green-screen' # trailing

[agents."core.llxprt"]
enabled = true
"#;
    let Ok(document) = SettingsDocument::parse(original) else {
        panic!("valid settings document must parse");
    };

    assert_eq!(document.original_bytes(), original);
    assert_eq!(document.sha256(), Sha256::digest(original));
    assert_eq!(document.comment_spans().len(), 2);

    let theme_path = ["appearance", "theme"];
    let Some(theme) = document.node(&theme_path) else {
        panic!("theme assignment must have a syntax node");
    };
    assert_eq!(document.span_bytes(theme.value_span), b"'green-screen'");
    assert_eq!(theme.path, theme_path);

    let owner_path = ["agents", "core.llxprt", "enabled"];
    let Some(enabled) = document.node(&owner_path) else {
        panic!("quoted owner assignment must have a syntax node");
    };
    assert_eq!(document.span_bytes(enabled.value_span), b"true");
}

#[test]
fn parser_accepts_multiline_values_without_losing_statement_spans() {
    let original = br#"settings_schema = 2
[appearance]
theme = """
green
screen
"""
[extensions.future]
values = [
  "one", # inside array
  "two",
]
"#;
    let Ok(document) = SettingsDocument::parse(original) else {
        panic!("multiline TOML must parse");
    };
    let Some(theme) = document.node(&["appearance", "theme"]) else {
        panic!("multiline assignment must be indexed");
    };
    let value = document.span_bytes(theme.value_span);
    assert!(value.starts_with(b"\"\"\""));
    assert!(value.ends_with(b"\"\"\""));
    assert_eq!(document.original_bytes(), original);
}

#[test]
fn malformed_toml_is_cfg_e002_with_source_span() {
    let diagnostics = SettingsDocument::parse(b"settings_schema = [")
        .err()
        .unwrap_or_else(|| panic!("malformed TOML must fail"));
    assert_eq!(diagnostics.code, CfgCode::E002);
    assert!(diagnostics.span.is_some());
}

#[test]
fn file_bound_is_inclusive_and_owned_by_settings_parser() {
    let at_limit = vec![b' '; FILE_LIMIT];
    let Ok(document) = SettingsDocument::parse(&at_limit) else {
        panic!("file exactly at the inclusive limit must parse");
    };
    assert_eq!(document.original_bytes().len(), FILE_LIMIT);

    let over_limit = vec![b' '; FILE_LIMIT + 1];
    let diagnostic = SettingsDocument::parse(&over_limit)
        .err()
        .unwrap_or_else(|| panic!("file over limit must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);
    assert_eq!(
        diagnostic.span,
        Some(ByteSpan::new(0, (FILE_LIMIT + 1) as u64))
    );
}

#[test]
fn string_array_map_and_depth_bounds_are_rejected_by_one() {
    let long_string = "x".repeat(super::diagnostic::STRING_LIMIT + 1);
    let input = format!("settings_schema = 2\n[extensions]\nvalue = \"{long_string}\"\n");
    let diagnostic = SettingsDocument::parse(input.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("overlong string must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);

    let nested = "[".repeat(super::diagnostic::NESTING_LIMIT);
    let closed = "]".repeat(super::diagnostic::NESTING_LIMIT);
    let input = format!("settings_schema = 2\n[extensions]\nvalue = {nested}0{closed}\n");
    let diagnostic = SettingsDocument::parse(input.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("depth over limit must fail"));
    assert_eq!(diagnostic.code, CfgCode::E008);
}

fn known_agent_catalog() -> (crate::domain::OwnerCatalog, crate::domain::Id) {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::domain::{
        CanonicalSemver, Id, OwnerCatalog, OwnerDescriptor, OwnerKind, TypedValue,
    };

    let owner_id = Id::parse("core.llxprt").unwrap_or_else(|error| panic!("owner id: {error:?}"));
    let profile = Id::parse("profile").unwrap_or_else(|error| panic!("profile id: {error:?}"));
    let nested = Id::parse("nested").unwrap_or_else(|error| panic!("nested id: {error:?}"));
    let left = Id::parse("left").unwrap_or_else(|error| panic!("left id: {error:?}"));
    let right = Id::parse("right").unwrap_or_else(|error| panic!("right id: {error:?}"));
    let mut nested_defaults = BTreeMap::new();
    nested_defaults.insert(left, TypedValue::Integer(1));
    nested_defaults.insert(right, TypedValue::Integer(2));
    let mut defaults = BTreeMap::new();
    defaults.insert(profile, TypedValue::String("default".to_owned()));
    defaults.insert(nested, TypedValue::Map(nested_defaults));
    let version = CanonicalSemver::parse("1.0.0")
        .unwrap_or_else(|error| panic!("version fixture: {error:?}"));
    let mut catalog = OwnerCatalog::default();
    assert!(
        catalog
            .insert(OwnerDescriptor {
                owner_id: owner_id.clone(),
                version,
                kind: OwnerKind::Agent,
                defaults,
                secret_paths: BTreeSet::new(),
            })
            .is_ok()
    );
    (catalog, owner_id)
}

#[test]
fn publisher_merges_known_owner_defaults_and_records_leaf_provenance() {
    use crate::domain::{Id, ProvenanceKind, TypedValue};

    let (catalog, owner_id) = known_agent_catalog();
    let profile = Id::parse("profile").unwrap_or_else(|error| panic!("profile id: {error:?}"));
    let nested = Id::parse("nested").unwrap_or_else(|error| panic!("nested id: {error:?}"));
    let left = Id::parse("left").unwrap_or_else(|error| panic!("left id: {error:?}"));
    let right = Id::parse("right").unwrap_or_else(|error| panic!("right id: {error:?}"));
    let source = br#"settings_schema = 2
[agents."core.llxprt"]
enabled = true
repository_defaults = { profile = "custom", nested = { left = 9 } }
"#;
    let Ok(document) = SettingsDocument::parse(source) else {
        panic!("settings fixture must parse");
    };
    let Ok(published) = document.publish(&catalog) else {
        panic!("known owner must publish");
    };
    let Some(agent) = published.agents.get(&owner_id) else {
        panic!("known agent owner must be present");
    };
    assert_eq!(agent.enabled, Some(true));
    assert_eq!(
        agent.values.get(&profile),
        Some(&TypedValue::String("custom".to_owned()))
    );
    let Some(TypedValue::Map(nested_values)) = agent.values.get(&nested) else {
        panic!("nested defaults must stay typed");
    };
    assert_eq!(nested_values.get(&left), Some(&TypedValue::Integer(9)));
    assert_eq!(nested_values.get(&right), Some(&TypedValue::Integer(2)));
    let origins = agent.origins(&[profile]);
    assert_eq!(origins.len(), 2);
    assert_eq!(origins[0].kind, ProvenanceKind::BuiltInDefault);
    assert_eq!(origins[1].kind, ProvenanceKind::SelectedDocument);
}

#[test]
fn publisher_keeps_unknown_owners_and_extensions_dormant() {
    use crate::domain::OwnerCatalog;

    let source = br#"settings_schema = 2
[agents."future.agent"]
enabled = true
future_field = "untouched"
[extensions."future.plugin"]
secret_material = "never-published"
"#;
    let Ok(document) = SettingsDocument::parse(source) else {
        panic!("settings fixture must parse");
    };
    let Ok(published) = document.publish(&OwnerCatalog::default()) else {
        panic!("unknown owners must remain dormant rather than fail");
    };
    assert!(published.agents.is_empty());
    assert!(published.plugins.is_empty());
    assert_eq!(published.dormant.len(), 2);
    assert!(
        published
            .dormant
            .iter()
            .any(|entry| entry.path == ["agents", "future.agent"])
    );
    assert!(
        published
            .dormant
            .iter()
            .any(|entry| entry.path == ["extensions"])
    );
    assert_eq!(document.original_bytes(), source);
}

#[test]
fn publisher_rejects_unknown_fields_for_active_known_owner() {
    use std::collections::BTreeSet;

    use crate::domain::{CanonicalSemver, Id, OwnerCatalog, OwnerDescriptor, OwnerKind};

    let Ok(owner_id) = Id::parse("core.llxprt") else {
        panic!("owner id fixture must be valid");
    };
    let Ok(version) = CanonicalSemver::parse("1.0.0") else {
        panic!("version fixture must be valid");
    };
    let mut catalog = OwnerCatalog::default();
    assert!(
        catalog
            .insert(OwnerDescriptor {
                owner_id,
                version,
                kind: OwnerKind::Agent,
                defaults: std::collections::BTreeMap::default(),
                secret_paths: BTreeSet::new(),
            })
            .is_ok()
    );
    let source = br#"settings_schema = 2
[agents."core.llxprt"]
unknown = true
"#;
    let Ok(document) = SettingsDocument::parse(source) else {
        panic!("settings fixture must parse syntactically");
    };
    let diagnostics = document
        .publish(&catalog)
        .err()
        .unwrap_or_else(|| panic!("active owner unknown field must fail"));
    assert_eq!(diagnostics[0].code, CfgCode::E005);
}

fn screen_plugin_catalog() -> crate::domain::OwnerCatalog {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::domain::{CanonicalSemver, Id, OwnerCatalog, OwnerDescriptor, OwnerKind};

    let mut catalog = OwnerCatalog::default();
    for (name, kind) in [
        ("core.dashboard", OwnerKind::Screen),
        ("vendor.plugin", OwnerKind::Plugin),
    ] {
        let owner_id = Id::parse(name).unwrap_or_else(|error| panic!("owner fixture: {error:?}"));
        let version = CanonicalSemver::parse("1.2.3")
            .unwrap_or_else(|error| panic!("version fixture: {error:?}"));
        assert!(
            catalog
                .insert(OwnerDescriptor {
                    owner_id,
                    version,
                    kind,
                    defaults: BTreeMap::default(),
                    secret_paths: BTreeSet::new(),
                })
                .is_ok()
        );
    }
    catalog
}

#[test]
fn publisher_validates_all_closed_roots_and_rejects_unknown_root() {
    let catalog = screen_plugin_catalog();
    let source = br#"settings_schema = 2
[appearance]
theme = "green-screen"
override_agent_theme = true
[workbench]
initial_screen = "core.dashboard"
enabled_screens = ["core.dashboard"]
screen_order = ["core.dashboard"]
[workbench.layout_overrides."core.dashboard"]
width = 80
[keymap."core.dashboard"]
open = ["g", "d"]
[plugins."vendor.plugin"]
enabled = true
version = "1.2.3"
config = { retries = 3, credential = { env = "GITHUB_TOKEN" } }
"#;
    let Ok(document) = SettingsDocument::parse(source) else {
        panic!("settings fixture must parse");
    };
    let Ok(published) = document.publish(&catalog) else {
        panic!("closed settings roots must publish");
    };
    assert_eq!(published.appearance.theme.as_deref(), Some("green-screen"));
    assert_eq!(published.workbench.enabled_screens.len(), 1);
    assert_eq!(published.keymap.len(), 1);
    assert_eq!(published.plugins.len(), 1);
    let credential = published
        .plugins
        .get(
            &crate::domain::Id::parse("vendor.plugin")
                .unwrap_or_else(|error| panic!("plugin id fixture must parse: {error}")),
        )
        .and_then(|plugin| {
            plugin.values.get(
                &crate::domain::Id::parse("credential")
                    .unwrap_or_else(|error| panic!("credential id fixture must parse: {error}")),
            )
        });
    assert!(matches!(
        credential,
        Some(crate::domain::TypedValue::SecretRef(reference))
            if reference.env.env() == "GITHUB_TOKEN"
    ));

    let unknown = b"settings_schema = 2\n[unknown]\nvalue = true\n";
    let Ok(document) = SettingsDocument::parse(unknown) else {
        panic!("unknown-root fixture must parse syntactically");
    };
    let diagnostics = document
        .publish(&catalog)
        .err()
        .unwrap_or_else(|| panic!("unknown root must fail"));
    assert_eq!(diagnostics[0].code, CfgCode::E005);
}

#[test]
fn publisher_rejects_legacy_secret_reference_key() {
    let catalog = screen_plugin_catalog();
    let source = br#"settings_schema = 2
[plugins."vendor.plugin"]
enabled = true
version = "1.2.3"
config = { credential = { secret_ref = "github.token" } }
"#;
    let document = SettingsDocument::parse(source)
        .unwrap_or_else(|error| panic!("settings fixture must parse: {error:?}"));
    let diagnostics = document
        .publish(&catalog)
        .err()
        .unwrap_or_else(|| panic!("legacy secret reference must fail"));
    assert_eq!(diagnostics[0].code, CfgCode::E003);
}

#[test]
fn publisher_rejects_invalid_secret_environment_name() {
    let catalog = screen_plugin_catalog();
    let source = br#"settings_schema = 2
[plugins."vendor.plugin"]
enabled = true
version = "1.2.3"
config = { credential = { env = "github-token" } }
"#;
    let document = SettingsDocument::parse(source)
        .unwrap_or_else(|error| panic!("settings fixture must parse: {error:?}"));
    let diagnostics = document
        .publish(&catalog)
        .err()
        .unwrap_or_else(|| panic!("invalid environment name must fail"));
    assert_eq!(diagnostics[0].code, CfgCode::E003);
}

#[test]
fn keymap_publishes_context_action_lists_without_config_owner_catalog() {
    let source = br#"settings_schema = 2
[keymap."issues.inline"]
"issues.submit-inline" = ["Ctrl+Enter"]
"issues.cancel-inline" = []
[extensions.future]
opaque = "retained"
"#;
    let Ok(document) = SettingsDocument::parse(source) else {
        panic!("keymap fixture must parse");
    };
    let Ok(published) = document.publish(&crate::domain::OwnerCatalog::default()) else {
        panic!("context/action keymap must publish without owner descriptors");
    };

    let inline = published
        .keymap
        .get("issues.inline")
        .unwrap_or_else(|| panic!("known context must publish"));
    assert_eq!(
        inline.get("issues.submit-inline"),
        Some(&vec!["Ctrl+Enter".to_owned()])
    );
    assert_eq!(inline.get("issues.cancel-inline"), Some(&Vec::new()));
    assert!(
        published
            .dormant
            .iter()
            .any(|entry| entry.path == ["extensions"])
    );
    assert_eq!(document.original_bytes(), source);
}

/// The agent field allow-list is exactly `enabled` and `repository_defaults`.
/// `parse_owner_version` returns early for non-plugin owners, so an agent
/// carrying a `version` key is unowned input and must be refused rather than
/// silently accepted.
#[test]
fn agent_owners_reject_a_version_field_and_accept_their_owned_fields() {
    let (catalog, owner_id) = known_agent_catalog();
    let versioned = br#"settings_schema = 2
[agents."core.llxprt"]
enabled = true
version = "1.0.0"
"#;
    let Ok(document) = SettingsDocument::parse(versioned) else {
        panic!("settings fixture must parse");
    };
    let Err(diagnostics) = document.publish(&catalog) else {
        panic!("an agent version field must not be publishable");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.as_str().ends_with("/version")),
        "the diagnostic must name the unowned version field: {diagnostics:?}"
    );

    let owned = br#"settings_schema = 2
[agents."core.llxprt"]
enabled = true
repository_defaults = { profile = "custom" }
"#;
    let Ok(document) = SettingsDocument::parse(owned) else {
        panic!("settings fixture must parse");
    };
    let Ok(published) = document.publish(&catalog) else {
        panic!("owned agent fields must publish");
    };
    assert!(
        published.agents.contains_key(&owner_id),
        "the agent owner must be published"
    );
}
