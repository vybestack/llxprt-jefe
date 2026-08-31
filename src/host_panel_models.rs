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
        HostPanelModelSource::SessionList => session_list(state),
        HostPanelModelSource::WorkbenchStatus => workbench_status(state),
    }
}

/// The STATUS block of the cards screen, projected as a list control.
///
/// Retained by the cutover (maintainer decision on #706). The legacy keymap
/// already treated the rail as "the navigable list", so it pairs with the
/// List control: one checkable row per bucket, checkbox from the active
/// filter mask, count over every agent before filtering so toggled-off
/// buckets stay visible.
fn workbench_status(state: &AppState) -> HostPanelModel {
    let inputs: Vec<crate::workbench_view::AgentInput<'_>> = state
        .agents
        .iter()
        .map(|agent| crate::workbench_view::AgentInput {
            agent,
            git_info: None,
            observation: state.observations.get(&agent.id),
        })
        .collect();
    let counts = crate::workbench_view::status_bucket_counts(&inputs);
    let filter = state.workbench.status_filter.mask();
    let buckets = crate::workbench_view::STATUS_BLOCK_ORDER;
    let items = buckets
        .iter()
        .enumerate()
        .map(
            |(index, bucket)| crate::runtime::provider::protocol::ListItem {
                id: Id::internal_indexed(InternalId::StatusBucketItem, index),
                label: format!(
                    "{} {}",
                    if filter.allows(*bucket) { "[x]" } else { "[ ]" },
                    bucket.label()
                ),
                description: None,
                status: Some(counts[bucket.as_index()].to_string()),
                actions: Vec::new(),
            },
        )
        .collect();
    let selected_id = Some(Id::internal_indexed(
        InternalId::StatusBucketItem,
        state.workbench.filter_cursor.min(buckets.len() - 1),
    ));
    HostPanelModel {
        title: "STATUS".to_owned(),
        body: PanelBody::List(crate::runtime::provider::protocol::ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id,
        scroll_offset: 0,
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
                    description: Some(repository.slug.clone()),
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
                description: Some(agent.description.clone()),
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

fn session_list(state: &AppState) -> HostPanelModel {
    let rows = crate::state::project_managed_shell_rows(state);
    let items = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let label = if row.close_only {
                format!("{} (close-only)", row.agent_name)
            } else {
                row.agent_name.clone()
            };
            ListItem {
                id: Id::internal_indexed(InternalId::SessionItem, index),
                label,
                description: Some(format!(
                    "{} · {} · {}{}",
                    row.repository_name,
                    row.work_dir,
                    row.status_label,
                    if row.close_only {
                        " · dead/non-running"
                    } else {
                        ""
                    }
                )),
                status: Some(row.status_label.clone()),
                actions: Vec::new(),
            }
        })
        .collect();
    let selected_id = state
        .terminal_manager
        .selected_index
        .map(|index| Id::internal_indexed(InternalId::SessionItem, index));
    HostPanelModel {
        title: "Terminal Manager".to_owned(),
        body: PanelBody::List(ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id,
        scroll_offset: state.session_scroll_offset,
    }
}
