use std::sync::Arc;

mod issues;
mod issues_dispatch;
mod issues_filter;
mod issues_mutation;
mod modal_handlers;
mod normal;
mod preflight;

pub use modal_handlers::{handle_f12_toggle, handle_mode_confirm_key, handle_mode_form_key};

pub use normal::{handle_global_shortcut_key, handle_normal_key_event};

use iocraft::hooks::State as HookState;
use iocraft::prelude::*;
use tracing::{debug, warn};

use std::time::Duration;

use jefe::domain::{AgentId, AgentStatus, LaunchSignature, Repository};

const MAC_ALT_DIGIT_SHORTCUTS: &[(char, u8)] = &[
    ('¡', 1),
    ('™', 2),
    ('£', 3),
    ('¢', 4),
    ('∞', 5),
    ('§', 6),
    ('¶', 7),
    ('•', 8),
    ('ª', 9),
];
use jefe::input::{SearchKeyRoute, route_search_key};
use jefe::messages::{AppMessage, IssuesMessage, RuntimeMessage, UiNavigationMessage};
use jefe::persistence::{PersistenceManager, State as PersistedState};
const REMOTE_ATTACH_SETTLE_DELAY: Duration = Duration::from_millis(150);

use jefe::runtime::{RuntimeError, RuntimeManager, sandbox_preflight, sandbox_ssh_agent_warning};

#[must_use]
fn jump_to_shortcut_agent(app_state: &mut AppStateHandle, ctx: &SharedContext, slot: u8) -> bool {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::JumpToAgentByShortcut(slot));

    let selected_running_agent_id = state
        .selected_agent()
        .filter(|agent| agent.is_running())
        .map(|agent| agent.id.clone());

    if let Some(agent_id) = selected_running_agent_id {
        state.pane_focus = PaneFocus::Terminal;
        if !state.terminal_focused {
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
        }
        drop(state);

        let attached_ok = if let Some(ctx_arc) = ctx
            && let Ok(mut ctx_guard) = ctx_arc.lock()
        {
            ctx_guard.runtime.attach(&agent_id).is_ok()
        } else {
            false
        };

        let mut state = app_state.write();
        if !attached_ok {
            state.terminal_focused = false;
            state.pane_focus = PaneFocus::Agents;
            mark_agent_runtime_attached(&mut state, &agent_id, false);
            let persisted = to_persisted_state(&state);
            drop(state);
            persist_state(ctx, &persisted);
            return false;
        }

        clear_agent_runtime_attachment(&mut state);
        mark_agent_runtime_attached(&mut state, &agent_id, true);
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        true
    } else {
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        false
    }
}

use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, RepositoryFormFocus};

fn repository_focus_toggles_checkbox(focus: RepositoryFormFocus) -> bool {
    matches!(
        focus,
        RepositoryFormFocus::RemoteEnabled | RepositoryFormFocus::SetupEnvDefault
    )
}

pub type SharedContext = Option<Arc<std::sync::Mutex<super::AppContext>>>;
pub type AppStateHandle = HookState<AppState>;
pub type QuitHandle = HookState<bool>;
pub type HelpScrollHandle = HookState<u32>;

pub fn to_persisted_state(state: &AppState) -> PersistedState {
    PersistedState {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: state.repositories.clone(),
        agents: state.agents.clone(),
        selected_repository_index: state.selected_repository_index,
        selected_agent_index: state.selected_agent_index,
        hide_idle_repositories: state.hide_idle_repositories,
        last_selected_agent_by_repo: state.last_selected_agent_by_repo.clone(),
    }
}

pub fn persist_state(ctx: &SharedContext, persisted: &PersistedState) {
    if let Some(ctx_arc) = &ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.persistence.save_state(persisted)
    {
        warn!(error = %e, "could not save state");
    }
}

fn clear_runtime_warning(state: &mut AppState) {
    if state.warning_message.as_deref().is_some_and(|warning| {
        warning.contains("SSH_AUTH_SOCK") || warning.contains("SSH agent socket")
    }) {
        state.warning_message = None;
    }
}

