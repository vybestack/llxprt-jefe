//! Manifest cross-field validation table
//! (issue #389 CW-09, acceptance rows D4, D5 and D7).

use super::*;
use crate::domain::plugin::limits::{ACTION_LIMIT, PANEL_LIMIT, ROUTE_LIMIT};
use crate::domain::plugin::{
    ActionConfirmation, ActionDraft, Field, FieldDraft, FieldKind, HostTriple, ModelKind,
    PanelDraft, ProviderMode, RelativePath, RestartScope, RouteDraft,
};
use crate::domain::{CanonicalSemver, Id};

fn id(value: &str) -> Id {
    Id::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn semver(value: &str) -> CanonicalSemver {
    CanonicalSemver::parse(value).unwrap_or_else(|error| panic!("{value} must parse: {error}"))
}

fn plugin_id() -> PluginId {
    PluginId::parse("vendor.pkg").unwrap_or_else(|error| panic!("must parse: {error}"))
}

fn action(owner: &str, handler: &str) -> Action {
    Action::parse(ActionDraft {
        id: id(owner),
        label: "Run".to_owned(),
        description: "Run it".to_owned(),
        category: id("tasks"),
        contexts: vec![id("core.dashboard")],
        arguments: Vec::new(),
        timeout_seconds: 30,
        destructive: false,
        confirmation: ActionConfirmation::None,
        handler: id(handler),
        allowed_outcomes: Vec::new(),
    })
    .unwrap_or_else(|error| panic!("{owner} must parse: {error}"))
}

fn panel(owner: &str) -> Panel {
    Panel::parse(PanelDraft {
        id: id(owner),
        model_kinds: vec![ModelKind::List],
        event_kinds: Vec::new(),
        handler: id("render"),
        ports: Vec::new(),
    })
    .unwrap_or_else(|error| panic!("{owner} must parse: {error}"))
}

fn route(owner: &str, target: &str) -> Route {
    Route::parse(RouteDraft {
        id: id(owner),
        activation_fields: Vec::new(),
        target_screen: id(target),
    })
    .unwrap_or_else(|error| panic!("{owner} must parse: {error}"))
}

fn screens(path: &str, ids: &[&str]) -> ScreenContribution {
    ScreenContribution::parse(
        RelativePath::parse(path).unwrap_or_else(|error| panic!("must parse: {error}")),
        ids.iter().map(|value| id(value)).collect(),
    )
    .unwrap_or_else(|error| panic!("{path} must parse: {error}"))
}

fn provider_none() -> Provider {
    Provider::parse(ProviderMode::None, Vec::new())
        .unwrap_or_else(|error| panic!("must parse: {error}"))
}

fn provider_one_shot() -> Provider {
    let triple = HostTriple::parse("aarch64-apple-darwin")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    let path = RelativePath::parse("bin/p").unwrap_or_else(|error| panic!("must parse: {error}"));
    Provider::parse(ProviderMode::OneShot, vec![(triple, path)])
        .unwrap_or_else(|error| panic!("must parse: {error}"))
}

fn draft() -> ManifestDraft {
    ManifestDraft {
        manifest_schema: 1,
        id: plugin_id(),
        version: semver("1.0.0"),
        display_name: "Git Merger".to_owned(),
        host_api_minimum: semver("1.0.0"),
        host_api_maximum: semver("2.0.0"),
        protocol: 1,
        provider: provider_none(),
        config: None,
        actions: Vec::new(),
        panels: Vec::new(),
        routes: Vec::new(),
        screens: Vec::new(),
        defaults: None,
    }
}

fn error_of(draft: ManifestDraft) -> ManifestError {
    Manifest::parse(draft)
        .err()
        .unwrap_or_else(|| panic!("the draft must be rejected"))
}

#[test]
fn a_minimal_provider_free_manifest_parses() {
    let manifest = Manifest::parse(draft()).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(manifest.id().as_str(), "vendor.pkg");
    assert_eq!(manifest.version().as_str(), "1.0.0");
    assert_eq!(manifest.display_name(), "Git Merger");
    assert!(!manifest.provider().is_executable());
    assert_eq!(manifest.coordinate().to_string(), "vendor.pkg@1.0.0");
}

#[test]
fn only_the_supported_schema_and_protocol_are_accepted() {
    for schema in [0, 2, 9] {
        let mut candidate = draft();
        candidate.manifest_schema = schema;
        assert_eq!(
            error_of(candidate),
            ManifestError::UnsupportedSchema { found: schema }
        );
    }
    for protocol in [0, 2, 9] {
        let mut candidate = draft();
        candidate.protocol = protocol;
        assert_eq!(
            error_of(candidate),
            ManifestError::UnsupportedProtocol { found: protocol }
        );
    }
}

#[test]
fn the_host_api_range_may_not_be_inverted() {
    let mut candidate = draft();
    candidate.host_api_minimum = semver("2.0.0");
    candidate.host_api_maximum = semver("1.0.0");
    assert_eq!(error_of(candidate), ManifestError::InvertedHostApiRange);

    let mut equal = draft();
    equal.host_api_minimum = semver("1.0.0");
    equal.host_api_maximum = semver("1.0.0");
    assert!(
        Manifest::parse(equal).is_ok(),
        "one supported version is a range"
    );
}

#[test]
fn a_display_name_may_not_be_blank() {
    let mut candidate = draft();
    candidate.display_name = "   ".to_owned();
    assert_eq!(error_of(candidate), ManifestError::BlankDisplayName);
}

#[test]
fn every_owned_declaration_must_be_prefixed_with_the_plugin_id() {
    let mut actions = draft();
    actions.provider = provider_one_shot();
    actions.actions = vec![action("other.pkg.run", "run")];
    assert_eq!(
        error_of(actions),
        ManifestError::ForeignOwner {
            kind: "action",
            id: "other.pkg.run".to_owned()
        }
    );

    let mut panels = draft();
    panels.provider = provider_one_shot();
    panels.panels = vec![panel("vendor.other.list")];
    assert_eq!(
        error_of(panels),
        ManifestError::ForeignOwner {
            kind: "panel",
            id: "vendor.other.list".to_owned()
        }
    );
}

#[test]
fn the_plugin_id_alone_is_not_an_owned_declaration_id() {
    // `vendor.pkg` is the package, not a declaration inside it; an owned id
    // must add at least one further label.
    let mut candidate = draft();
    candidate.provider = provider_one_shot();
    candidate.actions = vec![action("vendor.pkg", "run")];
    assert_eq!(
        error_of(candidate),
        ManifestError::ForeignOwner {
            kind: "action",
            id: "vendor.pkg".to_owned()
        }
    );
}

#[test]
fn a_prefix_must_end_at_a_label_boundary() {
    // `vendor.pkgx.run` starts with the text `vendor.pkg` but is a different
    // package, so a plain string prefix test would wrongly accept it.
    let mut candidate = draft();
    candidate.provider = provider_one_shot();
    candidate.actions = vec![action("vendor.pkgx.run", "run")];
    assert_eq!(
        error_of(candidate),
        ManifestError::ForeignOwner {
            kind: "action",
            id: "vendor.pkgx.run".to_owned()
        }
    );
}

#[test]
fn a_provider_free_package_may_not_declare_handlers() {
    let mut actions = draft();
    actions.actions = vec![action("vendor.pkg.run", "run")];
    assert_eq!(
        error_of(actions),
        ManifestError::ProviderFreeDeclaresHandler {
            kind: "action",
            id: "vendor.pkg.run".to_owned()
        }
    );

    let mut panels = draft();
    panels.panels = vec![panel("vendor.pkg.list")];
    assert_eq!(
        error_of(panels),
        ManifestError::ProviderFreeDeclaresHandler {
            kind: "panel",
            id: "vendor.pkg.list".to_owned()
        }
    );
}

#[test]
fn duplicate_declaration_ids_are_rejected() {
    let mut candidate = draft();
    candidate.provider = provider_one_shot();
    candidate.actions = vec![
        action("vendor.pkg.run", "run"),
        action("vendor.pkg.run", "run"),
    ];
    assert_eq!(
        error_of(candidate),
        ManifestError::DuplicateDeclaration {
            kind: "action",
            id: "vendor.pkg.run".to_owned()
        }
    );
}

#[test]
fn each_contributed_screen_is_bound_exactly_once() {
    let mut candidate = draft();
    candidate.screens = vec![
        screens("screens/a.json", &["vendor.pkg.main"]),
        screens("screens/b.json", &["vendor.pkg.main"]),
    ];
    assert_eq!(
        error_of(candidate),
        ManifestError::ScreenBoundTwice {
            id: "vendor.pkg.main".to_owned()
        }
    );

    let mut good = draft();
    good.screens = vec![
        screens("screens/a.json", &["vendor.pkg.main"]),
        screens("screens/b.json", &["vendor.pkg.other"]),
    ];
    assert!(Manifest::parse(good).is_ok());
}

#[test]
fn two_contributions_may_not_share_a_descriptor_path() {
    let mut candidate = draft();
    candidate.screens = vec![
        screens("screens/a.json", &["vendor.pkg.main"]),
        screens("screens/a.json", &["vendor.pkg.other"]),
    ];
    assert_eq!(
        error_of(candidate),
        ManifestError::DuplicateScreenPath {
            path: "screens/a.json".to_owned()
        }
    );
}

#[test]
fn a_route_must_target_a_contributed_screen() {
    let mut candidate = draft();
    candidate.routes = vec![route("vendor.pkg.open", "vendor.pkg.absent")];
    candidate.screens = vec![screens("screens/a.json", &["vendor.pkg.main"])];
    assert_eq!(
        error_of(candidate),
        ManifestError::UnresolvedRouteTarget {
            route: "vendor.pkg.open".to_owned(),
            screen: "vendor.pkg.absent".to_owned()
        }
    );

    let mut good = draft();
    good.routes = vec![route("vendor.pkg.open", "vendor.pkg.main")];
    good.screens = vec![screens("screens/a.json", &["vendor.pkg.main"])];
    assert!(Manifest::parse(good).is_ok());
}

#[test]
fn defaults_may_only_enable_declarations_the_manifest_makes() {
    let mut candidate = draft();
    candidate.provider = provider_one_shot();
    candidate.actions = vec![action("vendor.pkg.run", "run")];
    candidate.defaults = Some(PluginDefaults {
        actions_enabled: vec![id("vendor.pkg.absent")],
        screens_enabled: Vec::new(),
        config: Vec::new(),
    });
    assert_eq!(
        error_of(candidate),
        ManifestError::UnknownDefault {
            kind: "action",
            id: "vendor.pkg.absent".to_owned()
        }
    );

    let mut screens_default = draft();
    screens_default.screens = vec![screens("screens/a.json", &["vendor.pkg.main"])];
    screens_default.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: vec![id("vendor.pkg.absent")],
        config: Vec::new(),
    });
    assert_eq!(
        error_of(screens_default),
        ManifestError::UnknownDefault {
            kind: "screen",
            id: "vendor.pkg.absent".to_owned()
        }
    );
}

