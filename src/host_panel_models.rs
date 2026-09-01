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
        HostPanelModelSource::WorkbenchCards => workbench_cards(state),
    }
}

/// Borrow every agent as a workbench view input: no git info, with its live
/// observation when one has arrived.
///
/// The status block, the card grid, and the grid's input handlers all order
/// the same agents, so they share this projection instead of re-deriving it.
#[must_use]
pub fn workbench_agent_inputs(state: &AppState) -> Vec<crate::workbench_view::AgentInput<'_>> {
    state
        .agents
        .iter()
        .map(|agent| crate::workbench_view::AgentInput {
            agent,
            git_info: None,
            observation: state.observations.get(&agent.id),
        })
        .collect()
}

/// The STATUS block of the cards screen, projected as a list control.
///
/// Retained by the cutover (maintainer decision on #706). The legacy keymap
/// already treated the rail as "the navigable list", so it pairs with the
/// List control: one checkable row per bucket, checkbox from the active
/// filter mask, count over every agent before filtering so toggled-off
/// buckets stay visible.
fn workbench_status(state: &AppState) -> HostPanelModel {
    let inputs = workbench_agent_inputs(state);
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

/// The workbench card grid, projected as a list control over the grid's own
/// order.
///
/// Retained by the cutover (maintainer decision on #706): the rendered cards
/// keep coming from the workbench projection, while selection, paging, and
/// attach route through the host-panel input path exactly like every other
/// declared list. One item per visible agent; the page token stays live while
/// any agent is visible because the real page count is a render-time fact
/// only the projection can clamp.
fn workbench_cards(state: &AppState) -> HostPanelModel {
    let inputs = workbench_agent_inputs(state);
    let repository_filter = state
        .split_filter
        .as_ref()
        .map(|repository| repository.0.as_str());
    let ordered = crate::workbench_view::ordered_agent_inputs(
        &inputs,
        state.workbench.status_filter.mask(),
        repository_filter,
    );
    let items = ordered
        .iter()
        .enumerate()
        .map(|(position, (bucket, input))| ListItem {
            id: Id::internal_indexed(InternalId::WorkbenchCardItem, position),
            label: input.agent.name.clone(),
            description: None,
            status: Some(bucket.label().to_owned()),
            actions: Vec::new(),
        })
        .collect();
    let selected_id = state
        .selected_agent()
        .and_then(|agent| {
            ordered
                .iter()
                .position(|(_, input)| input.agent.id == agent.id)
        })
        .map(|position| Id::internal_indexed(InternalId::WorkbenchCardItem, position));
    let next_page_token = (!ordered.is_empty()).then(|| "workbench-next-page".to_owned());
    HostPanelModel {
        title: "Workbench".to_owned(),
        body: PanelBody::List(ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token,
        }),
        action_affordances: Vec::new(),
        selected_id,
        // The grid pages rather than scrolls; the page index lives in the
        // workbench state and the projection clamps it at render time.
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
    // A stale selected index must resolve to a row that still exists, the
    // same clamp `workbench_status` applies to the filter cursor.
    let selected_id = state.terminal_manager.selected_index.and_then(|index| {
        rows.len()
            .checked_sub(1)
            .map(|last| Id::internal_indexed(InternalId::SessionItem, index.min(last)))
    });
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
