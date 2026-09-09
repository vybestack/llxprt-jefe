//! Terminal navigate outcomes survive the route push that consumes them
//! (issue #758, Slice 1).
//!
//! The host applies a provider `Navigate` outcome by pushing the destination
//! route in the same tick, and a push allocates a fresh screen instance. The
//! dispatch layer therefore carries the terminal request onto the pushed
//! instance. These tests pin that state contract: the completed row stays
//! projectable from the pushed instance, Escape there dismisses it without
//! popping the navigation, the origin keeps nothing, and live or foreign
//! requests stay bound to the context they were authorized under.

use super::*;
use crate::domain::Id;
use crate::domain::TypedMap;
use crate::domain::effects::ProviderRequestKey;
use crate::domain::plugin::HostTriple;
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::messages::{AppMessage, ProviderMessage};
use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use crate::persistence::plugin_inventory::{MANIFEST_FILE_NAME, scan};
use crate::persistence::plugin_roots::{PluginRoot, PluginRootKind};
use crate::runtime::provider::Containment;
use crate::runtime::provider::protocol::Outcome;
use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};
use crate::state::provider_requests::{ActionPolicy, InvokeInput};
use crate::state::provider_view::ProviderRowStatus;
use crate::state::transition::TransitionExt;
use crate::workbench::{ActivationValues, RouteId};
use std::sync::Arc;

/// Stage a one-shot `vendor.deploy` package whose action may navigate.
fn stage_navigating_provider_package() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let manifest = format!(
        r#"{{
          "manifest_schema": 1,
          "id": "vendor.deploy",
          "version": "1.0.0",
          "display_name": "Package vendor.deploy",
          "host_api": {{ "minimum": "1.0.0", "maximum": "1.0.0" }},
          "protocol": 1,
          "provider": {{ "mode": "one-shot", "binaries": {{ "{}": "bin/provider" }} }},
          "actions": [
            {{
              "id": "vendor.deploy.ship",
              "label": "Ship release",
              "description": "Ship the selected release",
              "category": "vendor",
              "contexts": ["dashboard"],
              "arguments": [],
              "timeout_seconds": 60,
              "destructive": false,
              "confirmation": "none",
              "handler": "run",
              "allowed_outcomes": ["navigate-declared-route"]
            }}
          ],
          "panels": [],
          "routes": [],
          "screens": []
        }}"#,
        HostTriple::current().as_str()
    );
    let directory = temp
        .path()
        .join("packages")
        .join("vendor.deploy")
        .join("1.0.0");
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("staging must succeed: {error}"));
    std::fs::write(directory.join(MANIFEST_FILE_NAME), manifest.as_bytes())
        .unwrap_or_else(|error| panic!("manifest must write: {error}"));
    temp
}

