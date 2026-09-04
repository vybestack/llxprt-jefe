//! Dashboard chrome host-panel projection tests (issue #723).
//!
//! The dashboard preview and sidebar rows are host-owned models, so their
//! content contracts are proven here: the preview must carry the retained
//! preview_view field set, and an agent restored without a name value must
//! fall back to its id instead of rendering a blank sidebar row.
//!
//! Issue #733 extends the same subject: the preview's `Branch:` row must come
//! from the branch probe rather than the origin-only constructor, and the
//! turn-elapsed row, `Todo:` block and last reply the retained projection
//! computes must reach the shipped panel.

use crate::domain::AgentStatus;
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::PanelBody;
use crate::state::AppState;
use crate::test_support::{host_panel_agent, host_panel_repository};
use crate::workbench::HostPanelModelSource;

fn state_with_selected_agent(agent: crate::domain::Agent) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let repository = host_panel_repository("alpha");
    state.repositories = vec![repository];
    state.agents = vec![agent];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state
}

/// Issue #723 fix 3: the dashboard agent-preview panel renders the retained
/// preview_view field set — Name, Status, Repo, Branch, Dir — instead of the
/// two-line Status/Work directory stub #715 left behind.
#[test]
fn dashboard_preview_projects_the_full_preview_view_field_set() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    for expected in ["Name", "Status", "Repo", "Branch", "Dir"] {
        assert!(
            body.metadata.iter().any(|row| row.label == expected),
            "preview must carry the `{expected}:` field, got {:?}",
            body.metadata
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>()
        );
    }
}

/// Issue #723 fix 5: a restored schema-2 agent with empty values yields no
/// display name; the sidebar row must fall back to the agent id so the row
/// is never blank.
#[test]
fn dashboard_sidebar_falls_back_to_the_agent_id_when_values_have_no_name() {
    // A schema-2 restore rebuilds the agent with an id and no `name` value,
    // so the fixture pins exactly that shape: named id, empty display name.
    let mut agent = host_panel_agent("agent-x", "repo-alpha", AgentStatus::Dead);
    agent.name = String::new();
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentList);
    let PanelBody::List(body) = &model.body else {
        panic!(
            "agent sidebar must project a list body, got {:?}",
            model.body.kind()
        );
    };
    assert_eq!(
        body.items.first().map(|item| item.label.as_str()),
        Some("agent-x"),
        "a blank display name must fall back to the agent id"
    );
}

/// Issue #723 OCR fix: the preview metadata comes from preview_view's
/// structured rows, and the fixed pane-width budget applies to the value
/// after the label/value split. A truncated value can therefore never eat a
/// delimiter or silently drop a row, and an over-width Dir still gets the
/// full 30-cell budget instead of the 25 the `Dir: ` prefix used to spend.
#[test]
fn dashboard_preview_metadata_budgets_values_after_the_label_split() {
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    agent.work_dir =
        std::path::PathBuf::from("/tmp/jefe/workdirs/repo-alpha-very-long-checkout-path");
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    let labels: Vec<&str> = body.metadata.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["Name", "Status", "Repo", "Branch", "Dir"]);
    let dir = &body.metadata[4];
    assert_eq!(dir.label, "Dir");
    assert!(
        dir.value.starts_with("/tmp/jefe/workdirs"),
        "the Dir value keeps its leading path cells: {}",
        dir.value
    );
    assert_eq!(
        dir.value.chars().count(),
        30,
        "the width budget applies to the value alone, not the rendered row"
    );
    assert!(dir.value.ends_with('…'));
}

/// Issue #723 OCR fix: the structured accessor carries exactly the accepted
/// five-field set. The live "Turn elapsed" row stays a pane render concern:
/// it needs a clock, so it is not dashboard metadata.
#[test]
fn dashboard_preview_metadata_stays_five_rows_while_a_turn_is_active() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    let mut state = state_with_selected_agent(agent.clone());
    let mut observation = crate::test_support::working_observation();
    observation.turn = crate::domain::observation::FieldState::known(
        crate::domain::observation::Provenance::Authoritative,
        Some(crate::domain::observation::CurrentTurn { elapsed_ms: 5000 }),
    );
    state.observations.insert(agent.id.clone(), observation);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    let labels: Vec<&str> = body.metadata.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Name", "Status", "Repo", "Branch", "Dir"],
        "an active turn must not leak a sixth metadata row"
    );
}

// ── Issue #733: the preview's real branch, todo block, elapsed and last reply ─

