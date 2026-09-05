//! Dashboard Agents-row host-model tests for issue #730.

use crate::dashboard_git_info::DashboardGitInfoSnapshot;
use crate::domain::{Agent, AgentStatus, Id, InternalId, LaunchSignatureV1, RuntimeBinding};
use crate::git_info::GitRepoInfo;
use crate::host_controls::HostControlSpanRole;
use crate::host_panel_models::project_host_panel;
use crate::provider_panel_view::{ModelProjectionInput, ProjectedRow, project_model_rows};
use crate::runtime::provider::protocol::{ListItem, ListItemGlyphRole, PanelBody};
use crate::state::{AppState, DashboardGrabPane};
use crate::test_support::{host_panel_agent, host_panel_repository};
use crate::workbench::HostPanelModelSource;

fn state_with_agents(agents: Vec<Agent>) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![host_panel_repository("alpha")];
    state.agents = agents;
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state
}

fn confirmed_running(name: &str) -> Agent {
    let mut agent = host_panel_agent(name, "repo-alpha", AgentStatus::Running);
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: format!("jefe-{name}"),
        launch_signature: LaunchSignatureV1::default(),
        attached: false,
        last_seen: None,
        pane_identity: None,
        worker_identity: None,
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    });
    agent
}

fn projected_items(state: &AppState, git: Option<&DashboardGitInfoSnapshot>) -> Vec<ListItem> {
    let model = project_host_panel(state, HostPanelModelSource::AgentList, git);
    let PanelBody::List(body) = model.body else {
        panic!("Agents must project a List body");
    };
    body.items
}

fn projected_rows(
    state: &AppState,
    git: Option<&DashboardGitInfoSnapshot>,
    width: usize,
) -> Vec<ProjectedRow> {
    let model = project_host_panel(state, HostPanelModelSource::AgentList, git);
    project_model_rows(ModelProjectionInput {
        body: &model.body,
        affordances: &model.action_affordances,
        description: None,
        loading: false,
        stale: false,
        selected_id: model.selected_id.as_ref(),
        marked_id: model.grabbed_id.as_ref(),
        form_draft: None,
        body_width: width,
    })
}

fn status_glyph_cases() -> Vec<(Agent, &'static str, ListItemGlyphRole)> {
    vec![
        (
            confirmed_running("confirmed"),
            "*",
            ListItemGlyphRole::Bright,
        ),
        (
            host_panel_agent("dead", "repo-alpha", AgentStatus::Dead),
            "x",
            ListItemGlyphRole::Red,
        ),
        (
            host_panel_agent("queued", "repo-alpha", AgentStatus::Queued),
            "o",
            ListItemGlyphRole::Dim,
        ),
        (
            host_panel_agent("unconfirmed", "repo-alpha", AgentStatus::Running),
            "~",
            ListItemGlyphRole::Yellow,
        ),
        (
            host_panel_agent("completed", "repo-alpha", AgentStatus::Completed),
            "+",
            ListItemGlyphRole::Bright,
        ),
        (
            host_panel_agent("errored", "repo-alpha", AgentStatus::Errored),
            "!",
            ListItemGlyphRole::Red,
        ),
        (
            host_panel_agent("waiting", "repo-alpha", AgentStatus::Waiting),
            "?",
            ListItemGlyphRole::Yellow,
        ),
        (
            host_panel_agent("paused", "repo-alpha", AgentStatus::Paused),
            "-",
            ListItemGlyphRole::Blue,
        ),
        (
            host_panel_agent("lost", "repo-alpha", AgentStatus::ServerLost),
            "!",
            ListItemGlyphRole::Red,
        ),
    ]
}

#[test]
fn agent_rows_map_status_to_the_host_glyph_and_role_without_textual_status() {
    let statuses = status_glyph_cases();
    let expected: Vec<_> = statuses
        .iter()
        .map(|(_, glyph, role)| (*glyph, *role))
        .collect();
    let state = state_with_agents(statuses.into_iter().map(|(agent, _, _)| agent).collect());

    let items = projected_items(&state, None);
    let actual: Vec<_> = items
        .iter()
        .map(|item| {
            let glyph = item
                .glyph
                .as_ref()
                .unwrap_or_else(|| panic!("agent row must carry a glyph"));
            assert_eq!(item.status, None, "agent rows must not carry status text");
            (glyph.text.as_str(), glyph.role)
        })
        .collect();

    assert_eq!(actual, expected);
}

#[test]
fn agent_rows_show_shortcut_badges_only_for_slots_one_through_nine() {
    let agents = [0, 1, 9, 10]
        .into_iter()
        .map(|slot| {
            let mut agent =
                host_panel_agent(&format!("slot-{slot}"), "repo-alpha", AgentStatus::Dead);
            agent.shortcut_slot = Some(slot);
            agent
        })
        .chain(std::iter::once(host_panel_agent(
            "no-slot",
            "repo-alpha",
            AgentStatus::Dead,
        )))
        .collect();
    let state = state_with_agents(agents);

    let badges: Vec<_> = projected_items(&state, None)
        .into_iter()
        .map(|item| item.badge)
        .collect();

    assert_eq!(
        badges,
        [None, Some("1".to_owned()), Some("9".to_owned()), None, None]
    );
}

