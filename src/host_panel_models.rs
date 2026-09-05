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
        HostPanelModelSource::AgentTypeAvailability => agent_type_availability(state),
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
                // The count goes in the typed field rather than the label for
                // the reason `repository_list` records: the shared control
                // budgets the label against its suffixes, and the STATUS pane
                // is 20 cells wide, which `[x] Needs you (12)` overflows
                // (#745).
                status: None,
                count: Some(counts[bucket.as_index()]),
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

/// The agent's sidebar display name.
///
/// A restored schema-2 agent with no `name` value must not render as a
/// blank row: the id it was restored under is the only identity the host
/// still knows (#723).
fn agent_display_name(agent: &crate::domain::Agent) -> String {
    if agent.name.trim().is_empty() {
        agent.id.0.clone()
    } else {
        agent.name.clone()
    }
}

/// Wrap budget for the dashboard preview's fixed pane width.
const AGENT_PREVIEW_WIDTH: usize = 30;

/// Cells one preview document row may occupy.
///
/// The pane is `PREVIEW_COLUMNS` (36) wide and `PREVIEW_CHROME` spends two
/// columns of border and padding on each side (`src/workbench/screens.rs`),
/// leaving 32. The rows are budgeted here, by the retained projection, rather
/// than left to the Detail control's wrap: a wrapped `Last reply:` splits the
/// reply across two rows, and the pane — like the pre-cutover one, and like
/// the scenarios that read it — treats the reply as a single row that ends in
/// an ellipsis when it does not fit. The metadata block keeps its own,
/// narrower budget because that one applies to the value after the label
/// split rather than to a whole row.
const AGENT_PREVIEW_DOCUMENT_WIDTH: usize = 32;

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
            count: None,
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

/// The repository sidebar, projected as a list control.
///
/// The agent count is handed to the shared list control as a typed `count`
/// rather than as a `status` value or folded into the label. The shared status
/// suffix is `" [{value}]"` and every control carrying a status *word* pins it
/// that way (`Alpha One [Running]`, a card's `[Working]`); a count is not a
/// status word, and the corpus pins the pre-cutover round form,
/// `LLxprt Jefe (0)` (#745). The retained component the other screens render
/// spells it the same way (`src/ui/components/sidebar.rs`).
///
/// Folding it into the label instead, as #752 did, puts it inside the span the
/// control elides to fit the pane: the sidebar's content rectangle is 18 cells,
/// so a real-length repository name pushed the count off the row entirely. The
/// typed field is protected from that budget.
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
                    status: None,
                    count: Some(state.visible_agent_count_for_repository(&repository.id)),
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
                label: agent_display_name(agent),
                // The dashboard sidebar is one row per agent; a description
                // would project as a second row.
                description: None,
                status: Some(format!("{:?}", agent.status)),
                count: None,
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

/// The startup Agent Types availability pane, projected as a list control.
///
/// Restores the pane the pre-cutover dashboard rendered for a workspace with
/// no agents (`src/ui/screens/dashboard.rs:171-188`, dropped by #715). The
/// rows come from the same pure `agent_status_view` projection the retired
/// renderer consumed, so the status vocabulary the scenario corpus boots on
/// (`Code Puppy  Installed, enabled`) is reproduced rather than re-invented,
/// and the probe's reason becomes the row's second line (#734).
///
/// The whole availability row is composed here rather than handed to the
/// shared list control as a `status` value. The shared suffix is
/// `" [{status}]"`, one space, and every list carrying a status word pins it
/// that way (`Alpha One [Running]`, a card's `[Working]`); the retained
/// pre-cutover renderer spells this row with two
/// (`agent_types_status.rs::status_lines`: `"{name}  {status}, {enablement}
///  {create}"`), and the corpus pins the two-space form. Widening the shared
/// suffix would rewrite every agent and card row, so the availability rows
/// carry their own value formatting instead — the same reason
/// `repository_list` and `workbench_status` carry theirs (#745).
fn agent_type_availability(state: &AppState) -> HostPanelModel {
    let rows =
        crate::agent_status_view::project_agent_type_statuses(&state.agent_type_availability);
    let items = rows
        .iter()
        .enumerate()
        .map(|(index, row)| ListItem {
            id: Id::internal_indexed(InternalId::AgentTypeItem, index),
            label: format!(
                "{}  {}, {}  [{}]",
                row.display_name,
                row.status_text,
                if row.enabled { "enabled" } else { "disabled" },
                if row.create_enabled {
                    "Create enabled"
                } else {
                    "Create disabled"
                }
            ),
            description: row.reason.as_ref().map(|reason| {
                row.error_code
                    .map_or_else(|| reason.clone(), |code| format!("{code}  {reason}"))
            }),
            status: None,
            count: None,
            actions: Vec::new(),
        })
        .collect();
    // A republished, shorter snapshot can leave the state-owned cursor past
    // the last row; the same clamp `workbench_status` applies to its filter
    // cursor keeps a marker on screen.
    let selected_id = rows.len().checked_sub(1).map(|last| {
        Id::internal_indexed(
            InternalId::AgentTypeItem,
            state.selected_agent_type_index.min(last),
        )
    });
    HostPanelModel {
        title: "Agent Types".to_owned(),
        body: PanelBody::List(ListBody {
            items,
            selected_id: selected_id.clone(),
            next_page_token: None,
        }),
        action_affordances: Vec::new(),
        selected_id,
        scroll_offset: 0,
    }
}

