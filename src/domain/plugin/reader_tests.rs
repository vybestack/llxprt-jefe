//! Manifest reader table (issue #389 CW-09, acceptance rows D4 and D5).

use super::*;
use crate::domain::plugin::{
    ActionConfirmation, ActionOutcome, EventKind, EventSchemaEntry, Field, FieldKind,
    ManifestError, ModelKind, ProviderMode, RestartScope,
};

/// A manifest carrying one instance of every closed field.
const COMPLETE: &str = r#"{
  "manifest_schema": 1,
  "id": "vendor.git-merger",
  "version": "1.0.0",
  "display_name": "Git Merger",
  "host_api": { "minimum": "1.0.0", "maximum": "2.0.0" },
  "protocol": 1,
  "provider": {
    "mode": "persistent",
    "binaries": { "aarch64-apple-darwin": "bin/provider" }
  },
  "config": {
    "schema_version": 1,
    "fields": [
      {
        "id": "depth",
        "label": "Depth",
        "type": "integer",
        "required": false,
        "default": 2,
        "min": 1,
        "max": 10,
        "restart": "provider"
      },
      {
        "id": "ratio",
        "label": "Ratio",
        "type": "finite-number",
        "required": false,
        "default": 0.5,
        "restart": "none"
      },
      {
        "id": "mode",
        "label": "Mode",
        "description": "Merge strategy",
        "type": "enum",
        "required": true,
        "choices": ["fast", "safe"],
        "default": "safe",
        "visible_when": "depth",
        "restart": "host"
      },
      { "id": "token", "label": "Token", "type": "secret-reference", "required": false, "restart": "none" }
    ]
  },
  "actions": [
    {
      "id": "vendor.git-merger.merge",
      "label": "Merge",
      "description": "Merge the branch",
      "category": "git",
      "contexts": ["core.dashboard"],
      "arguments": [
        { "id": "branch", "label": "Branch", "type": "string", "required": true, "restart": "none" }
      ],
      "timeout_seconds": 120,
      "destructive": true,
      "confirmation": "host-before-invoke",
      "handler": "merge",
      "allowed_outcomes": ["notice", "refresh-current-resource"]
    }
  ],
  "panels": [
    {
      "id": "vendor.git-merger.status",
      "model_kinds": ["status", "error"],
      "event_schema": [
        { "kind": "selected", "arguments": [] },
        { "kind": "field-changed", "arguments": [] }
      ],
      "handler": "render-status",
      "ports": [{ "id": "rows" }]
    }
  ],
  "routes": [
    {
      "id": "vendor.git-merger.open",
      "activation_fields": [
        { "id": "sha", "label": "SHA", "type": "string", "required": true, "restart": "none" }
      ],
      "target_screen": "vendor.git-merger.main"
    }
  ],
  "screens": [
    { "path": "screens/main.json", "screen_ids": ["vendor.git-merger.main"] }
  ],
  "defaults": {
    "actions_enabled": ["vendor.git-merger.merge"],
    "screens_enabled": ["vendor.git-merger.main"],
    "config": { "depth": 3 }
  }
}"#;

fn read(text: &str) -> Result<Manifest, ManifestReadError> {
    read_manifest(text.as_bytes())
}

fn parsed(text: &str) -> Manifest {
    read(text).unwrap_or_else(|error| panic!("manifest must parse: {error}"))
}

fn rejected(text: &str) -> ManifestReadError {
    read(text)
        .err()
        .unwrap_or_else(|| panic!("manifest must be rejected"))
}

/// Replace the first occurrence of `from` with `to` in the complete manifest.
fn mutated(from: &str, to: &str) -> String {
    assert!(COMPLETE.contains(from), "fixture must contain {from:?}");
    COMPLETE.replacen(from, to, 1)
}

#[test]
fn a_complete_manifest_lowers_its_identity_and_provider() {
    let manifest = parsed(COMPLETE);
    assert_eq!(manifest.id().as_str(), "vendor.git-merger");
    assert_eq!(manifest.version().as_str(), "1.0.0");
    assert_eq!(manifest.display_name(), "Git Merger");
    assert_eq!(manifest.provider().mode(), ProviderMode::Persistent);
    assert_eq!(manifest.provider().binaries().len(), 1);
}

#[test]
fn a_complete_manifest_lowers_its_config_schema() {
    let manifest = parsed(COMPLETE);
    let config = manifest
        .config()
        .unwrap_or_else(|| panic!("config must be present"));
    assert_eq!(config.schema_version(), 1);
    assert_eq!(config.fields().len(), 4);
}

