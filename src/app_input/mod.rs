use std::sync::Arc;

mod normal;
mod preflight;

pub use normal::{handle_global_shortcut_key, handle_normal_key_event};
use preflight::handle_preflight_prompt_enter;

use iocraft::hooks::State as HookState;
use iocraft::prelude::*;
use tracing::{debug, warn};

use std::time::Duration;

use jefe::domain::{AgentId, AgentStatus, LaunchSignature, Repository, SandboxEngine};

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
            persist_state_snapshot(ctx, &state);
            return false;
        }

        clear_agent_runtime_attachment(&mut state);
        mark_agent_runtime_attached(&mut state, &agent_id, true);
        persist_state_snapshot(ctx, &state);
        true
    } else {
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        persist_state_snapshot(ctx, &state);
        false
    }
}

use jefe::state::{AgentFormFocus, AppEvent, AppState, ModalState, PaneFocus, RepositoryFormFocus};

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

pub fn persist_state_snapshot(ctx: &SharedContext, state: &AppState) {
    if let Some(ctx_arc) = &ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.persistence.save_state(&to_persisted_state(state))
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
    persist_state_snapshot(ctx, &state);
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
        persist_state_snapshot(ctx, &state);
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
    let attach_result = if let Some(ctx_arc) = ctx {
        if let Ok(mut ctx_guard) = ctx_arc.lock() {
            let spawn_result = if is_relaunch {
                ctx_guard
                    .runtime
                    .spawn_session_fresh(agent_id, work_dir, signature)
            } else {
                ctx_guard
                    .runtime
                    .spawn_session(agent_id, work_dir, signature)
            };
            match spawn_result {
                Ok(()) => {
                    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
                    ctx_guard.runtime.attach(agent_id)
                }
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        }
    } else {
        Ok(())
    };

    if let Err(e) = attach_result {
        warn!(error = %e, "could not spawn or attach session for agent");
        let mut state = app_state.write();
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        state.error_message = Some(e.to_string());
        if let Some(ctx_arc) = ctx
            && let Ok(mut ctx_guard) = ctx_arc.lock()
        {
            let _ = ctx_guard.runtime.mark_session_dead(agent_id);
        }
        if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
            agent.runtime_binding = None;
        }
        mark_runtime_session_dead_if_present(&mut state, agent_id);
        persist_state_snapshot(ctx, &state);
    } else {
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
        persist_state_snapshot(ctx, &state);
    }
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

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    debug!(event = ?evt, "dispatching app event");

    match evt {
        AppEvent::ToggleTerminalFocus => {
            // Keep Enter-in-terminal-pane as a UI focus toggle only.
            // Runtime attach/detach remains bound to F12.
            apply_and_persist(app_state, ctx, AppEvent::ToggleTerminalFocus);
        }
        AppEvent::KillAgent(ref agent_id) => {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
                && let Err(e) = ctx_guard.runtime.kill(agent_id)
            {
                warn!(agent_id = %agent_id.0, error = %e, "could not kill runtime session");
            }

            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(evt);
            state.terminal_focused = false;
            persist_state_snapshot(ctx, &state);
        }
        AppEvent::RelaunchAgent(agent_id) => {
            // Run preflight before attempting the relaunch.
            {
                let state_ro = app_state.read();
                if let Some((_agent, signature)) = agent_and_signature(&state_ro, &agent_id) {
                    drop(state_ro);
                    if !preflight_or_prompt(app_state, ctx, &agent_id, &signature) {
                        return;
                    }
                }
            }

            let mut relaunched = false;
            let relaunch_event = AppEvent::RelaunchAgent(agent_id.clone());
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                // Always relaunch from current in-memory agent config so edits made
                // before relaunch (e.g. LLXPRT_DEBUG changes) are applied.
                let state_ro = app_state.read();
                if let Some((agent, signature)) = agent_and_signature(&state_ro, &agent_id) {
                    match ctx_guard.runtime.spawn_session_fresh(
                        &agent_id,
                        &agent.work_dir,
                        &signature,
                    ) {
                        Ok(()) => {
                            relaunched = true;
                        }
                        Err(e) => {
                            // If the process-local mapping still exists, fall back to runtime relaunch.
                            // This keeps behavior stable for edge cases while still preferring fresh config.
                            match e {
                                RuntimeError::AlreadyRunning(_) => {
                                    match ctx_guard.runtime.relaunch(&agent_id) {
                                        Ok(()) => {
                                            relaunched = true;
                                        }
                                        Err(e2) => {
                                            warn!(
                                                agent_id = %agent_id.0,
                                                error = %e2,
                                                "could not relaunch runtime session"
                                            );
                                        }
                                    }
                                }
                                _ => {
                                    warn!(
                                        agent_id = %agent_id.0,
                                        error = %e,
                                        "could not spawn fresh runtime session for relaunch"
                                    );
                                }
                            }
                        }
                    }
                }

                if relaunched {
                    // Relaunch should make output visible immediately; focus remains separate.
                    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
                    match ctx_guard.runtime.attach(&agent_id) {
                        Ok(()) => {}
                        Err(e) => {
                            warn!(
                                agent_id = %agent_id.0,
                                error = %e,
                                "could not attach relaunched session"
                            );
                            let _ = ctx_guard.runtime.mark_session_dead(&agent_id);
                            relaunched = false;
                        }
                    }
                }
            }

            let mut state = app_state.write();
            if relaunched {
                if let Some((agent, signature)) = agent_and_signature(&state, &agent_id) {
                    set_agent_runtime_binding(
                        &mut state,
                        &agent_id,
                        jefe::runtime::RuntimeSession::session_name_for(&agent.id),
                        signature,
                    );
                }
                *state = std::mem::take(&mut *state).apply(relaunch_event);
                state.terminal_focused = false;
                clear_agent_runtime_attachment(&mut state);
                mark_agent_runtime_attached(&mut state, &agent_id, true);
                if let Some(warning) = sandbox_ssh_agent_warning() {
                    state.warning_message = Some(warning);
                } else {
                    clear_runtime_warning(&mut state);
                }
            } else {
                *state = std::mem::take(&mut *state).apply(relaunch_event);
                state.terminal_focused = false;
                state.pane_focus = PaneFocus::Agents;
                mark_runtime_session_dead_if_present(&mut state, &agent_id);
                if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == agent_id) {
                    agent.runtime_binding = None;
                }
            }
            persist_state_snapshot(ctx, &state);
        }

        _ => {
            apply_and_persist(app_state, ctx, evt);
        }
    }
}

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // F12 toggles terminal input focus.
    // When enabling, force pane focus to terminal and require attach success.
    let (enabling_focus, selected_agent_id) = {
        let mut state = app_state.write();

        if state.terminal_focused {
            // Leaving terminal capture should always return keyboard focus to agents.
            state.pane_focus = PaneFocus::Agents;
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
            (false, None)
        } else {
            let selected_running_agent_id = state
                .selected_agent()
                .filter(|agent| agent.is_running())
                .map(|agent| agent.id.clone());

            if selected_running_agent_id.is_some() {
                state.pane_focus = PaneFocus::Terminal;
                *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                (true, selected_running_agent_id)
            } else {
                // Dead/non-running agents are not attachable.
                state.pane_focus = PaneFocus::Agents;
                state.terminal_focused = false;
                (false, None)
            }
        }
    };

    if enabling_focus {
        let attached = selected_agent_id.as_ref().is_some_and(|agent_id| {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                match ctx_guard.runtime.attach(agent_id) {
                    Ok(()) => true,
                    Err(e) => {
                        warn!(
                            agent_id = %agent_id.0,
                            error = %e,
                            "could not attach session on F12 focus"
                        );
                        false
                    }
                }
            } else {
                false
            }
        });

        let mut state = app_state.write();
        if !attached {
            state.terminal_focused = false;
            state.pane_focus = PaneFocus::Agents;
            if let Some(agent_id) = selected_agent_id.as_ref() {
                mark_agent_runtime_attached(&mut state, agent_id, false);
            }
        } else if let Some(agent_id) = selected_agent_id.as_ref() {
            clear_agent_runtime_attachment(&mut state);
            mark_agent_runtime_attached(&mut state, agent_id, true);
        }
    }

    let state = app_state.read();
    persist_state_snapshot(ctx, &state);
}