fn agent_preview(state: &AppState) -> HostPanelModel {
    let Some(agent) = state.selected_agent() else {
        return HostPanelModel {
            title: "Agent preview".to_owned(),
            body: PanelBody::Detail(DetailBody {
                document: "No agent selected".to_owned(),
                metadata: Vec::new(),
                actions: Vec::new(),
            }),
            action_affordances: Vec::new(),
            selected_id: None,
            scroll_offset: 0,
        };
    };
    let git_info = crate::dashboard_git_info::resolve_preview_git_info(state);
    let observation = state.observations.get(&agent.id);
    // Retained preview_view owns the accepted field set (Name/Status/Repo/
    // Branch/Dir); the projection consumes its structured rows and budgets
    // each value on its own, so truncation can never eat the delimiter or
    // drop a row.
    let metadata =
        crate::preview_view::preview_metadata(Some(agent), git_info.as_ref(), observation)
            .into_iter()
            .map(|(label, value)| DetailMetadata {
                label: label.to_owned(),
                value: crate::list_viewport::fit_text_to_width(&value, AGENT_PREVIEW_WIDTH),
            })
            .collect();
    HostPanelModel {
        title: "Agent preview".to_owned(),
        body: PanelBody::Detail(DetailBody {
            document: agent_preview_document(agent, git_info.as_ref(), observation),
            metadata,
            actions: Vec::new(),
        }),
        action_affordances: Vec::new(),
        selected_id: None,
        scroll_offset: 0,
    }
}

/// The preview's document: the agent's description, then the rows the
/// metadata block does not carry — turn elapsed, the `Todo:` block, and the
/// last reply.
///
/// Those rows are taken from the retained `preview_view` projection rather
/// than recomputed, so the todo markers, the stale/unsupported/unknown arms
/// and the elapsed format cannot drift from the module that owns them, and
/// that module cannot silently lose its last live caller the way it did in
/// the #715 cutover (#733).
///
/// The rows are budgeted here, at [`AGENT_PREVIEW_DOCUMENT_WIDTH`], rather
/// than handed over untruncated for the shared Detail control to wrap. Both
/// alternatives were measured against the corpus and both fail it: an
/// unbudgeted `Last reply: Native LLxprt JSP reply` wraps onto a second row
/// and splits the 19-cell prefix `jsp-llxprt-preview-native` waits on, and the
/// metadata block's narrower per-value budget cuts `Last reply: JSP preview is
/// wired` short of the full row `jsp-llxprt-preview` asserts.
fn agent_preview_document(
    agent: &crate::domain::Agent,
    git_info: Option<&crate::git_info::GitRepoInfo>,
    observation: Option<&crate::domain::observation::AgentObservation>,
) -> String {
    let header_rows =
        crate::preview_view::preview_metadata(Some(agent), git_info, observation).len();
    let mut document = agent.description.clone();
    for row in crate::preview_view::build_preview_view(
        Some(agent),
        git_info,
        observation,
        AGENT_PREVIEW_DOCUMENT_WIDTH,
    )
    .lines
    .into_iter()
    .skip(header_rows)
    {
        document.push('\n');
        document.push_str(&row);
    }
    document
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
                count: None,
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
        assert_eq!(
            repository_body.items[0].count,
            Some(1),
            "the count is a typed count the control protects, not label text (#745)"
        );
        assert_eq!(
            repository_body.items[0].status.as_deref(),
            None,
            "a count is not a status word, so the shared `[value]` suffix stays clear (#745)"
        );

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
