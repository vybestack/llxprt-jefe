use super::AppState;
use crate::domain::{GitHubRepoRefError, Id, Repository, TypedPortValue, TypedValue};
use crate::workbench::{
    ISSUES_LIST_PANEL, PULL_REQUESTS_LIST_PANEL, PanelId, PortId, PortRef, PortValue,
    SELECTION_PORT, SourceIntent,
};

/// Authenticated runtime request to change one open screen's relationship state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipCommand {
    /// Exact open screen that produced the request.
    pub open_screen_id: crate::workbench::OpenScreenId,
    /// Exact panel instance that produced the request.
    pub panel_instance_id: crate::workbench::PanelInstanceId,
    /// Screen generation observed by the producer.
    pub generation: u64,
    /// Published owner that produced the request.
    pub owner_id: Id,
    /// Pure descriptor-level transition to validate and apply.
    pub intent: SourceIntent,
}

/// Why an authenticated relationship command was rejected before mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipCommandError {
    /// The command belongs to another open screen instance.
    StaleScreen,
    /// The command observed another screen generation.
    StaleGeneration,
    /// The command's panel instance does not own its declared endpoint.
    WrongPanel,
    /// The producer does not own the declared endpoint.
    WrongOwner,
    /// The declared endpoint does not exist.
    UnknownPort,
    /// The pure relationship transition was refused.
    Propagation(crate::workbench::PropagationAbort),
}

impl std::fmt::Display for RelationshipCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleScreen => formatter.write_str("relationship command names a stale screen"),
            Self::StaleGeneration => {
                formatter.write_str("relationship command names a stale generation")
            }
            Self::WrongPanel => formatter.write_str("relationship command names the wrong panel"),
            Self::WrongOwner => formatter.write_str("relationship command names the wrong owner"),
            Self::UnknownPort => formatter.write_str("relationship command names an unknown port"),
            Self::Propagation(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for RelationshipCommandError {}

const ISSUE_TYPE: &str = "github.issue";
const PULL_REQUEST_TYPE: &str = "github.pull-request";

impl AppState {
    /// Validate one producer-correlated command and atomically apply its pure intent.
    pub fn apply_relationship_command(
        &mut self,
        command: RelationshipCommand,
    ) -> Result<Vec<crate::workbench::PortUpdate>, RelationshipCommandError> {
        let current = self.nav.current();
        if command.open_screen_id != current.id {
            return Err(RelationshipCommandError::StaleScreen);
        }
        if command.generation != current.generation {
            return Err(RelationshipCommandError::StaleGeneration);
        }
        let endpoint = command_endpoint(&command.intent);
        let Some(relationships) = current.relationships() else {
            return Err(RelationshipCommandError::WrongPanel);
        };
        if relationships.panel_instance_id(&endpoint.panel) != Some(command.panel_instance_id) {
            return Err(RelationshipCommandError::WrongPanel);
        }
        let Some(descriptor) = self
            .published_workbench()
            .screen_registry()
            .get_identity(current.screen)
        else {
            return Err(RelationshipCommandError::UnknownPort);
        };
        let Some(port) = descriptor.port(endpoint) else {
            return Err(RelationshipCommandError::UnknownPort);
        };
        if port.owner_id != command.owner_id {
            return Err(RelationshipCommandError::WrongOwner);
        }
        self.apply_relationship_intent(command.intent)
            .map_err(RelationshipCommandError::Propagation)
    }

    pub(super) fn publish_selected_resource(
        &mut self,
        panel: &'static str,
        type_id: &'static str,
        semantic_key: Option<String>,
    ) {
        let value = semantic_key.map_or(PortValue::Absent, |key| {
            PortValue::Typed(TypedPortValue {
                type_id: parse_id(type_id),
                schema_version: 1,
                semantic_key: key.clone(),
                value: std::iter::once((parse_id("semantic-key"), TypedValue::String(key)))
                    .collect(),
            })
        });
        let port = PortRef {
            panel: PanelId::parse(panel)
                .unwrap_or_else(|error| unreachable!("compiled panel id is valid: {error}")),
            port: PortId::parse(SELECTION_PORT)
                .unwrap_or_else(|error| unreachable!("compiled port id is valid: {error}")),
        };
        let current = self.nav.current();
        let Some(panel_instance_id) = current
            .relationships()
            .and_then(|relationships| relationships.panel_instance_id(&port.panel))
        else {
            return;
        };
        let Some(owner_id) = self
            .published_workbench()
            .screen_registry()
            .get_identity(current.screen)
            .and_then(|descriptor| descriptor.port(&port))
            .map(|declared| declared.owner_id.clone())
        else {
            return;
        };
        let command = RelationshipCommand {
            open_screen_id: current.id,
            panel_instance_id,
            generation: current.generation,
            owner_id,
            intent: SourceIntent::Publish { port, value },
        };
        if let Err(error) = self.apply_relationship_command(command) {
            self.error_message = Some(error.to_string());
        }
    }

    pub(super) fn sync_issue_selected_resource(&mut self) {
        let selected = self
            .issues_state
            .selected_issue_index()
            .and_then(|index| self.issues_state.issues().get(index))
            .map(|issue| issue.number);
        self.publish_github_resource(ISSUES_LIST_PANEL, ISSUE_TYPE, selected);
    }

    pub(super) fn sync_pr_selected_resource(&mut self) {
        let selected = self
            .prs_state
            .selected_pr_index()
            .and_then(|index| self.prs_state.pull_requests().get(index))
            .map(|pr| pr.number);
        self.publish_github_resource(PULL_REQUESTS_LIST_PANEL, PULL_REQUEST_TYPE, selected);
    }

    fn publish_github_resource(
        &mut self,
        panel: &'static str,
        type_id: &'static str,
        number: Option<u64>,
    ) {
        match github_resource_key(self.selected_repository(), number) {
            Ok(key) => self.publish_selected_resource(panel, type_id, key),
            Err(error) => {
                self.publish_selected_resource(panel, type_id, None);
                self.error_message = Some(error.to_string());
            }
        }
    }

    pub(super) fn sync_current_selected_resource(&mut self) {
        match self.compiled_screen() {
            Some(crate::workbench::ScreenId::Issues) => self.sync_issue_selected_resource(),
            Some(crate::workbench::ScreenId::PullRequests) => self.sync_pr_selected_resource(),
            _ => {}
        }
    }
}
fn command_endpoint(intent: &SourceIntent) -> &PortRef {
    match intent {
        SourceIntent::Publish { port, .. } => port,
        SourceIntent::Activate { target } => target,
    }
}

pub(super) fn github_resource_key(
    repository: Option<&Repository>,
    number: Option<u64>,
) -> Result<Option<String>, GitHubRepoRefError> {
    let (Some(repository), Some(number)) = (repository, number) else {
        return Ok(None);
    };
    let Some(tracker) = repository.effective_issue_pr_repo()? else {
        return Ok(None);
    };
    Ok(Some(format!(
        "{}/{}#{number}",
        tracker.owner(),
        tracker.repo()
    )))
}

fn parse_id(value: &'static str) -> Id {
    Id::parse(value).unwrap_or_else(|error| unreachable!("compiled resource id is valid: {error}"))
}
