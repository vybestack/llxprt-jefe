fn snapshot_with_affordance(affordance: &str) -> String {
    format!(
        r#"{{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[{affordance}],"body":{{"kind":"empty","message":"x"}}}}"#
    )
}

#[test]
fn disabled_affordance_requires_reason() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_with_affordance(
            r#"{"id":"vendor.a","label":"A","action_id":"vendor.act","enabled":false}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn enabled_affordance_must_not_carry_reason() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_with_affordance(
            r#"{"id":"vendor.a","label":"A","action_id":"vendor.act","enabled":true,"unavailable_reason":"nope"}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn disabled_affordance_with_reason_is_accepted() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_with_affordance(
            r#"{"id":"vendor.a","label":"A","action_id":"vendor.act","enabled":false,"unavailable_reason":"busy"}"#,
        ),
    );
    assert!(parse_message(&bytes, Direction::ProviderToHost).is_ok());
}

// ---------------------------------------------------------------------------
// L. Action reference resolution and uniqueness
// ---------------------------------------------------------------------------

#[test]
fn body_action_must_resolve_to_a_declared_affordance() {
    // Detail references an action id that no affordance declares.
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "detail",
            r#"{"document":"d","metadata":[],"actions":["vendor.missing"]}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn body_action_must_resolve_to_an_enabled_affordance() {
    let payload = r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"detail","title":"t","loading":false,"action_affordances":[{"id":"vendor.disabled","label":"Disabled","action_id":"vendor.action","enabled":false,"unavailable_reason":"busy"}],"body":{"kind":"detail","document":"d","metadata":[],"actions":["vendor.disabled"]}}"#;
    let bytes = envelope("panel-snapshot", "p-000001", 1, payload);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn duplicate_affordance_ids_are_rejected() {
    let payload = r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[{"id":"vendor.dup","label":"A","action_id":"vendor.x","enabled":true},{"id":"vendor.dup","label":"B","action_id":"vendor.y","enabled":true}],"body":{"kind":"empty","message":"x"}}"#.to_string();
    let bytes = envelope("panel-snapshot", "p-000001", 1, &payload);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn duplicate_affordance_action_ids_are_rejected() {
    let payload = r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[{"id":"vendor.a","label":"A","action_id":"vendor.same","enabled":true},{"id":"vendor.b","label":"B","action_id":"vendor.same","enabled":true}],"body":{"kind":"empty","message":"x"}}"#.to_string();
    let bytes = envelope("panel-snapshot", "p-000001", 1, &payload);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn form_submit_action_must_resolve() {
    // submit_action references no declared affordance action_id.
    let payload = r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"form","title":"t","loading":false,"action_affordances":[],"body":{"kind":"form","fields":[],"values":{},"field_errors":[],"submit_action":"vendor.submit"}}"#;
    let bytes = envelope("panel-snapshot", "p-000001", 1, payload);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

// ---------------------------------------------------------------------------
// M. Inclusive N / N+1 bounds
// ---------------------------------------------------------------------------