#[allow(clippy::too_many_lines)]
pub fn handle_mode_confirm_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Esc => {
            close_modal_and_persist(app_state, ctx);
        }
        KeyCode::Enter => {
            let modal_snapshot = {
                let state = app_state.read();
                state.modal.clone()
            };

            match modal_snapshot {
                ModalState::ConfirmDeleteAgent {
                    id,
                    delete_work_dir,
                } => {
                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                        && let Err(e) = ctx_guard.runtime.kill(&id)
                    {
                        match e {
                            RuntimeError::SessionNotFound(_) => {}
                            _ => {
                                warn!(
                                    agent_id = %id.0,
                                    error = %e,
                                    "could not kill runtime session before delete"
                                );
                            }
                        }
                    }

                    let mut state = app_state.write();
                    let _ = super::delete_selected_agent(&mut state, &id, delete_work_dir);
                    state.modal = ModalState::None;
                    persist_state_snapshot(ctx, &state);
                }
                ModalState::ConfirmDeleteRepository { id } => {
                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                    {
                        let agent_ids: Vec<AgentId> = {
                            let state = app_state.read();
                            state
                                .agents
                                .iter()
                                .filter(|agent| agent.repository_id == id)
                                .map(|agent| agent.id.clone())
                                .collect()
                        };

                        for agent_id in &agent_ids {
                            if let Err(e) = ctx_guard.runtime.kill(agent_id) {
                                match e {
                                    RuntimeError::SessionNotFound(_) => {}
                                    _ => {
                                        warn!(
                                            agent_id = %agent_id.0,
                                            error = %e,
                                            "could not kill runtime session before repository delete"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    let mut state = app_state.write();
                    super::delete_selected_repository(&mut state, &id);
                    state.modal = ModalState::None;
                    persist_state_snapshot(ctx, &state);
                }
                ModalState::PreflightPrompt {
                    agent_id,
                    signature,
                    issue,
                    ..
                } => {
                    handle_preflight_prompt_enter(app_state, ctx, agent_id, signature, issue);
                }
                _ => {}
            }
        }
        KeyCode::Char(' ' | 'd' | 'D') | KeyCode::Up | KeyCode::Down => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleDeleteWorkDir);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
pub fn handle_mode_form_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    let app_event = match key_event.code {
        KeyCode::Esc => Some(AppEvent::CloseModal),
        KeyCode::Enter => {
            // Submit form and spawn PTY if new agent.
            let state_ro = app_state.read();
            let is_new_agent = matches!(state_ro.modal, ModalState::NewAgent { .. });
            drop(state_ro);

            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::SubmitForm);
            persist_state_snapshot(ctx, &state);

            // If new agent was created, spawn session and attach viewer.
            if is_new_agent && state.modal == ModalState::None {
                if let Some(agent) = state.selected_agent().cloned() {
                    let agent_id = agent.id.clone();
                    let work_dir = agent.work_dir.clone();
                    let repository = state.repository_by_id(&agent.repository_id).cloned();
                    let Some(repository) = repository else {
                        state.terminal_focused = false;
                        state.error_message =
                            Some("selected agent repository not found".to_owned());
                        persist_state_snapshot(ctx, &state);
                        return true;
                    };
                    let signature = launch_signature_for_agent(&agent, &repository);

                    // Drop write guard before preflight (it may take the lock).
                    drop(state);

                    // Run preflight checks before spawning.
                    if !preflight_or_prompt(app_state, ctx, &agent_id, &signature) {
                        return true;
                    }

                    // Match toy1 behavior: new agent opens attached and focused.
                    {
                        let mut state = app_state.write();
                        state.terminal_focused = true;
                        persist_state_snapshot(ctx, &state);
                    }

                    execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
                }
            }

            return true;
        }
        KeyCode::Tab | KeyCode::Down => Some(AppEvent::FormNextField),
        KeyCode::BackTab | KeyCode::Up => Some(AppEvent::FormPrevField),
        KeyCode::Left => Some(AppEvent::FormMoveCursorLeft),
        KeyCode::Right => Some(AppEvent::FormMoveCursorRight),
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        KeyCode::Delete => Some(AppEvent::FormDelete),
        // Space toggles checkbox or cycles sandbox engine on the dedicated controls.
        KeyCode::Char(' ') => {
            enum FocusedFormField {
                Repository(RepositoryFormFocus),
                Agent(AgentFormFocus),
                None,
            }

            let focused = {
                let state = app_state.read();
                match &state.modal {
                    ModalState::NewRepository { focus, .. }
                    | ModalState::EditRepository { focus, .. } => {
                        FocusedFormField::Repository(*focus)
                    }
                    ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                        FocusedFormField::Agent(*focus)
                    }
                    _ => FocusedFormField::None,
                }
            };

            match focused {
                FocusedFormField::Repository(focus) if repository_focus_toggles_checkbox(focus) => {
                    Some(AppEvent::FormToggleCheckbox)
                }
                FocusedFormField::Agent(
                    AgentFormFocus::PassContinue
                    | AgentFormFocus::Sandbox
                    | AgentFormFocus::Shortcut,
                ) => Some(AppEvent::FormToggleCheckbox),
                FocusedFormField::Agent(AgentFormFocus::SandboxEngine) => {
                    let mut state = app_state.write();
                    if let ModalState::NewAgent { fields, .. }
                    | ModalState::EditAgent { fields, .. } = &mut state.modal
                    {
                        SandboxEngine::next_from_form_value(&fields.sandbox_engine)
                            .label()
                            .clone_into(&mut fields.sandbox_engine);
                    }
                    persist_state_snapshot(ctx, &state);
                    return true;
                }
                _ => Some(AppEvent::FormChar(' ')),
            }
        }
        KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
        _ => None,
    };

    if let Some(evt) = app_event {
        apply_and_persist(app_state, ctx, evt);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use jefe::domain::{
        Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature,
        RemoteRepositorySettings, RepositoryId, RuntimeBinding, SandboxEngine,
    };

    fn sample_signature() -> LaunchSignature {
        LaunchSignature {
            work_dir: PathBuf::from("/tmp/agent"),
            profile: String::new(),
            mode_flags: vec![String::from("--yolo")],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: RemoteRepositorySettings::default(),
        }
    }

    fn sample_agent(agent_id: &AgentId) -> Agent {
        Agent::new(
            agent_id.clone(),
            RepositoryId(String::from("repo-1")),
            String::from("Agent One"),
            PathBuf::from("/tmp/agent"),
        )
    }

    #[test]
    fn repository_focus_toggles_checkbox_for_expected_fields() {
        assert!(repository_focus_toggles_checkbox(
            RepositoryFormFocus::RemoteEnabled
        ));
        assert!(repository_focus_toggles_checkbox(
            RepositoryFormFocus::SetupEnvDefault
        ));
        assert!(!repository_focus_toggles_checkbox(
            RepositoryFormFocus::Name
        ));
    }

    #[test]
    fn clear_runtime_warning_clears_only_ssh_agent_warnings() {
        let mut state = AppState {
            warning_message: Some(String::from("SSH_AUTH_SOCK is missing")),
            ..AppState::default()
        };
        clear_runtime_warning(&mut state);
        assert!(state.warning_message.is_none());

        state.warning_message = Some(String::from("regular warning"));
        clear_runtime_warning(&mut state);
        assert_eq!(state.warning_message, Some(String::from("regular warning")));
    }

    #[test]
    fn set_agent_runtime_binding_sets_session_and_signature() {
        let agent_id = AgentId(String::from("agent-1"));
        let mut state = AppState::default();
        state.agents.push(sample_agent(&agent_id));

        let signature = sample_signature();
        set_agent_runtime_binding(
            &mut state,
            &agent_id,
            String::from("jefe-agent-1"),
            signature.clone(),
        );

        let binding = state
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .and_then(|agent| agent.runtime_binding.as_ref());

        assert!(binding.is_some());
        if let Some(binding) = binding {
            assert_eq!(binding.session_name, String::from("jefe-agent-1"));
            assert_eq!(binding.launch_signature, signature);
            assert!(!binding.attached);
        }
    }

    #[test]
    fn mark_and_clear_runtime_attachment_flags() {
        let agent_a = AgentId(String::from("agent-a"));
        let agent_b = AgentId(String::from("agent-b"));

        let mut first = sample_agent(&agent_a);
        first.runtime_binding = Some(RuntimeBinding {
            session_name: String::from("sess-a"),
            launch_signature: sample_signature(),
            attached: false,
            last_seen: None,
        });

        let mut second = sample_agent(&agent_b);
        second.runtime_binding = Some(RuntimeBinding {
            session_name: String::from("sess-b"),
            launch_signature: sample_signature(),
            attached: true,
            last_seen: None,
        });

        let mut state = AppState::default();
        state.agents.push(first);
        state.agents.push(second);

        mark_agent_runtime_attached(&mut state, &agent_a, true);
        assert!(
            state.agents[0]
                .runtime_binding
                .as_ref()
                .is_some_and(|binding| binding.attached)
        );

        clear_agent_runtime_attachment(&mut state);
        assert!(state.agents.iter().all(|agent| {
            agent
                .runtime_binding
                .as_ref()
                .is_none_or(|binding| !binding.attached)
        }));
    }

    #[test]
    fn mark_runtime_session_dead_sets_dead_and_detaches() {
        let agent_id = AgentId(String::from("agent-1"));
        let mut agent = sample_agent(&agent_id);
        agent.status = AgentStatus::Running;
        agent.runtime_binding = Some(RuntimeBinding {
            session_name: String::from("sess"),
            launch_signature: sample_signature(),
            attached: true,
            last_seen: None,
        });

        let mut state = AppState::default();
        state.agents.push(agent);

        mark_runtime_session_dead_if_present(&mut state, &agent_id);

        assert_eq!(state.agents[0].status, AgentStatus::Dead);
        assert!(
            state.agents[0]
                .runtime_binding
                .as_ref()
                .is_some_and(|binding| !binding.attached)
        );
    }

    #[test]
    fn to_persisted_state_carries_hide_idle_toggle() {
        let state = AppState {
            hide_idle_repositories: true,
            ..AppState::default()
        };

        let persisted = to_persisted_state(&state);
        assert!(persisted.hide_idle_repositories);
    }
}