fn launch_signature_for_agent(
    agent: &jefe::domain::Agent,
    repository: &Repository,
) -> LaunchSignature {
    LaunchSignature {
        work_dir: agent.work_dir.clone(),
        profile: agent.profile.clone(),
        mode_flags: agent.mode_flags.clone(),
        llxprt_debug: agent.llxprt_debug.clone(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: agent.sandbox_enabled,
        sandbox_engine: agent.sandbox_engine,
        sandbox_flags: agent.sandbox_flags.clone(),
        remote: repository.remote.clone(),
    }
}

fn agent_and_signature(
    state: &AppState,
    agent_id: &AgentId,
) -> Option<(jefe::domain::Agent, LaunchSignature)> {
    let agent = state
        .agents
        .iter()
        .find(|agent| &agent.id == agent_id)?
        .clone();
    let repository = state.repository_by_id(&agent.repository_id)?;
    let signature = launch_signature_for_agent(&agent, repository);
    Some((agent, signature))
}

fn set_agent_runtime_binding(
    state: &mut AppState,
    agent_id: &AgentId,
    session_name: String,
    signature: LaunchSignature,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name,
            launch_signature: signature,
            attached: false,
            last_seen: None,
        });
    }
}

fn mark_agent_runtime_attached(state: &mut AppState, agent_id: &AgentId, attached: bool) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id)
        && let Some(binding) = agent.runtime_binding.as_mut()
    {
        binding.attached = attached;
    }
}

fn clear_agent_runtime_attachment(state: &mut AppState) {
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}

fn mark_runtime_session_dead_if_present(state: &mut AppState, agent_id: &AgentId) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.status = AgentStatus::Dead;
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}

fn apply_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(evt);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn close_modal_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::CloseModal);
}
/// Run sandbox preflight checks and either show a prompt or proceed with launch.
///
/// Returns `true` if the launch can proceed immediately (no issues or sandbox
/// not enabled).  Returns `false` if a `PreflightPrompt` modal was opened and
/// the caller should abort the immediate launch path.
fn preflight_or_prompt(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &LaunchSignature,
) -> bool {
    if !signature.sandbox_enabled {
        return true;
    }

    if let Some(issue) = sandbox_preflight(signature.sandbox_engine) {
        let mut state = app_state.write();
        state.modal = ModalState::PreflightPrompt {
            agent_id: agent_id.clone(),
            signature: signature.clone(),
            issue,
            remaining_issues: Vec::new(),
        };
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        return false;
    }

    true
}

/// Actually spawn + attach an agent session (shared by fresh-launch and
/// post-preflight resume paths).
fn execute_agent_launch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    is_relaunch: bool,
) {
    let attach_result = spawn_and_attach(ctx, agent_id, work_dir, signature, is_relaunch);

    if let Err(e) = attach_result {
        warn!(error = %e, "could not spawn or attach session for agent");
        mark_launch_failed(app_state, ctx, agent_id, e);
    } else {
        mark_launch_attached(app_state, ctx, agent_id, signature);
    }
}

fn spawn_and_attach(
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
    is_relaunch: bool,
) -> Result<(), RuntimeError> {
    let Some(ctx_arc) = ctx else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context unavailable".to_owned(),
        ));
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return Err(RuntimeError::SpawnFailed(
            "runtime context lock unavailable".to_owned(),
        ));
    };

    let spawn_result = if is_relaunch {
        ctx_guard
            .runtime
            .spawn_session_fresh(agent_id, work_dir, signature)
    } else {
        ctx_guard
            .runtime
            .spawn_session(agent_id, work_dir, signature)
    };
    spawn_result.and_then(|()| {
        std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
        ctx_guard.runtime.attach(agent_id)
    })
}

fn mark_launch_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    error: RuntimeError,
) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let _ = ctx_guard.runtime.mark_session_dead(agent_id);
    }

    let mut state = app_state.write();
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    state.error_message = Some(error.to_string());
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
        agent.runtime_binding = None;
    }
    mark_runtime_session_dead_if_present(&mut state, agent_id);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn mark_launch_attached(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &LaunchSignature,
) {
    let mut state = app_state.write();
    set_agent_runtime_binding(
        &mut state,
        agent_id,
        jefe::runtime::RuntimeSession::session_name_for(agent_id),
        signature.clone(),
    );
    clear_agent_runtime_attachment(&mut state);
    mark_agent_runtime_attached(&mut state, agent_id, true);
    if let Some(warning) = sandbox_ssh_agent_warning() {
        state.warning_message = Some(warning);
    } else {
        clear_runtime_warning(&mut state);
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

pub fn handle_mode_help_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    help_scroll: &mut HelpScrollHandle,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('?') => {
            close_modal_and_persist(app_state, ctx);
        }
        KeyCode::Up => {
            let offset = help_scroll.get();
            if offset > 0 {
                help_scroll.set(offset - 1);
            }
        }
        KeyCode::Down => {
            help_scroll.set(help_scroll.get() + 1);
        }
        _ => {}
    }
}