/// Panic helper that keeps these tests clippy-clean under `unwrap_used` /
/// `expect_used` (both `warn`, denied under `-D warnings`), matching the
/// convention in `src/recovery_tests.rs`.
trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn run_git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .value_or_panic(&format!("spawn git {args:?}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A real git work tree on `branch` with one commit.
///
/// The branch row is proven against the probe the pre-cutover dashboard used,
/// not against a fixture value, because the defect is precisely that the
/// projection stopped probing. `tests/git_info/real_repository.rs` builds its
/// fixtures the same way.
fn temp_git_repo(branch: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().value_or_panic("create a git work tree");
    let path = dir.path();
    run_git(path, &["init", "--quiet"]);
    run_git(
        path,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    );
    run_git(path, &["config", "user.email", "test@test.test"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["config", "commit.gpgsign", "false"]);
    std::fs::write(path.join("README.md"), "hello\n").value_or_panic("write README");
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "--quiet", "-m", "init"]);
    dir
}

/// An observation carrying everything the preview's document rows are built
/// from: an active turn, a three-item todo list, and a last reply.
///
/// `turn_observed_at` is left unset so the elapsed row is exactly its
/// published anchor and the assertions do not race the clock.
fn preview_observation() -> crate::domain::observation::AgentObservation {
    use crate::domain::observation::{
        BoundedText, CurrentTurn, DisplayedAssistantMessage, FieldState, Provenance, TodoItem,
        TodoList, TodoState,
    };
    let mut observation = crate::test_support::working_observation();
    observation.turn = FieldState::known(
        Provenance::Authoritative,
        Some(CurrentTurn { elapsed_ms: 5_000 }),
    );
    observation.todos = FieldState::known(
        Provenance::Authoritative,
        TodoList {
            revision: 1,
            items: vec![
                TodoItem {
                    text: BoundedText("Implement issue 522".to_owned()),
                    state: TodoState::InProgress,
                },
                TodoItem {
                    text: BoundedText("Ship the preview".to_owned()),
                    state: TodoState::Pending,
                },
            ],
        },
    );
    observation.last_message = FieldState::known(
        Provenance::Authoritative,
        DisplayedAssistantMessage {
            content: BoundedText("JSP preview is wired".to_owned()),
            committed_ms: 0,
        },
    );
    observation
}

/// The rows the shipped panel actually paints, taken through the shared
/// control projection rather than read off the model.
fn projected_preview_rows(state: &AppState, width: usize) -> Vec<String> {
    let model = project_host_panel(state, HostPanelModelSource::AgentPreview);
    crate::host_controls::project_control_body(
        &model.body,
        &model.action_affordances,
        model.selected_id.as_ref(),
        None,
        width,
    )
    .into_iter()
    .map(|row| row.text)
    .collect()
}

fn preview_metadata_value(state: &AppState, label: &str) -> String {
    let model = project_host_panel(state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    body.metadata
        .iter()
        .find(|row| row.label == label)
        .map_or_else(|| "<no such row>".to_owned(), |row| row.value.clone())
}

/// Issue #733 defect 1: `agent_preview` built its git info with
/// `GitRepoInfo::from_configured_origin`, whose constructor hardcodes
/// `branch: None`, so `Branch:` resolved to the `(unknown)` sentinel on every
/// repository. A selected agent working in a local checkout must report the
/// branch that checkout is on.
#[test]
fn dashboard_preview_reports_the_branch_of_a_local_work_tree() {
    let repository = temp_git_repo("feature/agent-cards");
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    agent.work_dir = repository.path().to_path_buf();
    let state = state_with_selected_agent(agent);

    assert_eq!(
        preview_metadata_value(&state, "Branch"),
        "feature/agent-cards",
        "the preview must probe the branch the way the pre-cutover dashboard did"
    );
}

/// The `(unknown)` sentinel is still the answer when there is nothing to
/// probe: restoring the probe must not invent a branch for a work dir that is
/// not a git tree.
#[test]
fn dashboard_preview_keeps_the_unknown_sentinel_off_a_git_tree() {
    let directory = tempfile::tempdir().value_or_panic("create a plain work dir");
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    agent.work_dir = directory.path().to_path_buf();
    let state = state_with_selected_agent(agent);

    assert_eq!(
        preview_metadata_value(&state, "Branch"),
        "(unknown)",
        "a work dir with no git tree has no branch to report"
    );
}

/// Remote repositories skip branch and dirty probing, which would need an SSH
/// round trip. The restored probe must honour that contract rather than
/// reaching for the local path a remote agent does not own.
#[test]
fn dashboard_preview_skips_the_branch_probe_for_a_remote_repository() {
    let repository = temp_git_repo("feature/agent-cards");
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    agent.work_dir = repository.path().to_path_buf();
    let mut state = state_with_selected_agent(agent);
    state.repositories[0].remote.enabled = true;

    assert_eq!(
        preview_metadata_value(&state, "Branch"),
        "(unknown)",
        "a remote repository is not probed over SSH just to fill a row"
    );
}

/// Issue #733 defect 2: `build_preview_view_at` computes a turn-elapsed row,
/// a `Todo:` block and the last reply, and the projection consumed only
/// `preview_metadata`, so none of it reached the panel. Six required scenarios
/// wait on the reply text.
#[test]
fn dashboard_preview_carries_the_todo_block_turn_elapsed_and_last_reply() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    let mut state = state_with_selected_agent(agent.clone());
    state
        .observations
        .insert(agent.id.clone(), preview_observation());

    let rows = projected_preview_rows(&state, 80);

    for expected in [
        "Turn elapsed: 5s",
        "Todo:",
        "  [>] Implement issue 522",
        "  [ ] Ship the preview",
        "Last reply: JSP preview is wired",
    ] {
        assert!(
            rows.iter().any(|row| row == expected),
            "the preview must carry `{expected}`, got {rows:?}"
        );
    }
}

/// Without an observation the block still announces itself: the retained
/// projection's no-telemetry arm reaches the panel too, so an agent with no
/// JSP producer reads as unavailable rather than as having no tasks.
#[test]
fn dashboard_preview_says_telemetry_is_unavailable_without_an_observation() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    let state = state_with_selected_agent(agent);

    let rows = projected_preview_rows(&state, 80);

    assert!(
        rows.iter().any(|row| row == "Todo:"),
        "the todo header is unconditional, got {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row == "  (telemetry unavailable)"),
        "the no-observation arm must reach the panel, got {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.starts_with("Turn elapsed:")),
        "there is no turn to time, got {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.starts_with("Last reply:")),
        "there is no reply to show, got {rows:?}"
    );
}