/// One explicit workbench whose published registry carries the navigate-capable
/// provider action `vendor.deploy.ship`, composed exactly as startup would
/// publish it.
fn navigating_provider_state() -> AppState {
    let temp = stage_navigating_provider_package();
    let root = temp.path().join("packages");
    let inventory = scan(&[PluginRoot::new(root, PluginRootKind::User)]);
    let catalog = crate::config_owners::owner_catalog_with_packages(inventory.packages())
        .unwrap_or_else(|diagnostics| panic!("owner catalog must build: {diagnostics:?}"));
    let settings = crate::persistence::settings_document::SettingsDocument::parse(
        "settings_schema = 2\n\n[plugins.\"vendor.deploy\"]\nenabled = true\n".as_bytes(),
    )
    .unwrap_or_else(|error| panic!("settings must parse: {error:?}"))
    .publish(&catalog)
    .unwrap_or_else(|diagnostics| panic!("settings must publish: {diagnostics:?}"));

    let file = |name: &str| ResolvedFile {
        path: temp.path().join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    let paths = ResolvedPaths {
        settings: file("settings.toml"),
        state: file("state.json"),
        definitions: temp.path().join("definitions"),
        plugins: temp.path().join("plugins"),
        themes: temp.path().join("themes"),
    };
    let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: &settings,
        host: HostTriple::current(),
        containment: Containment {
            home: temp.path().join("home"),
            tmpdir: temp.path().join("tmp"),
            working_dir: temp.path().join("work"),
            locale: "C".to_owned(),
            host_api: "1.0.0".to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("provider workbench must compose: {error}"));
    AppState::new(Arc::new(candidate))
}

fn invoke_request(
    state: &mut AppState,
    action: &str,
    context_screen: &Id,
    context_instance: &Id,
    policy: &ActionPolicy,
) -> ProviderRequestKey {
    let owner = Id::parse("host").unwrap_or_else(|error| panic!("owner: {error}"));
    let action = Id::parse(action).unwrap_or_else(|error| panic!("action: {error}"));
    state
        .provider_requests
        .invoke(InvokeInput {
            owner: &owner,
            action_id: &action,
            context_screen,
            context_instance,
            context_refs: &TypedMap::new(),
            arguments: &TypedMap::new(),
            policy,
        })
        .unwrap_or_else(|error| panic!("invoke: {error}"))
        .key
}

/// Invoke `vendor.deploy.ship` against the current screen instance with a
/// policy that allows exactly one declared-route navigate outcome.
fn invoke_navigating_request(state: &mut AppState) -> ProviderRequestKey {
    let screen = Id::parse(state.nav.current().screen.as_str())
        .unwrap_or_else(|error| panic!("screen: {error}"));
    let instance = Id::parse(&state.nav.current().id.to_string())
        .unwrap_or_else(|error| panic!("instance: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::None,
        vec![ActionOutcome::NavigateDeclaredRoute],
        false,
    );
    invoke_request(state, "vendor.deploy.ship", &screen, &instance, &policy)
}

/// Complete one request with a terminal `Navigate` outcome for `route`.
fn record_navigate_outcome(state: &mut AppState, key: &ProviderRequestKey, route: &str) {
    let route_id = Id::parse(route).unwrap_or_else(|error| panic!("route {route}: {error}"));
    state
        .provider_requests
        .record_outcome(
            key,
            Outcome::Navigate {
                route_id,
                activation: TypedMap::new(),
            },
            1,
        )
        .unwrap_or_else(|error| panic!("record navigate outcome: {error}"));
}

fn completed_navigate_row(projection: &super::provider_view::ProviderViewProjection) -> bool {
    projection.rows.iter().any(|row| {
        matches!(
            &row.status,
            ProviderRowStatus::Completed(summary) if summary.as_str() == "Navigate to actions"
        )
    })
}

fn current_provider_context(state: &AppState) -> (Id, Id) {
    let current = state.nav.current();
    let screen = Id::parse(current.screen.as_str())
        .unwrap_or_else(|error| panic!("screen context: {error}"));
    let instance = Id::parse(&current.id.to_string())
        .unwrap_or_else(|error| panic!("instance context: {error}"));
    (screen, instance)
}

fn enter_route_and_rebind_terminals(state: &mut AppState, route: RouteId) -> usize {
    let (old_screen, old_instance) = current_provider_context(state);
    let old_open_instance = state.nav.current().id;

    state.enter_provider_route(route, ActivationValues::empty());
    if state.nav.current().id == old_open_instance {
        return 0;
    }

    let (new_screen, new_instance) = current_provider_context(state);
    state.provider_requests.rebind_terminal_context(
        &old_screen,
        &old_instance,
        &new_screen,
        &new_instance,
    )
}

#[test]
fn terminal_navigate_row_follows_the_pushed_instance() {
    let mut state = navigating_provider_state();
    let key = invoke_navigating_request(&mut state);
    record_navigate_outcome(&mut state, &key, "actions");

    let origin = state.nav.current();
    let origin_screen = origin.screen.as_str().to_owned();
    let origin_instance = origin.id.to_string();
    let origin_projection = state
        .provider_surface_projection(24)
        .unwrap_or_else(|| panic!("origin surface must project the completed row"));
    assert!(completed_navigate_row(&origin_projection));

    assert_eq!(
        enter_route_and_rebind_terminals(&mut state, RouteId::from_static("actions")),
        1
    );
    let pushed_screen = state.nav.current().screen.as_str().to_owned();
    let pushed_instance = state.nav.current().id.to_string();
    assert_ne!(pushed_screen, origin_screen);
    assert_ne!(pushed_instance, origin_instance);

    // The dispatch layer that pushed this route carries the terminal request
    // onto the pushed instance (issue #758); the surface must follow it there.
    let pushed_projection = state
        .provider_surface_projection(24)
        .unwrap_or_else(|| panic!("terminal navigate row must follow the pushed instance"));
    assert!(completed_navigate_row(&pushed_projection));
    let terminal = state
        .latest_current_provider_request()
        .unwrap_or_else(|| panic!("terminal request must be reachable from the pushed instance"));
    assert_eq!(terminal.key(), &key);
    assert!(terminal.is_terminal());
}

#[test]
fn escape_on_the_pushed_instance_dismisses_the_carried_row_without_popping() {
    let mut state = navigating_provider_state();
    let key = invoke_navigating_request(&mut state);
    record_navigate_outcome(&mut state, &key, "actions");
    let origin_instance = state.nav.current().id;

    assert_eq!(
        enter_route_and_rebind_terminals(&mut state, RouteId::from_static("actions")),
        1
    );
    let pushed_instance = state.nav.current().id;
    assert!(
        state.latest_current_provider_request().is_some(),
        "the carried row must be dismissable from the pushed instance"
    );

    let transition = state
        .apply_message(AppMessage::Provider(Box::new(
            ProviderMessage::DismissTerminals,
        )))
        .unwrap_or_else(|error| panic!("dismiss transition: {error}"));
    let state = transition.next_state;

    assert_eq!(
        state.nav.current().id,
        pushed_instance,
        "Escape must dismiss the carried row without popping the route"
    );
    assert!(state.latest_current_provider_request().is_none());
    assert!(
        state
            .provider_requests
            .requests()
            .iter()
            .all(|request| request.key() != &key)
    );

    let state = state.apply(crate::state::AppEvent::Back).committed_pure();
    assert_eq!(state.nav.current().id, origin_instance);
    assert!(
        state.latest_current_provider_request().is_none(),
        "the origin instance must not keep the row after the push carried it away"
    );
}

fn invoke_live_request(
    state: &mut AppState,
    screen: &Id,
    instance: &Id,
) -> (ProviderRequestKey, ActionPolicy) {
    let live_policy = ActionPolicy::new(
        ActionConfirmation::None,
        vec![ActionOutcome::NavigateDeclaredRoute],
        false,
    );
    let live_key = invoke_request(state, "vendor.deploy.live", screen, instance, &live_policy);
    (live_key, live_policy)
}

fn record_pending_confirmation(
    state: &mut AppState,
    screen: &Id,
    instance: &Id,
) -> ProviderRequestKey {
    let confirmation_policy = ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![ActionOutcome::RequestHostConfirmation],
        false,
    );
    let confirmation_key = invoke_request(
        state,
        "vendor.deploy.confirm",
        screen,
        instance,
        &confirmation_policy,
    );
    state
        .provider_requests
        .record_outcome(
            &confirmation_key,
            Outcome::RequestHostConfirmation {
                confirmation_id: Id::parse("confirmation.deploy")
                    .unwrap_or_else(|error| panic!("confirmation id: {error}")),
                title: "Confirm deploy".to_owned(),
                body: "Proceed?".to_owned(),
                confirm_label: "Deploy".to_owned(),
                destructive: false,
                continuation_schema: Vec::new(),
            },
            1,
        )
        .unwrap_or_else(|error| panic!("record confirmation: {error}"));
    confirmation_key
}

fn record_foreign_terminal(
    state: &mut AppState,
    policy: &ActionPolicy,
) -> (ProviderRequestKey, Id, Id) {
    let foreign_screen =
        Id::parse("core.errors").unwrap_or_else(|error| panic!("foreign screen: {error}"));
    let foreign_instance =
        Id::parse("instance-foreign").unwrap_or_else(|error| panic!("foreign instance: {error}"));
    let foreign_key = invoke_request(
        state,
        "vendor.deploy.foreign",
        &foreign_screen,
        &foreign_instance,
        policy,
    );
    state
        .provider_requests
        .record_error(&foreign_key, "foreign failure".to_owned())
        .unwrap_or_else(|error| panic!("record foreign terminal: {error}"));
    (foreign_key, foreign_screen, foreign_instance)
}

#[test]
fn rebind_changes_only_matching_terminal_requests_and_not_confirmation_tokens() {
    let mut state = navigating_provider_state();
    let (old_screen, old_instance) = current_provider_context(&state);
    let new_screen =
        Id::parse("github.actions").unwrap_or_else(|error| panic!("new screen context: {error}"));
    let new_instance = Id::parse("instance-999999")
        .unwrap_or_else(|error| panic!("new instance context: {error}"));

    let terminal_key = invoke_navigating_request(&mut state);
    record_navigate_outcome(&mut state, &terminal_key, "actions");

    let (live_key, live_policy) = invoke_live_request(&mut state, &old_screen, &old_instance);
    let confirmation_key = record_pending_confirmation(&mut state, &old_screen, &old_instance);
    let (foreign_key, foreign_screen, foreign_instance) =
        record_foreign_terminal(&mut state, &live_policy);

    let pending_before = state
        .provider_requests
        .first_pending_confirmation()
        .unwrap_or_else(|| panic!("pending confirmation"))
        .identity();
    assert_eq!(
        state.provider_requests.rebind_terminal_context(
            &old_screen,
            &old_instance,
            &new_screen,
            &new_instance,
        ),
        2
    );

    for key in [&terminal_key, &confirmation_key] {
        let request = state
            .provider_requests
            .request(key)
            .unwrap_or_else(|| panic!("rebound terminal request"));
        assert_eq!(request.context_screen(), &new_screen);
        assert_eq!(request.context_instance(), &new_instance);
    }

    let live = state
        .provider_requests
        .request(&live_key)
        .unwrap_or_else(|| panic!("live request"));
    assert_eq!(live.context_screen(), &old_screen);
    assert_eq!(live.context_instance(), &old_instance);

    let foreign = state
        .provider_requests
        .request(&foreign_key)
        .unwrap_or_else(|| panic!("foreign terminal request"));
    assert_eq!(foreign.context_screen(), &foreign_screen);
    assert_eq!(foreign.context_instance(), &foreign_instance);

    let pending_after = state
        .provider_requests
        .first_pending_confirmation()
        .unwrap_or_else(|| panic!("pending confirmation after rebind"))
        .identity();
    assert_eq!(pending_after, pending_before);
    assert_eq!(pending_after.context_screen(), &old_screen);
    assert_eq!(pending_after.context_instance(), &old_instance);
}