pub fn handle_mode_search_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    match route_search_key(key_event) {
        SearchKeyRoute::CloseAndConsume => {
            close_modal_and_persist(app_state, ctx);
            true
        }
        SearchKeyRoute::Backspace => {
            apply_and_persist(app_state, ctx, AppEvent::FormBackspace);
            true
        }
        SearchKeyRoute::EditQueryChar(c) => {
            apply_and_persist(app_state, ctx, AppEvent::FormChar(c));
            true
        }
        SearchKeyRoute::CloseAndReroute => {
            debug!(
                code = ?key_event.code,
                modifiers = ?key_event.modifiers,
                "closing search mode on non-search key"
            );
            close_modal_and_persist(app_state, ctx);
            false
        }
        SearchKeyRoute::Ignore => true,
    }
}

pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    dispatch_app_message(app_state, ctx, evt.into());
}

pub fn dispatch_app_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: AppMessage,
) {
    log_dispatch(&message);

    match message {
        AppMessage::UiNavigation(UiNavigationMessage::ToggleTerminalFocus) => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleTerminalFocus);
        }
        AppMessage::Runtime(RuntimeMessage::KillAgent(agent_id)) => {
            dispatch_kill_agent(app_state, ctx, agent_id);
        }
        AppMessage::Runtime(RuntimeMessage::RelaunchAgent(agent_id)) => {
            dispatch_relaunch_agent(app_state, ctx, agent_id);
        }
        AppMessage::Issues(message @ (IssuesMessage::NavigateUp | IssuesMessage::NavigateDown)) => {
            dispatch_issues_navigation(app_state, ctx, message);
        }
        AppMessage::Issues(
            message @ (IssuesMessage::EnterMode
            | IssuesMessage::RefocusList
            | IssuesMessage::ApplyFilter
            | IssuesMessage::ClearFilter
            | IssuesMessage::ApplySearch),
        ) => dispatch_issue_list_reload(app_state, ctx, message),
        AppMessage::Issues(IssuesMessage::Enter) => {
            apply_and_persist(app_state, ctx, AppEvent::IssuesEnter);
            issues_dispatch::load_issue_detail_for_selection(app_state, ctx);
        }
        AppMessage::Issues(IssuesMessage::AgentChooserConfirm) => {
            dispatch_agent_chooser_confirm(app_state, ctx);
        }
        AppMessage::Issues(IssuesMessage::InlineSubmit) => {
            issues_mutation::handle_inline_submit(app_state, ctx);
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

fn log_dispatch(message: &AppMessage) {
    let route = message.route();
    debug!(
        message_domain = ?route.domain,
        message = route.name,
        "dispatching app message"
    );
}

fn dispatch_kill_agent(app_state: &mut AppStateHandle, ctx: &SharedContext, agent_id: AgentId) {
    if let Err(error) = kill_runtime_agent(ctx, &agent_id) {
        warn!(agent_id = %agent_id.0, error = %error, "could not kill runtime session");
        persist_error_message(app_state, ctx, error);
        return;
    }

    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::KillAgent(agent_id));
    state.terminal_focused = false;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn kill_runtime_agent(ctx: &SharedContext, agent_id: &AgentId) -> Result<(), String> {
    let Some(ctx_arc) = ctx else {
        return Ok(());
    };
    match ctx_arc.lock() {
        Ok(mut ctx_guard) => ctx_guard.runtime.kill(agent_id).map_err(|e| e.to_string()),
        Err(error) => Err(format!("application context lock poisoned: {error}")),
    }
}

fn persist_error_message(app_state: &mut AppStateHandle, ctx: &SharedContext, error: String) {
    let mut state = app_state.write();
    state.error_message = Some(error);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn dispatch_relaunch_agent(app_state: &mut AppStateHandle, ctx: &SharedContext, agent_id: AgentId) {
    if !relaunch_preflight_passed(app_state, ctx, &agent_id) {
        return;
    }

    let relaunched = relaunch_runtime_session(app_state, ctx, &agent_id);
    persist_relaunch_result(app_state, ctx, agent_id, relaunched);
}

fn relaunch_preflight_passed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) -> bool {
    let state_ro = app_state.read();
    let signature = agent_and_signature(&state_ro, agent_id).map(|(_, signature)| signature);
    drop(state_ro);
    match signature {
        Some(signature) => preflight_or_prompt(app_state, ctx, agent_id, &signature),
        None => true,
    }
}

fn relaunch_runtime_session(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };

    let state_ro = app_state.read();
    let Some((agent, signature)) = agent_and_signature(&state_ro, agent_id) else {
        return false;
    };
    drop(state_ro);

    if !spawn_relaunch_session(
        &mut ctx_guard.runtime,
        agent_id,
        &agent.work_dir,
        &signature,
    ) {
        return false;
    }
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    attach_relaunched_session(&mut ctx_guard.runtime, agent_id)
}