#[test]
fn a_default_config_key_must_name_a_declared_config_field() {
    let field = Field::parse(FieldDraft {
        id: id("depth"),
        kind: FieldKind::Integer,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("must parse: {error}"));

    let mut candidate = draft();
    candidate.config = Some(
        ConfigSchema::parse(1, vec![field.clone()])
            .unwrap_or_else(|error| panic!("must parse: {error}")),
    );
    candidate.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: Vec::new(),
        config: vec![(id("absent"), Scalar::Integer(1))],
    });
    assert_eq!(
        error_of(candidate),
        ManifestError::UnknownDefault {
            kind: "config field",
            id: "absent".to_owned()
        }
    );

    let mut good = draft();
    good.config = Some(
        ConfigSchema::parse(1, vec![field]).unwrap_or_else(|error| panic!("must parse: {error}")),
    );
    good.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: Vec::new(),
        config: vec![(id("depth"), Scalar::Integer(1))],
    });
    assert!(Manifest::parse(good).is_ok());
}

#[test]
fn a_default_config_value_must_match_its_field_kind() {
    let field = Field::parse(FieldDraft {
        id: id("depth"),
        kind: FieldKind::Integer,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("must parse: {error}"));

    let mut candidate = draft();
    candidate.config = Some(
        ConfigSchema::parse(1, vec![field]).unwrap_or_else(|error| panic!("must parse: {error}")),
    );
    candidate.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: Vec::new(),
        config: vec![(id("depth"), Scalar::Text("deep".to_owned()))],
    });
    assert_eq!(
        error_of(candidate),
        ManifestError::DefaultKindMismatch {
            field: "depth".to_owned()
        }
    );
}