#[test]
fn a_complete_manifest_lowers_its_action() {
    let manifest = parsed(COMPLETE);
    let action = manifest
        .actions()
        .first()
        .unwrap_or_else(|| panic!("action must be present"));
    assert_eq!(action.id().as_str(), "vendor.git-merger.merge");
    assert_eq!(action.timeout_seconds(), 120);
    assert!(action.destructive());
    assert_eq!(action.confirmation(), ActionConfirmation::HostBeforeInvoke);
    assert_eq!(
        action.allowed_outcomes(),
        [ActionOutcome::Notice, ActionOutcome::RefreshCurrentResource]
    );
    assert_eq!(action.arguments().len(), 1);
}

#[test]
fn a_complete_manifest_lowers_its_panel() {
    let manifest = parsed(COMPLETE);
    let panel = manifest
        .panels()
        .first()
        .unwrap_or_else(|| panic!("panel must be present"));
    assert_eq!(panel.model_kinds(), [ModelKind::Status, ModelKind::Error]);
    let kinds: Vec<EventKind> = panel
        .event_schema()
        .iter()
        .map(EventSchemaEntry::kind)
        .collect();
    assert_eq!(kinds, [EventKind::Selected, EventKind::FieldChanged]);
    assert_eq!(panel.ports().len(), 1);
}

#[test]
fn panel_contract_accepts_exact_nine_model_kinds_and_expansion_event() {
    let text = mutated(
        r#""model_kinds": ["status", "error"]"#,
        r#""model_kinds": ["list", "tree", "detail", "structured-diff", "form", "status", "progress", "empty", "error"]"#,
    );
    let text = text.replacen(
        r#"{ "kind": "field-changed", "arguments": [] }"#,
        r#"{ "kind": "expansion-changed", "arguments": [] }"#,
        1,
    );
    let manifest = parsed(&text);
    let panel = &manifest.panels()[0];
    assert_eq!(panel.model_kinds(), ModelKind::ALL);
    assert_eq!(panel.event_schema()[1].kind(), EventKind::ExpansionChanged);

    let rejected_terminal = mutated(
        r#""model_kinds": ["status", "error"]"#,
        r#""model_kinds": ["terminal"]"#,
    );
    assert!(matches!(
        rejected(&rejected_terminal),
        ManifestReadError::UnknownValue { .. }
    ));
}

#[test]
fn a_complete_manifest_lowers_its_route_and_screen() {
    let manifest = parsed(COMPLETE);
    let route = manifest
        .routes()
        .first()
        .unwrap_or_else(|| panic!("route must be present"));
    assert_eq!(route.target_screen().as_str(), "vendor.git-merger.main");
    assert_eq!(route.activation_fields().len(), 1);

    let screen = manifest
        .screens()
        .first()
        .unwrap_or_else(|| panic!("screen must be present"));
    assert_eq!(screen.path().as_str(), "screens/main.json");
}

#[test]
fn a_complete_manifest_lowers_its_defaults() {
    let manifest = parsed(COMPLETE);
    let defaults = manifest
        .defaults()
        .unwrap_or_else(|| panic!("defaults must be present"));
    assert_eq!(defaults.actions_enabled.len(), 1);
    assert_eq!(defaults.screens_enabled.len(), 1);
    assert_eq!(defaults.config.len(), 1);
}

#[test]
fn every_field_kind_and_restart_scope_lowers() {
    let manifest = parsed(COMPLETE);
    let config = manifest
        .config()
        .unwrap_or_else(|| panic!("config must be present"));
    let kinds: Vec<FieldKind> = config.fields().iter().map(Field::kind).collect();
    assert_eq!(
        kinds,
        vec![
            FieldKind::Integer,
            FieldKind::FiniteNumber,
            FieldKind::Enum,
            FieldKind::SecretReference,
        ]
    );
    let scopes: Vec<RestartScope> = config.fields().iter().map(Field::restart).collect();
    assert_eq!(
        scopes,
        vec![
            RestartScope::Provider,
            RestartScope::None,
            RestartScope::Host,
            RestartScope::None,
        ]
    );
}

#[test]
fn a_finite_number_default_keeps_its_canonical_text() {
    let manifest = parsed(COMPLETE);
    let config = manifest
        .config()
        .unwrap_or_else(|| panic!("config must be present"));
    let ratio = config
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "ratio")
        .unwrap_or_else(|| panic!("ratio must be present"));
    match ratio.default() {
        Some(crate::domain::TypedValue::Decimal(value)) => assert_eq!(value.as_str(), "0.5"),
        other => panic!("expected a decimal default, got {other:?}"),
    }
}