#[test]
fn agent_rows_use_snapshot_git_suffix_and_mark_only_known_dirty_trees() {
    let agents = ["dirty", "clean", "unknown"]
        .into_iter()
        .map(|name| host_panel_agent(name, "repo-alpha", AgentStatus::Dead))
        .collect();
    let state = state_with_agents(agents);
    let snapshot = DashboardGitInfoSnapshot {
        agents: vec![
            GitRepoInfo {
                origin_shortform: Some("owner/repo".to_owned()),
                branch: Some("main".to_owned()),
                dirty: Some(true),
            },
            GitRepoInfo {
                origin_shortform: Some("owner/repo".to_owned()),
                branch: Some("main".to_owned()),
                dirty: Some(false),
            },
            GitRepoInfo {
                origin_shortform: Some("owner/repo".to_owned()),
                branch: Some("main".to_owned()),
                dirty: None,
            },
        ],
        preview: None,
    };

    let suffixes: Vec<_> = projected_items(&state, Some(&snapshot))
        .into_iter()
        .map(|item| item.suffix)
        .collect();

    assert_eq!(
        suffixes,
        [
            Some("  owner/repo @ main *".to_owned()),
            Some("  owner/repo @ main".to_owned()),
            Some("  owner/repo @ main".to_owned()),
        ]
    );
}

#[test]
fn empty_git_snapshot_entry_projects_no_suffix_span() {
    let state = state_with_agents(vec![confirmed_running("no-suffix")]);
    let snapshot = DashboardGitInfoSnapshot {
        agents: vec![GitRepoInfo::default()],
        preview: None,
    };

    let items = projected_items(&state, Some(&snapshot));
    let rows = projected_rows(&state, Some(&snapshot), 80);

    assert_eq!(items[0].suffix, None);
    assert_eq!(rows[0].text, ">> * no-suffix");
    assert!(
        rows[0]
            .spans
            .iter()
            .all(|span| span.role != HostControlSpanRole::Dim)
    );
}

#[test]
fn agent_row_uses_id_as_the_fallback_label() {
    let mut agent = host_panel_agent("agent-fallback", "repo-alpha", AgentStatus::Dead);
    agent.name = "  ".to_owned();
    let state = state_with_agents(vec![agent]);

    let items = projected_items(&state, None);

    assert_eq!(items[0].label, "agent-fallback");
}

#[test]
fn grabbed_agent_id_reaches_the_list_marker_and_overrides_selection() {
    let agent = confirmed_running("grabbed");
    let mut state = state_with_agents(vec![agent]);
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: state.repositories[0].id.clone(),
        local_index: 0,
    });
    let model = project_host_panel(&state, HostPanelModelSource::AgentList, None);
    let expected_id = Id::internal_indexed(InternalId::AgentItem, 0);

    assert_eq!(model.grabbed_id, Some(expected_id.clone()));
    let rows = crate::host_controls::project_control_body_with_marked_id(
        &model.body,
        &model.action_affordances,
        model.selected_id.as_ref(),
        model.grabbed_id.as_ref(),
        None,
        80,
    );

    assert_eq!(rows[0].text, "↕ * grabbed");
}

#[test]
fn selected_agent_row_composes_marker_glyph_badge_name_and_suffix_in_order() {
    let mut agent = confirmed_running("selected");
    agent.shortcut_slot = Some(4);
    let state = state_with_agents(vec![agent]);
    let snapshot = DashboardGitInfoSnapshot {
        agents: vec![GitRepoInfo {
            origin_shortform: Some("owner/repo".to_owned()),
            branch: Some("main".to_owned()),
            dirty: None,
        }],
        preview: None,
    };

    let rows = projected_rows(&state, Some(&snapshot), 80);

    assert_eq!(rows[0].text, ">> * [4] selected  owner/repo @ main");
    assert_eq!(
        rows[0]
            .spans
            .iter()
            .map(|span| (span.text.as_str(), span.role))
            .collect::<Vec<_>>(),
        vec![
            (">> ", HostControlSpanRole::Themed),
            ("*", HostControlSpanRole::Bright),
            (" [4] selected", HostControlSpanRole::Themed),
            ("  owner/repo @ main", HostControlSpanRole::Dim),
        ]
    );
}

#[test]
fn empty_agent_list_projects_no_shared_control_rows() {
    let state = state_with_agents(Vec::new());

    let rows = projected_rows(&state, None, 40);

    assert!(rows.is_empty());
}

#[test]
fn shorter_git_snapshot_keeps_agent_alignment_and_omits_only_missing_suffix() {
    let state = state_with_agents(vec![
        confirmed_running("first"),
        confirmed_running("second"),
    ]);
    let snapshot = DashboardGitInfoSnapshot {
        agents: vec![GitRepoInfo {
            origin_shortform: Some("owner/first".to_owned()),
            branch: Some("main".to_owned()),
            dirty: None,
        }],
        preview: None,
    };

    let rows = projected_rows(&state, Some(&snapshot), 80);

    assert_eq!(rows.len(), 2);
    assert!(rows[0].text.ends_with("  owner/first @ main"));
    assert_eq!(rows[1].text, "   * second");
    assert!(
        rows[1]
            .spans
            .iter()
            .all(|span| span.role != HostControlSpanRole::Dim)
    );
}

#[test]
fn unconfirmed_running_agent_projects_warning_glyph_in_the_shared_row() {
    let state = state_with_agents(vec![host_panel_agent(
        "unconfirmed",
        "repo-alpha",
        AgentStatus::Running,
    )]);

    let rows = projected_rows(&state, None, 40);

    assert_eq!(rows[0].text, ">> ~ unconfirmed");
    assert_eq!(rows[0].spans[1].role, HostControlSpanRole::Yellow);
}
