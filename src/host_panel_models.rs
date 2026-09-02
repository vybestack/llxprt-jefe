//! Host-owned models for definable panel types in the shared screen runtime.

use crate::domain::InternalId;
use crate::domain::action_registry::{ActionId, InternalActionId};
use crate::domain::plugin::field::{Field, InternalField};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{
    Affordance, DetailBody, DetailMetadata, FormBody, ListBody, ListItem, PanelBody,
};
use crate::state::AppState;
use crate::workbench::HostPanelModelSource;

pub struct HostPanelModel {
    pub(crate) title: String,
    pub(crate) body: PanelBody,
    pub(crate) action_affordances: Vec<Affordance>,
    pub(crate) selected_id: Option<Id>,
    pub(crate) scroll_offset: u32,
}

#[must_use]
pub fn project_host_panel(state: &AppState, source: HostPanelModelSource) -> HostPanelModel {
    match source {
        HostPanelModelSource::RepositoryList => repository_list(state),
        HostPanelModelSource::SearchInput => search_input(state),
        HostPanelModelSource::AgentList => agent_list(state),
        HostPanelModelSource::AgentPreview => agent_preview(state),
    }
}

fn repository_list(state: &AppState) -> HostPanelModel {
    let visible = state.visible_repository_indices();
    let items = visible
        .iter()
        .enumerate()
        .filter_map(|(visible_index, repository_index)| {
            state
                .repositories
                .get(*repository_index)
                .map(|repository| ListItem {
                    id: Id::internal_indexed(InternalId::RepositoryItem, visible_index),
                    label: repository.name.clone(),
                    // The dashboard sidebar is one row per repository; a
                    // description would project as a second row.
                    description: None,
                    status: Some(
                        state
                            .visible_agent_count_for_repository(&repository.id)
                            .to_string(),
                    ),
                    actions: Vec::new(),
                })
        })
        .collect();
    let selected_id = state
        .selected_repository_visible_index()
        .map(|index| Id::internal_indexed(InternalId::RepositoryItem, index));
    HostPanelModel {
        title: "Repositories".to_owned(),
        body: PanelBody::List(ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id,
        scroll_offset: state.repository_scroll_offset,
    }
}

fn search_input(state: &AppState) -> HostPanelModel {
    let field = Field::internal(InternalField::SearchQuery);
    let mut values = TypedMap::new();
    values.insert(
        field.id().clone(),
        TypedValue::String(state.search_query().unwrap_or_default().to_owned()),
    );
    let submit_action = ActionId::internal(InternalActionId::OverlaySubmit);
    HostPanelModel {
        title: "Search".to_owned(),
        body: PanelBody::Form(FormBody {
            fields: vec![field],
            values,
            field_errors: Vec::new(),
            submit_action: submit_action.clone(),
        }),
        action_affordances: vec![Affordance {
            id: Id::internal(InternalId::OverlaySubmit),
            label: "Search".to_owned(),
            action_id: submit_action,
            arguments: None,
            enabled: true,
            unavailable_reason: None,
        }],
        selected_id: None,
        scroll_offset: 0,
    }
}

fn agent_list(state: &AppState) -> HostPanelModel {
    let indices = state
        .selected_repository()
        .map_or_else(Vec::new, |repository| {
            state.agent_indices_for_repository(&repository.id)
        });
    let items = indices
        .iter()
        .enumerate()
        .filter_map(|(local_index, agent_index)| {
            state.agents.get(*agent_index).map(|agent| ListItem {
                id: Id::internal_indexed(InternalId::AgentItem, local_index),
                label: agent.name.clone(),
                // The dashboard sidebar is one row per agent; a description
                // would project as a second row.
                description: None,
                status: Some(format!("{:?}", agent.status)),
                actions: Vec::new(),
            })
        })
        .collect();
    let selected_id = state.selected_agent_index.and_then(|selected| {
        indices
            .iter()
            .position(|index| *index == selected)
            .map(|index| Id::internal_indexed(InternalId::AgentItem, index))
    });
    HostPanelModel {
        title: "Agents".to_owned(),
        body: PanelBody::List(ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id,
        scroll_offset: state.agent_scroll_offset,
    }
}

fn agent_preview(state: &AppState) -> HostPanelModel {
    let (document, metadata) = state.selected_agent().map_or_else(
        || ("No agent selected".to_owned(), Vec::new()),
        |agent| {
            (
                agent.description.clone(),
                vec![
                    DetailMetadata {
                        label: "Status".to_owned(),
                        value: format!("{:?}", agent.status),
                    },
                    DetailMetadata {
                        label: "Work directory".to_owned(),
                        value: agent.work_dir.display().to_string(),
                    },
                ],
            )
        },
    );
    HostPanelModel {
        title: "Agent preview".to_owned(),
        body: PanelBody::Detail(DetailBody {
            document,
            metadata,
            actions: Vec::new(),
        }),
        action_affordances: Vec::new(),
        selected_id: None,
        scroll_offset: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #715 regression: a non-empty description projects as an extra
    /// row under each item, so the slug made every repository read as two
    /// rows (a duplicate where name matches slug). The dashboard sidebar is
    /// one row per item.
    #[test]
    fn dashboard_sidebar_rows_are_one_line_per_item() {
        let mut state = crate::state::AppState::new(crate::test_support::published_workbench());
        let repository = crate::domain::Repository::new(
            crate::domain::RepositoryId("repo-one".to_owned()),
            crate::domain::shipped_agent_type(1),
            crate::domain::TypedMap::new(),
            "One Repo".to_owned(),
            "one-repo".to_owned(),
            std::path::PathBuf::from("/tmp/one-repo"),
        );
        let agent = crate::domain::Agent::new(
            crate::domain::AgentId("agent-one".to_owned()),
            repository.id.clone(),
            crate::domain::shipped_agent_type(1),
            crate::domain::TypedMap::new(),
            "One Agent".to_owned(),
            std::path::PathBuf::from("/tmp/one-agent"),
        );
        state.repositories = vec![repository];
        state.agents = vec![agent];
        state.selected_repository_index = Some(0);

        let repository_model = project_host_panel(&state, HostPanelModelSource::RepositoryList);
        let PanelBody::List(repository_body) = repository_model.body else {
            panic!("repository sidebar must project a list body");
        };
        assert_eq!(repository_body.items.len(), 1, "one row per repository");
        assert!(
            repository_body.items[0].description.is_none(),
            "repository rows must not carry a second line: {:?}",
            repository_body.items[0].description
        );
        assert_eq!(repository_body.items[0].label, "One Repo");
        assert_eq!(repository_body.items[0].status.as_deref(), Some("1"));

        let agent_model = project_host_panel(&state, HostPanelModelSource::AgentList);
        let PanelBody::List(agent_body) = agent_model.body else {
            panic!("agent sidebar must project a list body");
        };
        assert_eq!(agent_body.items.len(), 1, "one row per agent");
        assert!(
            agent_body.items[0].description.is_none(),
            "agent rows must not carry a second line: {:?}",
            agent_body.items[0].description
        );
        assert_eq!(agent_body.items[0].label, "One Agent");
    }
}
