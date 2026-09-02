//! Shared panic diagnostics and domain fixtures for unit tests that exercise
//! fallible contracts.

use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::observation::{
    AgentObservation, FieldState, NativeActivityState, NativeActivityValue, ObservationHealth,
    Provenance, Wait, WaitReason,
};
use crate::domain::{Agent, AgentId, AgentStatus, AgentTypeId, Repository, RepositoryId, TypedMap};

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

/// One repository fixture shared by the host-panel model tests.
#[must_use]
pub fn host_panel_repository(id: &str) -> Repository {
    Repository::new(
        RepositoryId(format!("repo-{id}")),
        AgentTypeId::default(),
        TypedMap::default(),
        format!("Repo {id}"),
        format!("repo-{id}"),
        PathBuf::from("/tmp"),
    )
}

/// One agent fixture owned by `repository_id`, with `status` set.
#[must_use]
pub fn host_panel_agent(name: &str, repository_id: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(name.to_owned()),
        RepositoryId(repository_id.to_owned()),
        AgentTypeId::default(),
        TypedMap::default(),
        name.to_owned(),
        PathBuf::from("/tmp"),
    );
    agent.status = status;
    agent
}

/// A live observation whose native activity is `Acting`.
#[must_use]
pub fn working_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Acting,
            },
        ),
        ..AgentObservation::default()
    }
}

/// A live observation whose native activity is `Idle`, every field known.
#[must_use]
pub fn ready_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle,
            },
        ),
        wait: FieldState::known(Provenance::Authoritative, None),
        turn: FieldState::known(Provenance::Authoritative, None),
        terminal: FieldState::known(Provenance::Authoritative, None),
        ..AgentObservation::default()
    }
}

/// A live observation waiting on a permission prompt.
#[must_use]
pub fn waiting_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        wait: FieldState::known(
            Provenance::Authoritative,
            Some(Wait {
                reason: WaitReason::Permission,
            }),
        ),
        ..AgentObservation::default()
    }
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
