//! The zero-agent Agent Types availability pane on the shared runtime (#734).
//!
//! Pre-cutover, `src/ui/screens/dashboard.rs:171-188` replaced the agent list,
//! the embedded terminal and the preview with a full-width availability pane
//! whenever `state.agents.is_empty() && !state.agent_type_availability.is_empty()`.
//! #715 deleted that screen and the pane lost its mount, which is what
//! `dev-docs/tmux-scenarios/pid-commit-corner.json` reports as
//! `HAR-E006: frame does not contain 'Agent Types'`.
//!
//! These tests drive the shipped path end to end — declared descriptor,
//! resolver geometry, host-control projection — rather than any renderer, so
//! they fail exactly where a lost mount shows up.

use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::agent_definition::{AgentDefinition, Availability};
use crate::provider_panel_view::{PanelProjection, ProviderScreenView, project_current_screen};
use crate::screen_layout::{hidden_panel_ids, resolve_screen};
use crate::state::AppState;
use crate::workbench::{DASHBOARD_IDENTITY, ScreenDescriptor};

/// The size `pid-commit-corner.json` runs at.
const SCENARIO_COLS: u16 = 100;
/// The size `pid-commit-corner.json` runs at.
const SCENARIO_ROWS: u16 = 32;

fn definition(display_name: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|candidate| candidate.display_name == display_name)
        .unwrap_or_else(|| panic!("shipped definition {display_name} must exist"))
}

fn installed(display_name: &str) -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::new(
        &definition(display_name),
        true,
        Availability::InstalledCompatible {
            identity: format!("{display_name} 9.9.9"),
            generation: 1,
        },
    )
}

fn not_found(display_name: &str) -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::not_found(&definition(display_name), true, 1)
}

/// A first-run dashboard: no repositories, no agents, and a startup probe that
/// has published observations for all four shipped definitions.
fn zero_agent_state() -> AppState {
    let mut state = AppState::test_fixture();
    state.agent_type_availability = vec![
        not_found("Claude Code"),
        installed("Code Puppy"),
        not_found("Codex CLI"),
        installed("LLxprt"),
    ];
    state
}

/// The same probe result, on a workspace that already has an agent.
fn populated_state() -> AppState {
    let mut state = zero_agent_state();
    state.repositories = vec![crate::test_support::host_panel_repository("one")];
    state.agents = vec![crate::test_support::host_panel_agent(
        "Alpha One",
        "repo-one",
        crate::domain::AgentStatus::Running,
    )];
    state.selected_repository_index = Some(0);
    state
}

fn dashboard_descriptor(state: &AppState) -> ScreenDescriptor {
    state
        .published_workbench()
        .screen_registry()
        .get_identity(DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("the dashboard descriptor must be published"))
        .clone()
}

fn projected(state: &mut AppState, cols: u16, rows: u16) -> ProviderScreenView {
    state.resolved_layout = resolve_screen(state, cols, rows);
    let layout = state
        .resolved_layout
        .clone()
        .unwrap_or_else(|| panic!("the dashboard must resolve at {cols}x{rows}"));
    let descriptor = dashboard_descriptor(state);
    project_current_screen(state, &descriptor, &layout)
        .unwrap_or_else(|error| panic!("the dashboard must project: {error}"))
}

fn visible_titles(view: &ProviderScreenView) -> Vec<String> {
    view.panels
        .iter()
        .filter(|panel| panel.visible)
        .map(|panel| panel.title.clone())
        .collect()
}

fn pane<'a>(view: &'a ProviderScreenView, title: &str) -> &'a PanelProjection {
    view.panels
        .iter()
        .find(|panel| panel.visible && panel.title == title)
        .unwrap_or_else(|| {
            panic!(
                "a visible {title:?} pane must be projected, visible panes were {:?}",
                visible_titles(view)
            )
        })
}

#[test]
fn the_zero_agent_dashboard_mounts_the_agent_types_availability_pane() {
    let mut state = zero_agent_state();

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let titles = visible_titles(&view);
    assert!(
        titles.iter().any(|title| title == "Agent Types"),
        "a dashboard with no agents must mount the availability pane, visible panes were {titles:?}"
    );
}

#[test]
fn the_availability_pane_replaces_the_agent_list_terminal_and_preview() {
    let mut state = zero_agent_state();

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let titles = visible_titles(&view);
    for replaced in ["Agents", "Terminal", "Agent preview"] {
        assert!(
            !titles.iter().any(|title| title == replaced),
            "the availability pane replaced {replaced} pre-cutover, visible panes were {titles:?}"
        );
    }
}