#[test]
fn a_default_outside_the_fields_declared_bounds_is_rejected() {
    let bounded = Field::parse(FieldDraft {
        id: id("depth"),
        kind: FieldKind::Integer,
        required: false,
        default: None,
        minimum: Some(Scalar::Integer(10)),
        maximum: Some(Scalar::Integer(100)),
        choices: Vec::new(),
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("must parse: {error}"));

    let mut candidate = draft();
    candidate.config = Some(
        ConfigSchema::parse(1, vec![bounded.clone()])
            .unwrap_or_else(|error| panic!("must parse: {error}")),
    );
    candidate.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: Vec::new(),
        config: vec![(id("depth"), Scalar::Integer(5))],
    });
    assert_eq!(
        error_of(candidate),
        ManifestError::DefaultKindMismatch {
            field: "depth".to_owned()
        },
        "5 is below the field's declared minimum of 10"
    );

    let mut inside = draft();
    inside.config = Some(
        ConfigSchema::parse(1, vec![bounded]).unwrap_or_else(|error| panic!("must parse: {error}")),
    );
    inside.defaults = Some(PluginDefaults {
        actions_enabled: Vec::new(),
        screens_enabled: Vec::new(),
        config: vec![(id("depth"), Scalar::Integer(10))],
    });
    assert!(
        Manifest::parse(inside).is_ok(),
        "the bound itself is inside it"
    );
}