/// A reply the pane cannot fit ends in an ellipsis on one row rather than
/// wrapping onto a second.
///
/// The pre-cutover pane budgeted every Preview row this way, and the corpus
/// reads `Last reply:` as one row: `workbench-cards-native` and
/// `jsp-llxprt-preview-native` wait on a 19-character prefix of the reply,
/// which a wrap splits in half.
#[test]
fn dashboard_preview_truncates_a_reply_the_pane_cannot_fit() {
    use crate::domain::observation::{
        BoundedText, DisplayedAssistantMessage, FieldState, Provenance,
    };
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    let mut observation = preview_observation();
    observation.last_message = FieldState::known(
        Provenance::Authoritative,
        DisplayedAssistantMessage {
            content: BoundedText("Native LLxprt JSP reply".to_owned()),
            committed_ms: 0,
        },
    );
    let mut state = state_with_selected_agent(agent.clone());
    state.observations.insert(agent.id.clone(), observation);

    let rows = projected_preview_rows(&state, 80);

    assert!(
        rows.iter()
            .any(|row| row == "Last reply: Native LLxprt JSP r\u{2026}"),
        "an over-wide reply is truncated to one 32-cell row, got {rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row == "reply"),
        "the reply must not wrap onto a row of its own, got {rows:?}"
    );
}

/// The leading description line #723/#725 added is an explicit non-goal of
/// #733: restoring the document rows must not displace it.
#[test]
fn dashboard_preview_keeps_the_description_as_its_leading_row() {
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    agent.description = "Restores the preview".to_owned();
    let mut state = state_with_selected_agent(agent.clone());
    state
        .observations
        .insert(agent.id.clone(), preview_observation());

    let rows = projected_preview_rows(&state, 80);

    assert_eq!(
        rows.first().map(String::as_str),
        Some("Restores the preview"),
        "the description stays the panel's first row, got {rows:?}"
    );
}

/// Issue #733 guard: every row the retained `preview_view` projection produces
/// for the selected agent must reach the shipped panel.
///
/// `build_preview_view_at` lost its last live caller once already, in the #715
/// cutover, and the pane went quietly blank for three merges. If the
/// projection stops consuming the retained module, this fails instead.
///
/// The fixture's repository has no configured origin and its work dir is not a
/// git tree, so the git-derived rows read `(unknown)` whichever resolver the
/// projection uses; the comparison is therefore about the rows themselves.
/// Every row also stays inside the pane's budgets, so the row bytes are
/// comparable without restating the #723 truncation contract here.
#[test]
fn every_retained_preview_view_row_reaches_the_shipped_projection() {
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    agent.work_dir = std::path::PathBuf::from("/tmp/jefe/zed");
    let observation = preview_observation();
    let mut state = state_with_selected_agent(agent.clone());
    state
        .observations
        .insert(agent.id.clone(), observation.clone());

    let retained = crate::preview_view::build_preview_view_at(
        Some(&agent),
        None,
        Some(&observation),
        usize::MAX,
        std::time::Instant::now(),
    );
    // A guard that can pass on an empty expectation guards nothing.
    assert!(
        retained.lines.iter().any(|line| line == "Todo:")
            && retained
                .lines
                .iter()
                .any(|line| line.starts_with("Last reply:")),
        "the fixture must exercise the rows this guard exists for: {:?}",
        retained.lines
    );

    let rows = projected_preview_rows(&state, 80);
    for line in retained.lines.iter().filter(|line| !line.is_empty()) {
        assert!(
            rows.iter().any(|row| row == line),
            "preview_view row `{line}` never reached the shipped projection: {rows:?}"
        );
    }
}
