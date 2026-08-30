use std::path::PathBuf;

use super::{
    ResourcePublicationError, WorkbenchCandidateRequest, WorkbenchStaticFailure,
    build_workbench_candidate,
};
use crate::domain::Id;
use crate::domain::plugin::HostTriple;
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::scan;
use crate::persistence::settings_document::PublishedSettings;
use crate::runtime::provider::Containment;
use crate::workbench::{
    BuiltinResourceSchemaError, CustomScreenId, ResourceSchemaError, ScreenId, ScreenIdentity,
};

#[path = "tests/startup_candidate_binding_tests.rs"]
mod binding_tests;

fn custom_screen(raw: &'static str) -> CustomScreenId {
    CustomScreenId::parse(raw)
        .unwrap_or_else(|error| unreachable!("valid custom screen fixture: {error}"))
}

struct CandidateFixture {
    root: PathBuf,
}

impl CandidateFixture {
    fn new(label: &str, definition: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "jefe-startup-candidate-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("definitions"))
            .unwrap_or_else(|error| unreachable!("fixture directory must exist: {error}"));
        std::fs::write(root.join("definitions/review.screen.toml"), definition)
            .unwrap_or_else(|error| unreachable!("fixture definition must be written: {error}"));
        Self { root }
    }

    fn paths(&self) -> ResolvedPaths {
        let resolved = |name: &str| ResolvedFile {
            path: self.root.join(name),
            provenance: PathProvenance::ConfigArgument,
            sources: Vec::new(),
        };
        ResolvedPaths {
            settings: resolved("settings.toml"),
            state: resolved("state.json"),
            definitions: self.root.join("definitions"),
            plugins: self.root.join("plugins"),
            themes: self.root.join("themes"),
        }
    }

    fn build(
        &self,
        settings: &PublishedSettings,
    ) -> Result<crate::published_workbench::PublishedWorkbench, WorkbenchStaticFailure> {
        let paths = self.paths();
        let inventory = scan(&[]);
        build_workbench_candidate(&WorkbenchCandidateRequest {
            paths: &paths,
            inventory: &inventory,
            settings,
            host: HostTriple::current(),
            containment: Containment {
                home: self.root.join("home"),
                tmpdir: self.root.join("tmp"),
                working_dir: self.root.join("work"),
                locale: "C".to_owned(),
                host_api: crate::VERSION.to_owned(),
            },
        })
    }
}

impl Drop for CandidateFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn enabled_settings() -> PublishedSettings {
    let mut settings = PublishedSettings::default();
    settings.workbench.enabled_screens = vec![
        Id::parse("local.review")
            .unwrap_or_else(|error| unreachable!("fixture screen ID must parse: {error}")),
    ];
    settings
}

fn review_definition() -> String {
    include_str!("workbench/testdata/local-review.screen.toml").replace("\r\n", "\n")
}
#[test]
fn definition_resource_faults_are_configuration_failures() {
    let failure = WorkbenchStaticFailure::Resources(ResourcePublicationError::Composition(
        ResourceSchemaError::InvalidVersion { version: 0 },
    ));

    assert!(failure.is_configuration_failure());
    assert_eq!(failure.exit_code(), 2);
}

#[test]
fn compiled_resource_faults_remain_internal_failures() {
    let builtin = WorkbenchStaticFailure::Resources(ResourcePublicationError::Builtin(
        BuiltinResourceSchemaError::Resource(ResourceSchemaError::InvalidVersion { version: 0 }),
    ));
    let port = WorkbenchStaticFailure::Resources(ResourcePublicationError::InvalidPortType {
        screen: ScreenIdentity::Compiled(ScreenId::Issues),
        panel: "issues.list".to_owned(),
        port: "selection".to_owned(),
        type_id: "invalid".to_owned(),
    });

    for failure in [builtin, port] {
        assert!(!failure.is_configuration_failure());
        assert_eq!(failure.exit_code(), 78);
    }
}