fn spawn_relaunch_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    signature: &LaunchSignature,
) -> bool {
    match runtime.spawn_session_fresh(agent_id, work_dir, signature) {
        Ok(()) => true,
        Err(RuntimeError::AlreadyRunning(_)) => runtime.relaunch(agent_id).is_ok(),
        Err(error) => {
            warn!(
                agent_id = %agent_id.0,
                error = %error,
                "could not spawn fresh runtime session for relaunch"
            );
            false
        }
    }
}

fn attach_relaunched_session(
    runtime: &mut jefe::runtime::TmuxRuntimeManager,
    agent_id: &AgentId,
) -> bool {
    match runtime.attach(agent_id) {
        Ok(()) => true,
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not attach relaunched session");
            let _ = runtime.mark_session_dead(agent_id);
            false
        }
    }
}

fn persist_relaunch_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    relaunched: bool,
) {
    let relaunch_event = AppEvent::RelaunchAgent(agent_id.clone());
    let mut state = app_state.write();
    if relaunched {
        persist_relaunch_success(&mut state, &agent_id, relaunch_event);
    } else {
        persist_relaunch_failure(&mut state, &agent_id, relaunch_event);
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn persist_relaunch_success(state: &mut AppState, agent_id: &AgentId, relaunch_event: AppEvent) {
    if let Some((agent, signature)) = agent_and_signature(state, agent_id) {
        set_agent_runtime_binding(
            state,
            agent_id,
            jefe::runtime::RuntimeSession::session_name_for(&agent.id),
            signature,
        );
    }
    *state = std::mem::take(state).apply(relaunch_event);
    state.terminal_focused = false;
    clear_agent_runtime_attachment(state);
    mark_agent_runtime_attached(state, agent_id, true);
    if let Some(warning) = sandbox_ssh_agent_warning() {
        state.warning_message = Some(warning);
    } else {
        clear_runtime_warning(state);
    }
}

fn persist_relaunch_failure(state: &mut AppState, agent_id: &AgentId, relaunch_event: AppEvent) {
    *state = std::mem::take(state).apply(relaunch_event);
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    mark_runtime_session_dead_if_present(state, agent_id);
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.runtime_binding = None;
    }
}

fn dispatch_issues_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    let (focus, prev_repo_idx, prev_issue_idx) = {
        let state = app_state.read();
        (
            state.issues_state.issue_focus,
            state.selected_repository_index,
            state.issues_state.selected_issue_index,
        )
    };

    apply_and_persist(app_state, ctx, AppEvent::from(message));
    refresh_issue_navigation(app_state, ctx, focus, prev_repo_idx, prev_issue_idx);
}

fn refresh_issue_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    focus: jefe::state::IssueFocus,
    prev_repo_idx: Option<usize>,
    prev_issue_idx: Option<usize>,
) {
    match focus {
        jefe::state::IssueFocus::RepoList => {
            refresh_repo_scope_if_changed(app_state, ctx, prev_repo_idx);
        }
        jefe::state::IssueFocus::IssueList => {
            refresh_issue_preview_if_changed(app_state, prev_issue_idx);
        }
        jefe::state::IssueFocus::IssueDetail => {}
    }
}

fn refresh_repo_scope_if_changed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prev_repo_idx: Option<usize>,
) {
    let new_repo_idx = app_state.read().selected_repository_index;
    if new_repo_idx == prev_repo_idx {
        return;
    }
    reset_issue_list_for_repo_change(app_state);
    dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
    app_state.write().issues_state.issue_focus = jefe::state::IssueFocus::RepoList;
}

