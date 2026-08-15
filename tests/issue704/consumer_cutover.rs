//! CWR1-06/CWR1-08: every declaration consumer reads one committed aggregate.

use std::fs;
use std::sync::Arc;

use jefe::startup_commit::commit_candidate;
use jefe::state::AppState;
use jefe::workbench::{ActivationValues, CustomScreenId, RouteId, ScreenIdentity};

use super::support::{build, config_root, publish_settings, resolve_paths, scan_roots};
use super::transaction_support::{EmptyEnv, fast_bounds};

fn committed_workbench_with_local_screen() -> jefe::startup_commit::StartupCommit {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let config = config_root(temp.path());
    let definitions = config.join("definitions");
    fs::create_dir_all(&definitions)
        .unwrap_or_else(|error| panic!("definitions directory: {error}"));
    fs::write(
        definitions.join("review.screen.toml"),
        include_str!("../../src/workbench/testdata/local-review.screen.toml"),
    )
    .unwrap_or_else(|error| panic!("screen definition: {error}"));
    let paths = resolve_paths(&config);
    let inventory = scan_roots(&[]);
    let settings = publish_settings(
        &inventory,
        "settings_schema = 2\n[workbench]\nenabled_screens = [\"local.review\"]\n",
    );
    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("candidate: {error}"));
    commit_candidate(candidate, None, &fast_bounds(), &EmptyEnv)
        .unwrap_or_else(|error| panic!("commit: {error}"))
}

#[test]
fn app_state_holds_the_exact_committed_aggregate_identity() {
    let commit = committed_workbench_with_local_screen();
    let state = AppState::new(Arc::clone(&commit.workbench));

    assert!(
        commit.workbench.provider_ready().is_some(),
        "successful commit owns its Ready publication metadata"
    );
    assert!(Arc::ptr_eq(state.published_workbench(), &commit.workbench));
    assert!(Arc::ptr_eq(
        state.published_workbench(),
        state.clone().published_workbench()
    ));
}

#[test]
fn injected_screen_navigation_and_layout_begin_at_the_committed_aggregate() {
    let commit = committed_workbench_with_local_screen();
    let mut state = AppState::new(Arc::clone(&commit.workbench));
    let route = RouteId::from_static("review");

    let _ = state.enter_provider_route(route, ActivationValues::empty());

    let expected = CustomScreenId::parse("local.review")
        .unwrap_or_else(|error| panic!("custom screen id: {error}"));
    assert_eq!(state.screen(), ScreenIdentity::Custom(expected));
    let layout = jefe::screen_layout::resolve_screen(&state, 120, 40);
    assert!(
        layout.is_some(),
        "the injected descriptor must resolve layout"
    );
    assert!(Arc::ptr_eq(state.published_workbench(), &commit.workbench));
}

#[test]
fn production_sources_have_no_ambient_or_split_screen_authority() {
    let workbench = include_str!("../../src/workbench/mod.rs");
    let screens = include_str!("../../src/startup_screens.rs");
    let main = include_str!("../../src/main.rs");
    let coordinator = include_str!("../../src/runtime/provider/coordinator.rs");
    let settings_state = include_str!("../../src/state/settings_types.rs");

    for deleted in [
        "PUBLISHED_REGISTRY",
        "RegistryAlreadyPublished",
        "publish_screen_registry",
        "pub fn screen_registry()",
        "pub fn screen_descriptor(",
    ] {
        assert!(!workbench.contains(deleted), "must delete {deleted}");
    }
    for deleted in ["compose_and_publish", "publish_composed"] {
        assert!(!screens.contains(deleted), "must delete {deleted}");
    }
    assert!(!main.contains("publish_screen_registry_or_exit"));
    assert!(
        !settings_state.contains("ActionRegistrySnapshot"),
        "mutable Settings state must not copy committed action declarations"
    );
    for deleted in [
        "publication: PersistentPublication",
        "catalog: ProviderCatalog",
        "pub fn empty(",
        "pub fn from_catalog(",
        "pub fn from_startup(",
        "pub fn publication(",
        "pub fn catalog(",
    ] {
        assert!(
            !coordinator.contains(deleted),
            "coordinator must not retain static authority: {deleted}"
        );
    }
}

#[test]
fn composition_root_enforces_identity_and_fallible_owner_transfer_before_commit() {
    let app_init = include_str!("../../src/app_init.rs");
    let aggregate = include_str!("../../src/published_workbench.rs");
    let startup = include_str!("../../src/startup_commit.rs");

    assert!(app_init.contains("Arc::ptr_eq(state.published_workbench(), &ctx_guard.workbench)"));
    assert!(!app_init.contains("debug_assert!(std::sync::Arc::ptr_eq"));
    assert!(!app_init.contains("let Some(ctx_arc) = ctx else {\n        return Vec::new();"));
    assert!(!aggregate.contains("#[derive(Debug, Clone)]\npub struct PublishedWorkbench"));

    let owner_transfer = startup
        .find("ProviderCoordinator::from_ready_supervisor(supervisor)")
        .unwrap_or_else(|| panic!("startup must transfer every ready provider into its owner"));
    let durable_commit = startup
        .find("commit_state_import(plan)")
        .unwrap_or_else(|| panic!("startup must retain the final durable commit"));
    let aggregate_commit = startup
        .find("candidate.with_provider_ready(publication)")
        .unwrap_or_else(|| panic!("startup must attach Ready metadata to the aggregate"));
    assert!(owner_transfer < durable_commit);
    assert!(durable_commit < aggregate_commit);
    assert!(startup.contains("map_err(StartupCommitFailure::ProviderOwner)"));
}
