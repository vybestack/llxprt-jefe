//! Explicit aggregate-backed application state fixture for integration tests.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use jefe::domain::plugin::HostTriple;
use jefe::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
use jefe::persistence::plugin_inventory;
use jefe::persistence::settings_document::PublishedSettings;
use jefe::published_workbench::PublishedWorkbench;
use jefe::runtime::provider::Containment;
use jefe::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};
use jefe::state::AppState;

#[must_use]
pub fn app_state() -> AppState {
    AppState::new(Arc::clone(published_workbench()))
}

/// The shared explicit workbench for integration tests.
///
/// # Panics
///
/// Panics when the explicit fixture workbench cannot be composed; the
/// integration suite treats that as an unrecoverable fixture failure.
#[must_use]
pub fn published_workbench() -> &'static Arc<PublishedWorkbench> {
    static WORKBENCH: OnceLock<Arc<PublishedWorkbench>> = OnceLock::new();
    WORKBENCH.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("jefe-test-workbench-{}", std::process::id()));
        let paths = fixture_paths(&root);
        let inventory = plugin_inventory::scan(&[]);
        let settings = PublishedSettings::default();
        let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
            paths: &paths,
            inventory: &inventory,
            settings: &settings,
            host: HostTriple::current(),
            containment: Containment {
                home: root.join("provider-home"),
                tmpdir: root.join("provider-tmp"),
                working_dir: root.join("provider-work"),
                locale: "C".to_owned(),
                host_api: jefe::VERSION.to_owned(),
            },
        })
        .unwrap_or_else(|error| panic!("compose integration-test workbench: {error}"));
        Arc::new(candidate)
    })
}

fn fixture_paths(root: &std::path::Path) -> ResolvedPaths {
    let file = |name: &str| ResolvedFile {
        path: root.join(name),
        provenance: PathProvenance::ConfigArgument,
        sources: Vec::new(),
    };
    ResolvedPaths {
        settings: file("settings.toml"),
        state: file("state.json"),
        definitions: root.join("definitions"),
        plugins: PathBuf::from(root).join("plugins"),
        themes: root.join("themes"),
    }
}