fn reset_issue_list_for_repo_change(app_state: &mut AppStateHandle) {
    let mut state = app_state.write();
    state.issues_state.issues.clear();
    state.issues_state.selected_issue_index = None;
    state.issues_state.issue_detail = None;
    state.issues_state.list_cursor = None;
    state.issues_state.has_more_issues = false;
    state.issues_state.error = None;
    if state.issues_state.inline_state != jefe::state::InlineState::None {
        state.issues_state.draft_notice = Some("Unsent draft discarded".to_string());
    }
    state.issues_state.inline_state = jefe::state::InlineState::None;
    state.issues_state.agent_chooser = None;
    state.issues_state.list_loading = true;
}

fn refresh_issue_preview_if_changed(app_state: &mut AppStateHandle, prev_issue_idx: Option<usize>) {
    let new_issue_idx = app_state.read().issues_state.selected_issue_index;
    if new_issue_idx != prev_issue_idx {
        issues_dispatch::preview_issue_from_list(app_state);
    }
}

fn dispatch_issue_list_reload(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    let fresh_reload = matches!(
        message,
        IssuesMessage::RefocusList | IssuesMessage::ApplySearch
    );
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    let params = issue_fetch_params(app_state, fresh_reload);

    if params.owner.is_empty() || params.repo.is_empty() {
        persist_missing_github_repo(app_state, ctx);
        return;
    }

    let result = fetch_issue_list(ctx, &params);
    persist_issue_list_result(app_state, &params.scope_repo_id, result);
}

struct IssueFetchParams {
    scope_repo_id: jefe::domain::RepositoryId,
    owner: String,
    repo: String,
    filter: jefe::domain::IssueFilter,
    cursor: Option<String>,
    page_size: u32,
}

fn issue_fetch_params(app_state: &AppStateHandle, fresh_reload: bool) -> IssueFetchParams {
    let state = app_state.read();
    let gh_repo = issues_dispatch::resolve_gh_repo(&state);
    IssueFetchParams {
        scope_repo_id: issues_dispatch::current_scope_repo_id(&state),
        owner: gh_repo.0,
        repo: gh_repo.1,
        filter: state.issues_state.committed_filter.clone(),
        cursor: (!fresh_reload)
            .then(|| state.issues_state.list_cursor.clone())
            .flatten(),
        page_size: 30,
    }
}

fn persist_missing_github_repo(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.issues_state.list_loading = false;
    state.issues_state.error = Some(
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings."
            .to_string(),
    );
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn fetch_issue_list(
    ctx: &SharedContext,
    params: &IssueFetchParams,
) -> Option<Result<jefe::github::IssueListResponse, jefe::github::GhError>> {
    if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
    {
        Some(ctx_guard.gh_client.list_issues(
            &params.owner,
            &params.repo,
            &params.filter,
            params.cursor.as_deref(),
            params.page_size,
        ))
    } else {
        None
    }
}

fn persist_issue_list_result(
    app_state: &mut AppStateHandle,
    scope_repo_id: &jefe::domain::RepositoryId,
    result: Option<Result<jefe::github::IssueListResponse, jefe::github::GhError>>,
) {
    match result {
        Some(Ok(response)) => persist_issue_list_loaded(app_state, scope_repo_id, response),
        Some(Err(error)) => persist_issue_list_failed(app_state, scope_repo_id, error.to_string()),
        None => persist_issue_list_failed(
            app_state,
            scope_repo_id,
            "Application context unavailable".to_string(),
        ),
    }
}

fn persist_issue_list_loaded(
    app_state: &mut AppStateHandle,
    scope_repo_id: &jefe::domain::RepositoryId,
    response: jefe::github::IssueListResponse,
) {
    let has_issues = !response.issues.is_empty();
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::IssueListLoaded {
        scope_repo_id: scope_repo_id.clone(),
        issues: response.issues,
        cursor: response.cursor,
        has_more: response.has_more,
    });
    drop(state);
    if has_issues {
        issues_dispatch::preview_issue_from_list(app_state);
    }
}

fn persist_issue_list_failed(
    app_state: &mut AppStateHandle,
    scope_repo_id: &jefe::domain::RepositoryId,
    error: String,
) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::IssueListLoadFailed {
        scope_repo_id: scope_repo_id.clone(),
        error,
    });
}

