//! Shared panic diagnostics for unit tests that exercise fallible contracts.

use std::fmt::Debug;
use std::sync::Arc;

/// Build one explicit, process-free declaration fixture for state unit tests.
#[must_use]
pub fn published_workbench() -> Arc<crate::published_workbench::PublishedWorkbench> {
    use crate::domain::plugin::HostTriple;
    use crate::persistence::paths::{PathProvenance, ResolvedFile, ResolvedPaths};
    use crate::persistence::plugin_inventory::scan;
    use crate::persistence::settings_document::PublishedSettings;
    use crate::runtime::provider::Containment;
    use crate::startup_candidate::{WorkbenchCandidateRequest, build_workbench_candidate};

    let root = std::env::temp_dir().join("jefe-unit-workbench");
    let paths = ResolvedPaths {
        settings: ResolvedFile {
            path: root.join("settings.toml"),
            provenance: PathProvenance::ConfigArgument,
            sources: Vec::new(),
        },
        state: ResolvedFile {
            path: root.join("state.json"),
            provenance: PathProvenance::ConfigArgument,
            sources: Vec::new(),
        },
        definitions: root.join("definitions-absent"),
        plugins: root.join("plugins-absent"),
        themes: root.join("themes-absent"),
    };
    let inventory = scan(&[]);
    let settings = PublishedSettings::default();
    let candidate = build_workbench_candidate(&WorkbenchCandidateRequest {
        paths: &paths,
        inventory: &inventory,
        settings: &settings,
        host: HostTriple::current(),
        containment: Containment {
            home: root.join("home"),
            tmpdir: root.join("tmp"),
            working_dir: root.join("work"),
            locale: "C".to_owned(),
            host_api: crate::VERSION.to_owned(),
        },
    })
    .unwrap_or_else(|error| panic!("unit workbench fixture must compose: {error}"));
    Arc::new(candidate)
}

pub trait Must<T> {
    fn must(self, context: &str) -> T;
}

impl<T, E: Debug> Must<T> for Result<T, E> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }
}

impl<T> Must<T> for Option<T> {
    fn must(self, context: &str) -> T {
        self.unwrap_or_else(|| panic!("{context}"))
    }
}

pub trait MustErr<E> {
    fn must_err(self, context: &str) -> E;
}

impl<T: Debug, E> MustErr<E> for Result<T, E> {
    fn must_err(self, context: &str) -> E {
        match self {
            Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
            Err(error) => error,
        }
    }
}
