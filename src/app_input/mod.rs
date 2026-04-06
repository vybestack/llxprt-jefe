use std::sync::Arc;

mod issues;
mod issues_dispatch;
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

        // ── Issue list / repo list navigation with auto-load ─────────────
        // @plan PLAN-20260329-ISSUES-MODE.P15
        // @requirement REQ-ISS-001, REQ-ISS-006, REQ-ISS-009
        AppEvent::IssuesNavigateUp | AppEvent::IssuesNavigateDown => {
            let (focus, prev_repo_idx, prev_issue_idx) = {
                let state = app_state.read();
                (
                    state.issues_state.issue_focus,
                    state.selected_repository_index,
                    state.issues_state.selected_issue_index,
                )
            };

            apply_and_persist(app_state, ctx, evt);

            match focus {
                jefe::state::IssueFocus::RepoList => {
                    let new_repo_idx = app_state.read().selected_repository_index;
                    if new_repo_idx != prev_repo_idx {
                        {
                            let mut state = app_state.write();
                            state.issues_state.issues.clear();
                            state.issues_state.selected_issue_index = None;
                            state.issues_state.issue_detail = None;
                            state.issues_state.list_cursor = None;
                            state.issues_state.has_more_issues = false;
                            state.issues_state.error = None;
                            if state.issues_state.inline_state != jefe::state::InlineState::None {
                                state.issues_state.draft_notice =
                                    Some("Unsent draft discarded".to_string());
                            }
                            state.issues_state.inline_state = jefe::state::InlineState::None;
                            state.issues_state.agent_chooser = None;
                            state.issues_state.list_loading = true;
                        }
                        // Trigger issue list load; RefocusIssueList changes
                        // focus to IssueList, so restore RepoList focus after.
                        dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
                        app_state.write().issues_state.issue_focus =
                            jefe::state::IssueFocus::RepoList;
                    }
                }
                jefe::state::IssueFocus::IssueList => {
                    let new_issue_idx = app_state.read().issues_state.selected_issue_index;
                    if new_issue_idx != prev_issue_idx {
                        // Build a lightweight preview from list data (no I/O)
                        issues_dispatch::preview_issue_from_list(app_state);
                    }
                }
                jefe::state::IssueFocus::IssueDetail => {}
            }
        }

        // ── Issues-mode events that require I/O ────────────────────────────
        // @plan PLAN-20260329-ISSUES-MODE.P15
        // @requirement REQ-ISS-006, REQ-ISS-013
        AppEvent::EnterIssuesMode | AppEvent::RefocusIssueList => {
            // Apply state transition first (sets list_loading = true, etc.)
            apply_and_persist(app_state, ctx, evt);

            // Now perform the actual issue list fetch
            let (scope_repo_id, owner, repo, filter, cursor, page_size) = {
                let state = app_state.read();
                let gh_repo = issues_dispatch::resolve_gh_repo(&state);
                let filter = state.issues_state.committed_filter.clone();
                let cursor = state.issues_state.list_cursor.clone();
                let scope_repo_id = issues_dispatch::current_scope_repo_id(&state);
                (scope_repo_id, gh_repo.0, gh_repo.1, filter, cursor, 30u32)
            };

            if owner.is_empty() || repo.is_empty() {
                let mut state = app_state.write();
                state.issues_state.list_loading = false;
                state.issues_state.error = Some(
                    "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
                );
                persist_state_snapshot(ctx, &state);
                return;
            }

            let result = if let Some(ctx_arc) = &ctx
                && let Ok(ctx_guard) = ctx_arc.lock()
            {
                Some(ctx_guard.gh_client.list_issues(
                    &owner,
                    &repo,
                    &filter,
                    cursor.as_deref(),
                    page_size,
                ))
            } else {
                None
            };

            match result {
                Some(Ok(response)) => {
                    let has_issues = !response.issues.is_empty();
                    let mut state = app_state.write();
                    *state = std::mem::take(&mut *state).apply(AppEvent::IssueListLoaded {
                        scope_repo_id: scope_repo_id.clone(),
                        issues: response.issues,
                        cursor: response.cursor,
                        has_more: response.has_more,
                    });
                    drop(state);
                    // Show preview for the first issue (no I/O)
                    if has_issues {
                        issues_dispatch::preview_issue_from_list(app_state);
                    }
                }
                Some(Err(e)) => {
                    let mut state = app_state.write();
                    *state = std::mem::take(&mut *state).apply(AppEvent::IssueListLoadFailed {
                        scope_repo_id: scope_repo_id.clone(),
                        error: e.to_string(),
                    });
                }
                None => {
                    let mut state = app_state.write();
                    *state = std::mem::take(&mut *state).apply(AppEvent::IssueListLoadFailed {
                        scope_repo_id,
                        error: "Application context unavailable".to_string(),
                    });
                }
            }
        }

        // @plan PLAN-20260329-ISSUES-MODE.P15
        // @requirement REQ-ISS-009
        AppEvent::IssuesEnter => {
            apply_and_persist(app_state, ctx, AppEvent::IssuesEnter);
            issues_dispatch::load_issue_detail_for_selection(app_state, ctx);
        }

        // ── Send issue to agent ──────────────────────────────────────────
        // @requirement REQ-ISS-011
        AppEvent::AgentChooserConfirm => {
            // Gather chosen agent and issue data before clearing the chooser
            let send_info = {
                let state = app_state.read();
                let chooser = state.issues_state.agent_chooser.as_ref();
                let detail = state.issues_state.issue_detail.as_ref();
                let subfocus = state.issues_state.detail_subfocus;

                (|| -> Option<_> {
                    let (ch, det) = chooser.zip(detail)?;
                    let (agent_id, _) = ch.agents.get(ch.selected_index)?.clone();
                    let agent = state.agents.iter().find(|a| a.id == agent_id)?.clone();
                    let repo = state.repository_by_id(&agent.repository_id)?;
                    let repo_slug = repo.slug.clone();
                    let base_prompt = repo.issue_base_prompt.clone();
                    let signature = launch_signature_for_agent(&agent, repo);

                    let focused_comment = match subfocus {
                        jefe::state::DetailSubfocus::Comment(idx) => det.comments.get(idx).cloned(),
                        _ => None,
                    };
                    let payload = jefe::github::GhClient::build_send_payload(
                        &repo_slug,
                        det,
                        focused_comment.as_ref(),
                        &base_prompt,
                    );
                    Some((agent_id, agent.work_dir.clone(), signature, payload))
                })()
            };

            // Clear the chooser regardless
            apply_and_persist(app_state, ctx, evt);

            let Some((agent_id, work_dir, signature, payload)) = send_info else {
                return;
            };

            // Write the issue prompt to a file in the agent's work dir
            let prompt_dir = work_dir.join(".jefe");
            if let Err(e) = std::fs::create_dir_all(&prompt_dir) {
                let mut state = app_state.write();
                *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
                    error: format!("Failed to create .jefe dir: {e}"),
                });
                return;
            }
            let prompt_path = prompt_dir.join("issue-prompt.md");
            let prompt_content = issues_dispatch::format_issue_prompt(&payload);
            if let Err(e) = std::fs::write(&prompt_path, &prompt_content) {
                let mut state = app_state.write();
                *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
                    error: format!("Failed to write issue prompt: {e}"),
                });
                return;
            }

            // Clone signature with -i flag for this launch only
            let mut launch_sig = signature;
            launch_sig.mode_flags.push("-i".to_owned());
            launch_sig.mode_flags.push(
                "Read and work on the GitHub issue described in .jefe/issue-prompt.md".to_owned(),
            );

            if !preflight_or_prompt(app_state, ctx, &agent_id, &launch_sig) {
                return;
            }

            // Launch the agent
            let mut launched = false;
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                match ctx_guard
                    .runtime
                    .spawn_session_fresh(&agent_id, &work_dir, &launch_sig)
                {
                    Ok(()) => {
                        std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
                        match ctx_guard.runtime.attach(&agent_id) {
                            Ok(()) => {
                                launched = true;
                            }
                            Err(e) => {
                                warn!(
                                    agent_id = %agent_id.0,
                                    error = %e,
                                    "could not attach agent after issue send"
                                );
                                let _ = ctx_guard.runtime.mark_session_dead(&agent_id);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            agent_id = %agent_id.0,
                            error = %e,
                            "could not spawn agent for issue send"
                        );
                    }
                }
            }

            let mut state = app_state.write();
            if launched {
                if let Some(agent) = state.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.status = jefe::domain::AgentStatus::Running;
                    let session_name = jefe::runtime::RuntimeSession::session_name_for(&agent_id);
                    agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
                        session_name,
                        launch_signature: launch_sig,
                        attached: false,
                        last_seen: None,
                    });
                }
                clear_agent_runtime_attachment(&mut state);
                mark_agent_runtime_attached(&mut state, &agent_id, true);
            } else {
                *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
                    error: "Failed to launch agent".to_string(),
                });
            }
            persist_state_snapshot(ctx, &state);
        }

        // @plan PLAN-20260329-ISSUES-MODE.P15
        // @requirement REQ-ISS-010
        AppEvent::InlineSubmit => {
            let submit_action = {
                let state = app_state.read();
                match &state.issues_state.inline_state {
                    jefe::state::InlineState::Composer { target, text, .. } => {
                        Some(InlineSubmitAction::Create {
                            target: target.clone(),
                            text: text.clone(),
                        })
                    }
                    jefe::state::InlineState::Editor { target, text, .. } => {
                        Some(InlineSubmitAction::Edit {
                            target: target.clone(),
                            text: text.clone(),
                        })
                    }
                    jefe::state::InlineState::None => None,
                }
            };

            let Some(action) = submit_action else {
                return;
            };

            let (owner, repo) = {
                let state = app_state.read();
                issues_dispatch::resolve_gh_repo(&state)
            };

            if owner.is_empty() || repo.is_empty() {
                let mut state = app_state.write();
                *state = std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                    error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
                });
                return;
            }

            match action {
                InlineSubmitAction::Create { target, text } => {
                    if target == jefe::state::ComposerTarget::NewIssue {
                        let (title, body) =
                            if let Some((first, rest)) = text.split_once(char::from(0x0Au8)) {
                                (first.trim().to_string(), rest.to_string())
                            } else {
                                (text.trim().to_string(), String::new())
                            };

                        if title.is_empty() {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                                error: "Issue title cannot be empty".to_string(),
                            });
                            return;
                        }

                        let result = if let Some(ctx_arc) = &ctx
                            && let Ok(ctx_guard) = ctx_arc.lock()
                        {
                            Some(
                                ctx_guard
                                    .gh_client
                                    .create_issue(&owner, &repo, &title, &body),
                            )
                        } else {
                            None
                        };

                        match result {
                            Some(Ok(issue)) => {
                                {
                                    let mut state = app_state.write();
                                    state.issues_state.inline_state =
                                        jefe::state::InlineState::None;
                                    state.issues_state.error = None;
                                    state.issues_state.draft_notice =
                                        Some(format!("Created issue #{}", issue.number));
                                }
                                dispatch_app_event(app_state, ctx, AppEvent::RefocusIssueList);
                            }
                            Some(Err(e)) => {
                                let mut state = app_state.write();
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                                        error: e.to_string(),
                                    });
                            }
                            None => {
                                let mut state = app_state.write();
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                                        error: "Application context unavailable".to_string(),
                                    });
                            }
                        }
                    } else {
                        let issue_number = {
                            let state = app_state.read();
                            state.issues_state.issue_detail.as_ref().map(|d| d.number)
                        };
                        let Some(number) = issue_number else { return };

                        let result = if let Some(ctx_arc) = &ctx
                            && let Ok(ctx_guard) = ctx_arc.lock()
                        {
                            Some(
                                ctx_guard
                                    .gh_client
                                    .create_comment(&owner, &repo, number, &text),
                            )
                        } else {
                            None
                        };

                        match result {
                            Some(Ok(comment)) => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state)
                                    .apply(AppEvent::CommentCreated { comment });
                            }
                            Some(Err(e)) => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state).apply(
                                    AppEvent::CommentCreateFailed {
                                        error: e.to_string(),
                                    },
                                );
                            }
                            None => {}
                        }
                    }
                }
                InlineSubmitAction::Edit { target, text } => match target {
                    jefe::state::EditorTarget::IssueBody => {
                        let issue_number = {
                            let state = app_state.read();
                            state.issues_state.issue_detail.as_ref().map(|d| d.number)
                        };
                        let Some(number) = issue_number else { return };

                        let result = if let Some(ctx_arc) = &ctx
                            && let Ok(ctx_guard) = ctx_arc.lock()
                        {
                            Some(
                                ctx_guard
                                    .gh_client
                                    .update_issue_body(&owner, &repo, number, &text),
                            )
                        } else {
                            None
                        };

                        match result {
                            Some(Ok(())) => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state)
                                    .apply(AppEvent::IssueBodyUpdated { body: text });
                            }
                            Some(Err(e)) => {
                                let mut state = app_state.write();
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                                        error: e.to_string(),
                                    });
                            }
                            None => {}
                        }
                    }
                    jefe::state::EditorTarget::Comment { comment_index } => {
                        let comment_id = {
                            let state = app_state.read();
                            state
                                .issues_state
                                .issue_detail
                                .as_ref()
                                .and_then(|d| d.comments.get(comment_index))
                                .map(|c| c.comment_id)
                        };
                        let Some(cid) = comment_id else { return };

                        let result = if let Some(ctx_arc) = &ctx
                            && let Ok(ctx_guard) = ctx_arc.lock()
                        {
                            Some(
                                ctx_guard
                                    .gh_client
                                    .update_comment(&owner, &repo, cid, &text),
                            )
                        } else {
                            None
                        };

                        match result {
                            Some(Ok(())) => {
                                let mut state = app_state.write();
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::CommentUpdated {
                                        comment_index,
                                        body: text,
                                    });
                            }
                            Some(Err(e)) => {
                                let mut state = app_state.write();
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::MutationFailed {
                                        error: e.to_string(),
                                    });
                            }
                            None => {}
                        }
                    }
                },
            }
        }

        _ => {
            apply_and_persist(app_state, ctx, evt);
        }
    }
}

/// Helper enum for classifying inline submit actions.
enum InlineSubmitAction {
    Create {
        target: jefe::state::ComposerTarget,
        text: String,
    },
    Edit {
        target: jefe::state::EditorTarget,
        text: String,
    },
}
#[cfg(test)]
#[path = "app_input_tests.rs"]
mod tests;