fn dispatch_agent_chooser_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let send_info = issue_send_info(app_state);
    apply_and_persist(app_state, ctx, AppEvent::AgentChooserConfirm);

    let Some(send_info) = send_info else {
        return;
    };
    if let Err(error) = write_issue_prompt(&send_info.work_dir, &send_info.payload) {
        apply_send_to_agent_failed(app_state, error);
        return;
    }

    let mut launch_sig = send_info.signature;
    launch_sig.mode_flags.push("-i".to_owned());
    launch_sig
        .mode_flags
        .push("Read and work on the GitHub issue described in .jefe/issue-prompt.md".to_owned());
    if preflight_or_prompt(app_state, ctx, &send_info.agent_id, &launch_sig) {
        launch_issue_agent(
            app_state,
            ctx,
            send_info.agent_id,
            send_info.work_dir,
            launch_sig,
        );
    }
}

struct IssueSendInfo {
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    signature: LaunchSignature,
    payload: jefe::github::SendPayload,
}

fn issue_send_info(app_state: &AppStateHandle) -> Option<IssueSendInfo> {
    let state = app_state.read();
    let chooser = state.issues_state.agent_chooser.as_ref()?;
    let detail = state.issues_state.issue_detail.as_ref()?;
    let (agent_id, _) = chooser.agents.get(chooser.selected_index)?.clone();
    let agent = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)?
        .clone();
    let repo = state.repository_by_id(&agent.repository_id)?;
    let focused_comment = focused_issue_comment(&state, detail);
    let work_dir = agent.work_dir.clone();
    let signature = launch_signature_for_agent(&agent, repo);
    let payload = jefe::github::GhClient::build_send_payload(
        &repo.slug,
        detail,
        focused_comment.as_ref(),
        &repo.issue_base_prompt,
    );
    drop(state);

    Some(IssueSendInfo {
        agent_id,
        work_dir,
        signature,
        payload,
    })
}

fn focused_issue_comment(
    state: &AppState,
    detail: &jefe::domain::IssueDetail,
) -> Option<jefe::domain::IssueComment> {
    match state.issues_state.detail_subfocus {
        jefe::state::DetailSubfocus::Comment(idx) => detail.comments.get(idx).cloned(),
        _ => None,
    }
}

fn write_issue_prompt(
    work_dir: &std::path::Path,
    payload: &jefe::github::SendPayload,
) -> Result<(), String> {
    let prompt_dir = work_dir.join(".jefe");
    std::fs::create_dir_all(&prompt_dir)
        .map_err(|error| format!("Failed to create .jefe dir: {error}"))?;
    let prompt_path = prompt_dir.join("issue-prompt.md");
    let prompt_content = issues_dispatch::format_issue_prompt(payload);
    std::fs::write(&prompt_path, &prompt_content)
        .map_err(|error| format!("Failed to write issue prompt: {error}"))
}

fn launch_issue_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    launch_sig: LaunchSignature,
) {
    let launched = spawn_and_attach_fresh_for_issue(ctx, &agent_id, &work_dir, &launch_sig);
    let mut state = app_state.write();
    if launched {
        persist_issue_agent_launch_success(&mut state, &agent_id, launch_sig);
    } else {
        *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
            error: "Failed to launch agent".to_string(),
        });
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn spawn_and_attach_fresh_for_issue(
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    launch_sig: &LaunchSignature,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };
    match ctx_guard
        .runtime
        .spawn_session_fresh(agent_id, work_dir, launch_sig)
    {
        Ok(()) => attach_issue_agent(&mut ctx_guard.runtime, agent_id),
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not spawn agent for issue send");
            false
        }
    }
}

fn attach_issue_agent(runtime: &mut jefe::runtime::TmuxRuntimeManager, agent_id: &AgentId) -> bool {
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    match runtime.attach(agent_id) {
        Ok(()) => true,
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not attach agent after issue send");
            let _ = runtime.mark_session_dead(agent_id);
            false
        }
    }
}

fn persist_issue_agent_launch_success(
    state: &mut AppState,
    agent_id: &AgentId,
    launch_sig: LaunchSignature,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.status = jefe::domain::AgentStatus::Running;
        let session_name = jefe::runtime::RuntimeSession::session_name_for(agent_id);
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name,
            launch_signature: launch_sig,
            attached: false,
            last_seen: None,
        });
    }
    clear_agent_runtime_attachment(state);
    mark_agent_runtime_attached(state, agent_id, true);
}

fn apply_send_to_agent_failed(app_state: &mut AppStateHandle, error: String) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed { error });
}

#[cfg(test)]
#[path = "app_input_tests.rs"]
mod tests;