#[test]
fn the_availability_rows_carry_the_literals_the_scenario_corpus_boots_on() {
    // Every boot wait in the corpus is one of these literals; the two-space
    // separator and the `, enabled` suffix are load-bearing text, not
    // formatting taste.
    for (cols, rows) in [(SCENARIO_COLS, SCENARIO_ROWS), (120, 40), (130, 40)] {
        let mut state = zero_agent_state();
        let view = projected(&mut state, cols, rows);
        let rendered = pane(&view, "Agent Types").lines.join("\n");
        for literal in [
            "Code Puppy  Installed",
            "Code Puppy  Installed, enabled",
            "LLxprt  Installed",
            "LLxprt  Installed, enabled",
            "Claude Code  Not found, enabled",
        ] {
            assert!(
                rendered.contains(literal),
                "at {cols}x{rows} the availability pane must render {literal:?}, got:\n{rendered}"
            );
        }
        assert!(
            !rendered.contains("Probe error"),
            "a clean probe must not report an error at {cols}x{rows}, got:\n{rendered}"
        );
    }
}

#[test]
fn the_availability_pane_spans_every_column_the_sidebar_leaves() {
    let mut state = zero_agent_state();

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let sidebar = pane(&view, "Repositories").chrome;
    let availability = pane(&view, "Agent Types").chrome;
    assert_eq!(
        availability.col,
        sidebar.col + sidebar.width,
        "the pane starts where the sidebar ends"
    );
    assert_eq!(
        availability.col + availability.width,
        SCENARIO_COLS,
        "the pane runs to the right edge"
    );
    assert_eq!(
        (availability.row, availability.height),
        (sidebar.row, sidebar.height),
        "the pane is as tall as the sidebar"
    );
}

#[test]
fn a_dashboard_with_an_agent_keeps_the_agent_list_and_hides_the_availability_pane() {
    let mut state = populated_state();

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let titles = visible_titles(&view);
    assert!(
        !titles.iter().any(|title| title == "Agent Types"),
        "one agent is enough to restore the ordinary dashboard, visible panes were {titles:?}"
    );
    for restored in ["Agents", "Terminal", "Agent preview"] {
        assert!(
            titles.iter().any(|title| title == restored),
            "{restored} must come back once an agent exists, visible panes were {titles:?}"
        );
    }
}

#[test]
fn an_unpublished_probe_keeps_the_agent_list_rather_than_an_empty_pane() {
    // Before the startup probe answers there is nothing to show, and the
    // pre-cutover condition required a non-empty snapshot for exactly that
    // reason.
    let mut state = AppState::test_fixture();

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let titles = visible_titles(&view);
    assert!(
        !titles.iter().any(|title| title == "Agent Types"),
        "an empty availability snapshot must not mount the pane, visible panes were {titles:?}"
    );
    assert!(
        titles.iter().any(|title| title == "Agents"),
        "the ordinary dashboard must survive an unpublished probe, visible panes were {titles:?}"
    );
}

#[test]
fn a_shell_overlay_keeps_the_required_terminal_visible() {
    // The overlay and the availability pane can never both apply — an overlay
    // needs a running agent — but the hiding rules must not be able to leave
    // the dashboard with no content pane at all if they ever did.
    let mut state = zero_agent_state();
    state.open_shell_overlay(crate::domain::AgentId("agent-1".to_owned()));

    let view = projected(&mut state, SCENARIO_COLS, SCENARIO_ROWS);

    let titles = visible_titles(&view);
    assert!(
        titles.iter().any(|title| title == "Terminal"),
        "the required terminal survives every hiding rule, visible panes were {titles:?}"
    );
    assert!(
        !titles.iter().any(|title| title == "Agent Types"),
        "the shell overlay owns the workspace, visible panes were {titles:?}"
    );
}

#[test]
fn every_panel_the_dashboard_hiding_rule_names_is_declared_by_the_dashboard() {
    // Whether an identity is written as a literal or read out of the layout,
    // a rule that addressed a panel the dashboard does not declare would hide
    // nothing, and the pane would silently never appear again.
    let mut named = 0_usize;
    let fixtures: [fn() -> AppState; 3] =
        [zero_agent_state, populated_state, AppState::test_fixture];
    for fixture in fixtures {
        let state = fixture();
        let descriptor = dashboard_descriptor(&state);
        for panel in hidden_panel_ids(&state) {
            assert!(
                descriptor
                    .panels
                    .iter()
                    .any(|declared| declared.id == panel),
                "the dashboard hides {panel}, which its descriptor does not declare"
            );
            named += 1;
        }
    }
    assert!(
        named > 0,
        "the dashboard must exercise its conditional hiding rules"
    );
}