#[test]
fn definition_port_resource_faults_are_configuration_failures() {
    let failure = WorkbenchStaticFailure::Resources(ResourcePublicationError::InvalidPortType {
        screen: ScreenIdentity::Custom(custom_screen("local.review")),
        panel: "review.list".to_owned(),
        port: "selection".to_owned(),
        type_id: "invalid".to_owned(),
    });

    assert!(failure.is_configuration_failure());
    assert_eq!(failure.exit_code(), 2);
}

#[test]
fn candidate_publishes_an_enabled_definition_resource_schema() {
    let fixture = CandidateFixture::new("resource-valid", &review_definition());
    let candidate = fixture
        .build(&enabled_settings())
        .unwrap_or_else(|error| unreachable!("valid definition must publish: {error}"));
    let owner = Id::parse("local.review")
        .unwrap_or_else(|error| unreachable!("fixture owner must parse: {error}"));
    let type_id = Id::parse("local.review.note")
        .unwrap_or_else(|error| unreachable!("fixture type must parse: {error}"));

    assert_eq!(
        candidate
            .resource_schemas()
            .validate_reference(&owner, &type_id, 1),
        Ok(())
    );
}

#[test]
fn candidate_refuses_unknown_wrong_version_and_wrong_owner_port_references() {
    let base = review_definition();
    let cases = [
        (
            "resource-unknown",
            base.replace("github.pull-request@1", "github.unknown@1"),
            "resource type github.unknown is not published",
        ),
        (
            "resource-version",
            base.replace("github.pull-request@1", "github.pull-request@2"),
            "resource type github.pull-request version 2 is not published",
        ),
        (
            "resource-owner",
            base.replace("github.pull-requests", "github.issues"),
            "resource schema owner github.issues does not match github.pull-requests",
        ),
    ];

    for (label, definition, expected) in cases {
        let fixture = CandidateFixture::new(label, &definition);
        let Err(error) = fixture.build(&enabled_settings()) else {
            panic!("invalid port reference must refuse the whole candidate");
        };
        assert!(error.is_configuration_failure());
        assert_eq!(error.exit_code(), 2);
        let diagnostic = error.to_string();
        assert!(diagnostic.contains(expected), "{diagnostic}");
        assert!(!diagnostic.contains("hidden"));
    }
}

#[test]
fn candidate_refuses_a_definition_schema_colliding_with_a_builtin() {
    let definition = review_definition().replace("local.review.note", "github.issue");
    let fixture = CandidateFixture::new("resource-duplicate", &definition);

    let Err(error) = fixture.build(&enabled_settings()) else {
        panic!("duplicate schema identity must refuse the whole candidate");
    };

    assert!(matches!(
        error,
        WorkbenchStaticFailure::Resources(ResourcePublicationError::Composition(
            ResourceSchemaError::DuplicateSchema { .. }
        ))
    ));
    assert!(error.is_configuration_failure());
    assert_eq!(error.exit_code(), 2);
}

fn with_binding(definition: &str, context: &str, action: &str) -> String {
    format!("{definition}\n[[bindings]]\ncontext = \"{context}\"\naction = \"{action}\"\n")
}

#[test]
fn candidate_refuses_screen_binding_that_conflicts_with_host_controls() {
    let definition = with_binding(&review_definition(), "prs.list", "prs.open");
    let fixture = CandidateFixture::new("binding-host-conflict", &definition);

    let Err(failure) = fixture.build(&enabled_settings()) else {
        panic!("Enter must not ambiguously mean both open PR and activate host control");
    };

    assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
    assert!(failure.to_string().contains("ImplicitShadow"));
}

#[test]
fn candidate_refuses_a_protected_screen_binding() {
    let definition = with_binding(&review_definition(), "prs", "prs.exit");
    let fixture = CandidateFixture::new("binding-protected", &definition);

    let Err(failure) = fixture.build(&enabled_settings()) else {
        panic!("a screen must not claim a protected host action");
    };

    assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
    assert!(failure.to_string().contains("ProtectedDeclared"));
}