#[test]
fn an_unknown_field_is_rejected_at_every_level() {
    for (from, to) in [
        (r#""protocol": 1,"#, r#""protocol": 1, "extra": 1,"#),
        (
            r#""mode": "persistent","#,
            r#""mode": "persistent", "extra": 1,"#,
        ),
        (r#""id": "depth","#, r#""id": "depth", "extra": 1,"#),
        (r#""label": "Merge","#, r#""label": "Merge", "extra": 1,"#),
        (
            r#""handler": "render-status","#,
            r#""handler": "render-status", "extra": 1,"#,
        ),
        (
            r#""path": "screens/main.json","#,
            r#""path": "screens/main.json", "extra": 1,"#,
        ),
    ] {
        let error = rejected(&mutated(from, to));
        assert!(
            matches!(error, ManifestReadError::UnknownField { .. }),
            "an unknown field must be rejected, got {error}"
        );
    }
}

#[test]
fn old_grammar_keys_are_rejected_as_unknown_fields() {
    // The cutover is hard: the old wire names are not aliases.
    for (from, to) in [
        // Field kind → type
        (r#""type": "integer""#, r#""kind": "integer""#),
        // minimum → min
        (r#""min": 1,"#, r#""minimum": 1,"#),
        // maximum → max
        (r#""max": 10,"#, r#""maximum": 10,"#),
        // event_kinds → event_schema
        (r#""event_schema": ["#, r#""event_kinds": ["#),
    ] {
        let error = rejected(&mutated(from, to));
        assert!(
            matches!(error, ManifestReadError::UnknownField { .. }),
            "the old key {to:?} must be rejected, got {error}"
        );
    }
}
#[test]
fn a_duplicate_key_is_rejected() {
    let error = rejected(&mutated(
        r#""protocol": 1,"#,
        r#""protocol": 1, "protocol": 1,"#,
    ));
    assert!(
        matches!(error, ManifestReadError::Json(_)),
        "a duplicate key must be rejected by the bounded reader, got {error}"
    );
}

#[test]
fn a_missing_required_field_is_rejected() {
    let error = rejected(&mutated(r#""protocol": 1,"#, ""));
    assert_eq!(
        error,
        ManifestReadError::MissingField {
            path: "manifest".to_owned(),
            field: "protocol".to_owned()
        }
    );
}

#[test]
fn a_wrong_case_or_snake_case_enum_spelling_is_rejected() {
    for (from, to) in [
        (r#""mode": "persistent""#, r#""mode": "persistent_""#),
        (r#""mode": "persistent""#, r#""mode": "Persistent""#),
        (r#""type": "finite-number""#, r#""type": "finite_number""#),
        (
            r#""confirmation": "host-before-invoke""#,
            r#""confirmation": "HostBeforeInvoke""#,
        ),
        (r#""field-changed""#, r#""fieldChanged""#),
        (r#""restart": "provider""#, r#""restart": "Provider""#),
    ] {
        let error = rejected(&mutated(from, to));
        assert!(
            matches!(error, ManifestReadError::UnknownValue { .. }),
            "{to} must be rejected, got {error}"
        );
    }
}

#[test]
fn a_value_of_the_wrong_json_type_is_rejected() {
    for (from, to) in [
        (r#""protocol": 1"#, r#""protocol": "1""#),
        (r#""display_name": "Git Merger""#, r#""display_name": 1"#),
        (r#""destructive": true"#, r#""destructive": "true""#),
        (
            r#""contexts": ["core.dashboard"]"#,
            r#""contexts": "core.dashboard""#,
        ),
    ] {
        let error = rejected(&mutated(from, to));
        assert!(
            matches!(error, ManifestReadError::TypeMismatch { .. }),
            "{to} must be rejected, got {error}"
        );
    }
}

#[test]
fn an_invalid_value_type_reports_its_own_reason() {
    for (from, to) in [
        (r#""id": "vendor.git-merger""#, r#""id": "core.dashboard""#),
        (r#""version": "1.0.0""#, r#""version": "v1.0.0""#),
        (
            r#""aarch64-apple-darwin": "bin/provider""#,
            r#""aarch64-apple-darwin": "/abs/provider""#,
        ),
        // One component cannot be a triple. Note that a plausible-looking
        // `not-a-triple` *is* a well-formed triple by grammar, so the fixture
        // uses a shape the grammar genuinely rejects.
        (
            r#""aarch64-apple-darwin": "bin/provider""#,
            r#""nope": "bin/provider""#,
        ),
    ] {
        let error = rejected(&mutated(from, to));
        assert!(
            matches!(error, ManifestReadError::InvalidValue { .. }),
            "{to} must be rejected, got {error}"
        );
    }
}

#[test]
fn a_cross_declaration_failure_surfaces_as_a_manifest_error() {
    let error = rejected(&mutated(
        r#""target_screen": "vendor.git-merger.main""#,
        r#""target_screen": "vendor.git-merger.absent""#,
    ));
    assert!(
        matches!(
            error,
            ManifestReadError::Manifest(ManifestError::UnresolvedRouteTarget { .. })
        ),
        "expected a manifest validation error, got {error}"
    );
}

#[test]
fn optional_sections_may_be_omitted() {
    let minimal = r#"{
      "manifest_schema": 1,
      "id": "vendor.pkg",
      "version": "1.0.0",
      "display_name": "Pkg",
      "host_api": { "minimum": "1.0.0", "maximum": "1.0.0" },
      "protocol": 1,
      "provider": { "mode": "none", "binaries": {} },
      "actions": [],
      "panels": [],
      "routes": [],
      "screens": []
    }"#;
    let manifest = parsed(minimal);
    assert!(manifest.config().is_none());
    assert!(manifest.defaults().is_none());
    assert!(manifest.actions().is_empty());
}

#[test]
fn a_manifest_over_the_byte_bound_is_rejected() {
    let padding = " ".repeat(crate::domain::plugin::limits::MANIFEST_BYTE_LIMIT);
    let error = rejected(&format!("{COMPLETE}{padding}"));
    assert!(
        matches!(error, ManifestReadError::Json(_)),
        "an oversize manifest must be rejected, got {error}"
    );
}

#[test]
fn a_non_object_document_is_rejected() {
    for text in ["[]", "1", "\"x\"", "null", "true"] {
        let error = rejected(text);
        assert!(
            matches!(error, ManifestReadError::TypeMismatch { .. }),
            "{text} must be rejected, got {error}"
        );
    }
}

#[test]
fn a_string_list_default_lowers_as_a_typed_list() {
    let json = COMPLETE.replace(
        r#""id": "token", "label": "Token", "type": "secret-reference", "required": false, "restart": "none" }"#,
        r#""id": "token", "label": "Token", "type": "secret-reference", "required": false, "default": {"env": "API_TOKEN"}, "restart": "none" }, {"id": "tags", "label": "Tags", "type": "string-list", "required": false, "default": ["alpha", "beta"], "restart": "none" }"#,
    );
    let manifest = parsed(&json);
    let config = manifest
        .config()
        .unwrap_or_else(|| panic!("config must be present"));
    let token = config
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "token")
        .unwrap_or_else(|| panic!("token must be present"));
    match token.default() {
        Some(crate::domain::TypedValue::SecretRef(reference)) => {
            assert_eq!(reference.env.env(), "API_TOKEN");
        }
        other => panic!("expected a SecretRef default, got {other:?}"),
    }
    let tags = config
        .fields()
        .iter()
        .find(|field| field.id().as_str() == "tags")
        .unwrap_or_else(|| panic!("tags must be present"));
    match tags.default() {
        Some(crate::domain::TypedValue::List(values)) => {
            assert_eq!(values.len(), 2);
        }
        other => panic!("expected a List default, got {other:?}"),
    }
}

#[test]
fn package_config_defaults_use_the_declared_field_types() {
    let json = COMPLETE
        .replace(
            r#"{ "id": "token", "label": "Token", "type": "secret-reference", "required": false, "restart": "none" }"#,
            r#"{ "id": "token", "label": "Token", "type": "secret-reference", "required": false, "restart": "none" }, { "id": "tags", "label": "Tags", "type": "string-list", "required": false, "restart": "none" }"#,
        )
        .replace(
            r#""config": { "depth": 3 }"#,
            r#""config": { "depth": 3, "token": {"env": "API_TOKEN"}, "tags": ["alpha", "beta"] }"#,
        );
    let manifest = parsed(&json);
    let defaults = manifest
        .defaults()
        .unwrap_or_else(|| panic!("defaults must be present"));
    assert!(matches!(
        defaults
            .config
            .iter()
            .find(|(id, _)| id.as_str() == "token")
            .map(|(_, value)| value),
        Some(crate::domain::TypedValue::SecretRef(_))
    ));
    assert!(matches!(
        defaults
            .config
            .iter()
            .find(|(id, _)| id.as_str() == "tags")
            .map(|(_, value)| value),
        Some(crate::domain::TypedValue::List(values)) if values.len() == 2
    ));
}

#[test]
fn a_wrong_type_default_is_rejected_by_the_reader() {
    let json = COMPLETE.replace(
        "\"default\": 2,\n        \"min\": 1",
        "\"default\": \"not-a-number\",\n        \"min\": 1",
    );
    let error = rejected(&json);
    assert!(
        matches!(error, ManifestReadError::Declaration { .. }),
        "a wrong-type default must be rejected, got {error}"
    );
}

#[test]
fn a_legacy_field_key_is_rejected() {
    let json = COMPLETE.replace(
        "\"max\": 10,\n        \"restart\": \"provider\"",
        "\"max\": 10,\n        \"minimum\": 1,\n        \"restart\": \"provider\"",
    );
    let error = rejected(&json);
    assert!(
        matches!(error, ManifestReadError::UnknownField { .. }),
        "a legacy key like 'minimum' must be rejected, got {error}"
    );
}