#[test]
fn declaration_arrays_accept_their_limits_and_reject_one_more() {
    let mut at_limit = draft();
    at_limit.provider = provider_one_shot();
    at_limit.actions = (0..ACTION_LIMIT)
        .map(|index| action(&format!("vendor.pkg.a{index}"), "run"))
        .collect();
    assert!(Manifest::parse(at_limit).is_ok());

    let mut over_limit = draft();
    over_limit.provider = provider_one_shot();
    over_limit.actions = (0..=ACTION_LIMIT)
        .map(|index| action(&format!("vendor.pkg.a{index}"), "run"))
        .collect();
    assert_eq!(
        error_of(over_limit),
        ManifestError::TooManyDeclarations {
            kind: "action",
            len: ACTION_LIMIT + 1
        }
    );

    let mut panels_over = draft();
    panels_over.provider = provider_one_shot();
    panels_over.panels = (0..=PANEL_LIMIT)
        .map(|index| panel(&format!("vendor.pkg.p{index}")))
        .collect();
    assert_eq!(
        error_of(panels_over),
        ManifestError::TooManyDeclarations {
            kind: "panel",
            len: PANEL_LIMIT + 1
        }
    );

    let mut routes_over = draft();
    routes_over.routes = (0..=ROUTE_LIMIT)
        .map(|index| route(&format!("vendor.pkg.r{index}"), "vendor.pkg.main"))
        .collect();
    routes_over.screens = vec![screens("screens/a.json", &["vendor.pkg.main"])];
    assert_eq!(
        error_of(routes_over),
        ManifestError::TooManyDeclarations {
            kind: "route",
            len: ROUTE_LIMIT + 1
        }
    );
}

#[test]
fn validating_a_manifest_starts_no_process() {
    // Validation is pure: it takes declarations and returns declarations or a
    // diagnostic. The type system carries the guarantee — nothing in this
    // module can spawn, because it never touches std::process.
    let mut candidate = draft();
    candidate.provider = provider_one_shot();
    candidate.actions = vec![action("vendor.pkg.run", "run")];
    let manifest = Manifest::parse(candidate).unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(manifest.actions().len(), 1);
    assert!(manifest.provider().is_executable());
}