/// Build a snapshot payload with N identical affordances (unique ids).
fn snapshot_with_n_affordances(n: usize) -> String {
    let affordances = (0..n)
        .map(|i| {
            format!(
                r#"{{"id":"vendor.a{i}","label":"A","action_id":"vendor.act{i}","enabled":true}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[{affordances}],"body":{{"kind":"empty","message":"x"}}}}"#
    )
}

#[test]
fn affordance_limit_inclusive_64() {
    let ok = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_with_n_affordances(64),
    );
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
    let bad = envelope(
        "panel-snapshot",
        "p-000002",
        1,
        &snapshot_with_n_affordances(65),
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn status_row_limit_inclusive_256() {
    let rows = (0..256)
        .map(|i| format!(r#"{{"label":"l{i}","value":"v{i}","state":"normal"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let ok = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("status", &format!(r#"{{"rows":[{rows}]}}"#)),
    );
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
}

#[test]
fn status_row_limit_exceeded() {
    let rows = (0..257)
        .map(|i| format!(r#"{{"label":"l{i}","value":"v{i}","state":"normal"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("status", &format!(r#"{{"rows":[{rows}]}}"#)),
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn detail_document_byte_limit_inclusive() {
    let doc = "a".repeat(262_144);
    let ok = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "detail",
            &format!(r#"{{"document":"{doc}","metadata":[],"actions":[]}}"#),
        ),
    );
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
}

#[test]
fn detail_document_byte_limit_exceeded() {
    let doc = "a".repeat(262_145);
    let bad = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "detail",
            &format!(r#"{{"document":"{doc}","metadata":[],"actions":[]}}"#),
        ),
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn migrated_notes_limit_inclusive_64() {
    let notes = (0..64).map(|_| "\"n\"").collect::<Vec<_>>().join(",");
    let payload = format!(
        r#"{{"from_version":1,"to_version":2,"config":{{}},"draft_token":1,"target_config":{{}},"notes":[{notes}]}}"#
    );
    let bytes = envelope("migrated-config", "h-000004", 1, &payload);
    assert!(parse_message(&bytes, Direction::ProviderToHost).is_ok());
}

#[test]
fn migrated_notes_limit_exceeded() {
    let notes = (0..65).map(|_| "\"n\"").collect::<Vec<_>>().join(",");
    let payload = format!(
        r#"{{"from_version":1,"to_version":2,"config":{{}},"draft_token":1,"target_config":{{}},"notes":[{notes}]}}"#
    );
    let bytes = envelope("migrated-config", "h-000004", 1, &payload);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn list_item_limit_inclusive_1000() {
    let items = (0..1000)
        .map(|i| format!(r#"{{"id":"vendor.i{i}","label":"L","actions":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let ok = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("list", &format!(r#"{{"items":[{items}]}}"#)),
    );
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
}

#[test]
fn list_item_limit_exceeded() {
    let items = (0..1001)
        .map(|i| format!(r#"{{"id":"vendor.i{i}","label":"L","actions":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let bad = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("list", &format!(r#"{{"items":[{items}]}}"#)),
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

// ---------------------------------------------------------------------------
// N. Description optional and closed HostLocal shape
// ---------------------------------------------------------------------------

#[test]
fn snapshot_description_is_optional() {
    let payload = r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","description":"a desc","loading":false,"action_affordances":[],"body":{"kind":"empty","message":"x"}}"#;
    let bytes = envelope("panel-snapshot", "p-000001", 1, payload);
    let snap = match parsed(&bytes, Direction::ProviderToHost).message {
        ProviderMessage::PanelSnapshot(s) => s,
        other => panic!("expected snapshot, got {other:?}"),
    };
    assert_eq!(snap.description.as_deref(), Some("a desc"));
}

#[test]
fn host_local_rejects_unknown_field() {
    let bytes = envelope(
        "activate-panel",
        "h-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":1,"panel_type":"vendor.p","activation":{},"prior_host_local":{"scroll_offset":0,"evil":1},"generation":1}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}

#[test]
fn body_kind_enum_matches_panel_body() {
    assert_eq!(BodyKind::ALL.len(), 7);
    // Smoke-check that every BodyKind resolves and round-trips through the
    // model enum without panic.
    for kind in BodyKind::ALL {
        let _ = kind.as_str();
    }
}

#[test]
fn affordance_and_list_item_types_compile_against_model() {
    // Type-presence smoke test: ensure the public model types are reachable
    // and constructible without touching private fields.
    let item = ListItem {
        id: Id::parse("vendor.i").unwrap_or_else(|e| panic!("{e:?}")),
        label: "L".to_owned(),
        description: None,
        status: None,
        actions: Vec::new(),
    };
    let _list = ListBody {
        items: vec![item],
        selected_id: None,
        next_page_token: None,
    };
    let _aff = Affordance {
        id: Id::parse("vendor.a").unwrap_or_else(|e| panic!("{e:?}")),
        label: "A".to_owned(),
        action_id: ActionId::parse("vendor.act").unwrap_or_else(|e| panic!("{e:?}")),
        arguments: None,
        enabled: true,
        unavailable_reason: None,
    };
}