#[test]
fn candidate_validates_binding_existence_and_context_after_composition() {
    for (label, context, action, expected) in [
        (
            "binding-unknown",
            "vendor.context",
            "vendor.action",
            "UnknownAction",
        ),
        (
            "binding-context-mismatch",
            "prs.list",
            "dashboard.open-help",
            "ContextMismatch",
        ),
    ] {
        let definition = with_binding(&review_definition(), context, action);
        let fixture = CandidateFixture::new(label, &definition);
        let Err(failure) = fixture.build(&enabled_settings()) else {
            panic!("the final composed action registry must validate screen declarations");
        };
        assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
        assert!(failure.to_string().contains(expected));
    }
}

#[test]
fn candidate_refuses_a_screen_binding_unbound_by_effective_settings() {
    let definition = with_binding(&review_definition(), "prs.list", "prs.list-browser");
    let fixture = CandidateFixture::new("binding-unbound", &definition);
    let mut settings = enabled_settings();
    settings
        .keymap
        .entry("prs.list".to_owned())
        .or_default()
        .insert("prs.list-browser".to_owned(), Vec::new());

    let Err(failure) = fixture.build(&settings) else {
        panic!("a requested action without an effective chord is unreachable");
    };

    assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
    assert!(failure.to_string().contains("DeclaredUnbound"));
}

#[test]
fn candidate_refuses_a_declared_binding_that_shadows_protected_back() {
    let definition = with_binding(&review_definition(), "prs.list", "prs.list-browser");
    let fixture = CandidateFixture::new("binding-shadows-back", &definition);
    let mut settings = enabled_settings();
    settings
        .keymap
        .entry("prs.list".to_owned())
        .or_default()
        .insert("prs.list-browser".to_owned(), vec!["Esc".to_owned()]);

    let Err(failure) = fixture.build(&settings) else {
        panic!("a declared action must not shadow protected host Back");
    };

    assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
    assert!(failure.to_string().contains("ProtectedShadowed"));
}

#[test]
fn candidate_refuses_duplicate_and_conflicting_declared_bindings() {
    let duplicate = with_binding(
        &with_binding(&review_definition(), "prs.list", "prs.list-browser"),
        "prs.list",
        "prs.list-browser",
    );
    let conflicting = with_binding(
        &with_binding(&review_definition(), "prs.list", "prs.list-browser"),
        "prs.detail",
        "prs.open-browser",
    );

    for (label, definition, expected) in [
        ("binding-duplicate", duplicate, "DuplicateBinding"),
        ("binding-declared-conflict", conflicting, "ImplicitShadow"),
    ] {
        let fixture = CandidateFixture::new(label, &definition);
        let Err(failure) = fixture.build(&enabled_settings()) else {
            panic!("invalid requested action pairs must refuse publication");
        };
        assert!(matches!(failure, WorkbenchStaticFailure::Actions(_)));
        assert!(failure.to_string().contains(expected));
    }
}
fn active_definition_state() -> (
    crate::state::AppState,
    crate::domain::input_context::ContextStack,
) {
    use std::sync::Arc;

    use crate::state::navigation::{Activation, NavIntent, NavMessage, reduce_navigation};
    use crate::workbench::ActivationValues;

    let definition = with_binding(&review_definition(), "dashboard", "dashboard.open-help");
    let fixture = CandidateFixture::new("binding-runtime", &definition);
    let candidate = fixture
        .build(&enabled_settings())
        .unwrap_or_else(|error| panic!("valid declared binding must publish: {error}"));
    let screen = ScreenIdentity::Custom(custom_screen("local.review"));
    let route = candidate
        .screen_registry()
        .get_identity(screen)
        .unwrap_or_else(|| panic!("published local screen must be present"))
        .route;
    let published = Arc::new(candidate);
    let mut state = crate::state::AppState::new(Arc::clone(&published));
    let activation =
        Activation::from_source(route, ActivationValues::default(), state.nav.current());
    state.nav = reduce_navigation(
        state.nav,
        published.screen_registry(),
        NavMessage::Navigate(NavIntent::Push(activation)),
    )
    .state;
    let stack =
        crate::domain::input_context::ContextStack::from_ordered(["workbench", "global"], false)
            .unwrap_or_else(|error| panic!("context stack: {error}"));
    (state, stack)
}

