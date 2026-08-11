#[test]
fn snapshot_with_known_action_id_is_accepted() {
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.open"]);
    state.activate(outcome.instance).must("activate");
    let snap = snapshot_with_affordance(
        outcome.instance.as_u64(),
        state.generation(outcome.instance).must("gen"),
        "vendor.pkg.open",
        true,
        None,
    );
    accept(&mut state, outcome.instance, &snap, 0, 1).must("snapshot accepted");
    assert_eq!(
        state.lifecycle(outcome.instance),
        Some(PanelLifecycle::Active)
    );
}

#[test]
fn snapshot_with_foreign_action_id_is_rejected_without_partial_model() {
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.open"]);
    state.activate(outcome.instance).must("activate");
    let snap = snapshot_with_affordance(
        outcome.instance.as_u64(),
        state.generation(outcome.instance).must("gen"),
        "vendor.other.foreign",
        true,
        None,
    );
    let result = accept(&mut state, outcome.instance, &snap, 0, 1);
    assert_eq!(result, Err(PanelError::SnapshotInvalid));
    assert_eq!(
        state.lifecycle(outcome.instance),
        Some(PanelLifecycle::Failed)
    );
    assert!(state.accepted_snapshot(outcome.instance).is_none());
}

#[test]
fn snapshot_with_disabled_affordance_and_nonempty_reason_is_accepted() {
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.delete"]);
    state.activate(outcome.instance).must("activate");
    let snap = snapshot_with_affordance(
        outcome.instance.as_u64(),
        state.generation(outcome.instance).must("gen"),
        "vendor.pkg.delete",
        false,
        Some("read only"),
    );
    accept(&mut state, outcome.instance, &snap, 0, 1).must("snapshot accepted");
}

#[test]
fn snapshot_with_disabled_affordance_and_empty_reason_is_rejected() {
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.delete"]);
    state.activate(outcome.instance).must("activate");
    let snap = snapshot_with_affordance(
        outcome.instance.as_u64(),
        state.generation(outcome.instance).must("gen"),
        "vendor.pkg.delete",
        false,
        Some("   "),
    );
    let result = accept(&mut state, outcome.instance, &snap, 0, 1);
    assert_eq!(result, Err(PanelError::SnapshotInvalid));
}

#[test]
fn snapshot_with_disabled_affordance_and_missing_reason_is_rejected() {
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.delete"]);
    state.activate(outcome.instance).must("activate");
    let snap = snapshot_with_affordance(
        outcome.instance.as_u64(),
        state.generation(outcome.instance).must("gen"),
        "vendor.pkg.delete",
        false,
        None,
    );
    let result = accept(&mut state, outcome.instance, &snap, 0, 1);
    assert_eq!(result, Err(PanelError::SnapshotInvalid));
}

#[test]
fn snapshot_with_unknown_action_id_is_rejected_even_when_others_are_known() {
    use crate::runtime::provider::protocol::Affordance;
    let mut state = ProviderPanelState::new();
    let outcome = declare_with_authority(&mut state, &["vendor.pkg.known"]);
    state.activate(outcome.instance).must("activate");
    let generation = state.generation(outcome.instance).must("gen");
    let snap = PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: outcome.instance.as_u64(),
        generation,
        revision: 1,
        kind: BodyKind::Empty,
        title: "t".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![
            Affordance {
                id: id("ok"),
                label: "Known".to_owned(),
                action_id: crate::domain::action_registry::ActionId::parse("vendor.pkg.known")
                    .unwrap_or_else(|error| panic!("action id: {error:?}")),
                arguments: None,
                enabled: true,
                unavailable_reason: None,
            },
            Affordance {
                id: id("bad"),
                label: "Unknown".to_owned(),
                action_id: crate::domain::action_registry::ActionId::parse("vendor.pkg.unknown")
                    .unwrap_or_else(|error| panic!("action id: {error:?}")),
                arguments: None,
                enabled: true,
                unavailable_reason: None,
            },
        ],
        body: PanelBody::Empty(EmptyBody {
            message: "m".to_owned(),
            action: None,
        }),
    };
    let result = accept(&mut state, outcome.instance, &snap, 0, 1);
    assert_eq!(result, Err(PanelError::SnapshotInvalid));
    assert!(state.accepted_snapshot(outcome.instance).is_none());
}
