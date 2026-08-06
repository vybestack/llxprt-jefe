//! Manifest reader table (issue #389 CW-09, acceptance rows D4 and D5).

use super::*;
use crate::domain::plugin::{
    ActionConfirmation, ActionOutcome, EventKind, Field, FieldKind, ManifestError, ModelKind,
    ProviderMode, RestartScope, Scalar,
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
    "mode": "one-shot",
    "binaries": { "aarch64-apple-darwin": "bin/provider" }
  },
  "config": {
    "schema_version": 1,
    "fields": [
      {
        "id": "depth",
        "kind": "integer",
        "required": false,
        "default": 2,
        "minimum": 1,
        "maximum": 10,
        "restart": "provider"
      },
      {
        "id": "ratio",
        "kind": "finite-number",
        "required": false,
        "default": 0.5,
        "restart": "none"
      },
      {
        "id": "mode",
        "kind": "enum",
        "required": true,
        "choices": ["fast", "safe"],
        "default": "safe",
        "visible_when": "depth",
        "restart": "host"
      },
      { "id": "token", "kind": "secret-reference", "required": false, "restart": "none" }
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
        { "id": "branch", "kind": "string", "required": true, "restart": "none" }
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
      "event_kinds": ["selected", "field-changed"],
      "handler": "render-status",
      "ports": [{ "id": "rows" }]
    }
  ],
  "routes": [
    {
      "id": "vendor.git-merger.open",
      "activation_fields": [
        { "id": "sha", "kind": "string", "required": true, "restart": "none" }
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
    assert_eq!(manifest.provider().mode(), ProviderMode::OneShot);
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
    assert_eq!(
        panel.event_kinds(),
        [EventKind::Selected, EventKind::FieldChanged]
    );
    assert_eq!(panel.ports().len(), 1);
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
        Some(Scalar::Decimal(value)) => assert_eq!(value.as_str(), "0.5"),
        other => panic!("expected a decimal default, got {other:?}"),
    }
}

#[test]
fn an_unknown_field_is_rejected_at_every_level() {
    for (from, to) in [
        (r#""protocol": 1,"#, r#""protocol": 1, "extra": 1,"#),
        (
            r#""mode": "one-shot","#,
            r#""mode": "one-shot", "extra": 1,"#,
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
        (r#""mode": "one-shot""#, r#""mode": "one_shot""#),
        (r#""mode": "one-shot""#, r#""mode": "OneShot""#),
        (r#""kind": "finite-number""#, r#""kind": "finite_number""#),
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