fn mark_declared_help_unavailable(state: &mut crate::state::AppState) {
    use crate::domain::action_registry::{
        ActionAvailability, ActionId, Availability, AvailabilityGeneration,
    };
    use crate::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};

    let action =
        ActionId::parse("dashboard.open-help").unwrap_or_else(|error| panic!("action: {error}"));
    state.action_availability = Some(AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(705),
            owner: Id::parse("core.bindings").unwrap_or_else(|error| panic!("owner: {error}")),
            screen_generation: 1,
            activation_generation: 1,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "declared-binding"),
        },
        vec![ActionAvailability::new(
            action,
            Availability::Unavailable {
                reason: "runtime owner unavailable".to_owned(),
            },
        )],
    ));
}

#[test]
fn active_definition_resolves_only_its_exact_declared_action_pair() {
    use crate::domain::action_registry::{ActionId, HandlerKey, Resolution};
    use crate::domain::input_context::ContextStack;
    use crate::domain::keymap::Chord;

    let chord = |raw| Chord::parse(raw).unwrap_or_else(|error| panic!("chord: {error}"));
    let (mut state, stack) = active_definition_state();
    assert!(!state.has_dashboard_action_context());
    let declared_action =
        ActionId::parse("dashboard.open-help").unwrap_or_else(|error| panic!("action: {error}"));

    assert_eq!(
        state.resolve_action(&chord("h"), &stack),
        Resolution::Dispatch {
            action: declared_action.clone(),
            handler: HandlerKey::OpenHelp,
        }
    );
    assert_eq!(
        state.resolve_action(&chord("e"), &stack),
        Resolution::Unbound
    );
    mark_declared_help_unavailable(&mut state);
    assert_eq!(
        state.resolve_action(&chord("h"), &stack),
        Resolution::Unavailable {
            action: declared_action,
            reason: "runtime owner unavailable".to_owned(),
        }
    );
    let terminal_stack = ContextStack::from_ordered(["workbench", "global"], true)
        .unwrap_or_else(|error| panic!("terminal context stack: {error}"));
    assert_eq!(
        state.resolve_action(&chord("h"), &terminal_stack),
        Resolution::ForwardToPty
    );
}

#[test]
fn modal_and_shell_authorities_precede_active_screen_declarations() {
    use crate::domain::action_registry::{ActionId, HandlerKey, Resolution};
    use crate::domain::input_context::ContextStack;
    use crate::domain::keymap::Chord;

    let chord = |raw| Chord::parse(raw).unwrap_or_else(|error| panic!("chord: {error}"));
    let (state, _) = active_definition_state();
    let help = ContextStack::from_ordered(["help"], false)
        .unwrap_or_else(|error| panic!("help context: {error}"));
    assert_eq!(
        state.resolve_action(&chord("Shift+?"), &help),
        Resolution::Dispatch {
            action: ActionId::parse("help.close").unwrap_or_else(|error| panic!("action: {error}")),
            handler: HandlerKey::HelpClose,
        }
    );

    let shell = ContextStack::from_ordered(["shell-overlay"], false)
        .unwrap_or_else(|error| panic!("shell context: {error}"));
    assert_eq!(
        state.resolve_action(&chord("h"), &shell),
        Resolution::Unbound
    );
    assert_eq!(
        state.resolve_action(&chord("F12"), &shell),
        Resolution::Dispatch {
            action: ActionId::parse("shell.hide").unwrap_or_else(|error| panic!("action: {error}")),
            handler: HandlerKey::HideShellOverlay,
        }
    );
}
