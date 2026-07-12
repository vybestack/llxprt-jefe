use jefe::domain::RepositoryId;
use jefe::messages::ActionsMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client, persist_state,
    to_persisted_state,
};

/// Resolve the GitHub client and parse a `owner/repo` slug into its parts.
///
/// All Actions orchestration tasks share this preamble: they need a working
/// `gh` client and a valid `(owner, repo)` pair. Centralizing it keeps the
/// "client unavailable" and "malformed slug" error handling consistent across
/// every dispatcher and guarantees every task always produces a result (so a
/// loading flag can never wedge on an early return).
fn gh_client_and_slug<'a>(
    ctx: &SharedContext,
    repo: &'a jefe::domain::Repository,
) -> Result<(jefe::github::GhClient, &'a str, &'a str), jefe::github::GhError> {
    let client = github_client(ctx)
        .ok_or_else(|| jefe::github::GhError::ApiError("gh client unavailable".to_string()))?;
    let owner_repo: Vec<&str> = repo.github_repo.split('/').collect();
    if owner_repo.len() != 2 {
        return Err(jefe::github::GhError::ApiError(format!(
            "malformed repository slug: {}",
            repo.github_repo
        )));
    }
    Ok((client, owner_repo[0], owner_repo[1]))
}

/// Route an `ActionsMessage` to the appropriate dispatcher.
pub(super) fn dispatch_actions_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: ActionsMessage,
) {
    match message {
        m @ (ActionsMessage::EnterMode
        | ActionsMessage::Reload
        | ActionsMessage::RefocusList
        | ActionsMessage::ApplyFilter
        | ActionsMessage::ClearFilter
        | ActionsMessage::ApplySearch
        | ActionsMessage::WorkflowDispatchSuccess { .. }) => {
            apply_and_persist(app_state, ctx, AppEvent::from(m));
            dispatch_actions_list_reload(app_state, ctx);
            dispatch_workflows_reload(app_state, ctx);
        }
        ActionsMessage::Navigate(dir) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::from(ActionsMessage::Navigate(dir)),
            );
            dispatch_run_detail_reload(app_state, ctx);
        }
        ActionsMessage::ScrollDetail(dir) => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::from(ActionsMessage::ScrollDetail(dir)),
            );
        }
        ActionsMessage::WorkflowDispatchSubmitted {
            scope_repo_id,
            workflow_id,
            ref_name,
            inputs,
        } => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::WorkflowDispatchSubmitted {
                    scope_repo_id: scope_repo_id.clone(),
                    workflow_id: workflow_id.clone(),
                    ref_name: ref_name.clone(),
                    inputs: inputs.clone(),
                },
            );
            dispatch_workflow_run(app_state, ctx, scope_repo_id, workflow_id, ref_name, inputs);
        }
        m => {
            apply_and_persist(app_state, ctx, AppEvent::from(m));
        }
    }
}

fn handle_list_reload_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<jefe::github::WorkflowRunListResponse, jefe::github::GhError>,
    req: (
        jefe::domain::RepositoryId,
        jefe::domain::ActionsFilter,
        u32,
        u64,
    ),
) {
    let (repo_id, filter, page, request_id) = req;
    let is_ok = res.is_ok();
    let event = match res {
        Ok(res) => AppEvent::ActionsRunsLoaded {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page,
            request_id,
            runs: res.runs,
            has_more: res.has_more,
        },
        Err(e) => AppEvent::ActionsRunsLoadFailed {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page,
            request_id,
            error: e.to_string(),
        },
    };
    apply_and_persist(&mut app_state, ctx, event);
    // After a successful page-1 list load, launch detail loading for the
    // newly-selected first run (BLOCKER 2 fix: exactly one request, allocated
    // by orchestration, not the reducer).
    if is_ok && page == 1 {
        dispatch_run_detail_reload(&mut app_state, ctx);
    }
}

fn handle_workflows_reload_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<Vec<jefe::domain::Workflow>, jefe::github::GhError>,
    repo_id: jefe::domain::RepositoryId,
    request_id: u64,
) {
    let event = match res {
        Ok(workflows) => AppEvent::WorkflowsLoaded {
            scope_repo_id: repo_id,
            request_id,
            workflows,
        },
        Err(e) => AppEvent::WorkflowsLoadFailed {
            scope_repo_id: repo_id,
            request_id,
            error: e.to_string(),
        },
    };
    apply_and_persist(&mut app_state, ctx, event);
}

fn handle_run_detail_reload_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<jefe::domain::WorkflowRunDetail, jefe::github::GhError>,
    repo_id: jefe::domain::RepositoryId,
    run_id: u64,
    request_id: u64,
) {
    let event = match res {
        Ok(detail) => AppEvent::ActionsDetailLoaded {
            scope_repo_id: repo_id,
            run_id,
            request_id,
            detail: Box::new(detail),
        },
        Err(e) => AppEvent::ActionsDetailLoadFailed {
            scope_repo_id: repo_id,
            run_id,
            request_id,
            error: e.to_string(),
        },
    };
    apply_and_persist(&mut app_state, ctx, event);
}

fn handle_workflow_dispatch_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<(), jefe::github::GhError>,
    repo_id: jefe::domain::RepositoryId,
    request_id: u64,
) {
    let event = match res {
        Ok(()) => AppEvent::WorkflowDispatchSuccess {
            scope_repo_id: repo_id,
            request_id,
        },
        Err(e) => AppEvent::WorkflowDispatchFailed {
            scope_repo_id: repo_id,
            request_id,
            error: e.to_string(),
        },
    };
    super::dispatch_app_event(&mut app_state, ctx, event);
}

const NO_REPO_MSG: &str = "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.";

/// Clear list loading and surface a no-repo error (SHOULD-FIX F).
fn persist_no_repo_error_list(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.loading.list = false;
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Clear detail loading and surface a no-repo error (SHOULD-FIX F).
fn persist_no_repo_error_detail(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.loading.detail = false;
    state.actions_state.detail_pending = None;
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Clear workflows loading and surface a no-repo error (SHOULD-FIX F).
fn persist_no_repo_error_workflows(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.workflows_pending = None;
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn dispatch_actions_list_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (repo, filter, page, request_id) = {
        let state = app_state.read();
        let repo = if let Some(r) = state.selected_repository() {
            r.clone()
        } else {
            drop(state);
            persist_no_repo_error_list(app_state, ctx);
            return;
        };
        let filter = state.actions_state.committed_filter.clone();
        let page = state.actions_state.page;
        let request_id = state.actions_state.next_list_request_id.saturating_add(1);
        let repo_clone = repo.clone();
        drop(state);
        (repo_clone, filter, page, request_id)
    };

    {
        let mut state = app_state.write();
        state.actions_state.next_list_request_id = request_id;
        state.actions_state.list_reload_pending = Some(jefe::state::ActionsListReloadPending {
            scope_repo_id: repo.id.clone(),
            filter: filter.clone(),
            page,
            request_id,
        });
        state.actions_state.loading.list = true;
    }

    let (repo_id, repo_id_panic) = (repo.id.clone(), repo.id.clone());
    let (filter_clone, filter_panic) = (filter.clone(), filter.clone());
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |app_state, ctx| {
            let res = (|| {
                let (client, owner, repo_name) = gh_client_and_slug(&ctx, &repo)?;
                client.list_runs(owner, repo_name, &filter_clone, page, 30)
            })();
            handle_list_reload_result(
                app_state,
                &ctx,
                res,
                (repo_id, filter_clone, page, request_id),
            );
        },
        move |app_state, ctx, msg| {
            let error_msg = format!("GitHub Actions list task panicked: {msg}");
            handle_list_reload_result(
                app_state,
                &ctx,
                Err(jefe::github::GhError::ApiError(error_msg)),
                (repo_id_panic, filter_panic, page, request_id),
            );
        },
    );
}

fn dispatch_workflows_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (repo, request_id) = {
        let state = app_state.read();
        let repo = if let Some(r) = state.selected_repository() {
            r.clone()
        } else {
            drop(state);
            persist_no_repo_error_workflows(app_state, ctx);
            return;
        };
        let request_id = state
            .actions_state
            .next_workflows_request_id
            .saturating_add(1);
        let repo_clone = repo.clone();
        drop(state);
        (repo_clone, request_id)
    };

    {
        let mut state = app_state.write();
        state.actions_state.next_workflows_request_id = request_id;
        state.actions_state.workflows_pending = Some(jefe::state::WorkflowsPending {
            scope_repo_id: repo.id.clone(),
            request_id,
        });
    }

    let repo_id_clone = repo.id.clone();
    let repo_id_clone_panic = repo_id_clone.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |app_state, ctx| {
            let res = (|| {
                let (client, owner, repo_name) = gh_client_and_slug(&ctx, &repo)?;
                client.list_workflows(owner, repo_name)
            })();
            handle_workflows_reload_result(app_state, &ctx, res, repo_id_clone, request_id);
        },
        move |app_state, ctx, msg| {
            let error_msg = format!("Workflows list task panicked: {msg}");
            handle_workflows_reload_result(
                app_state,
                &ctx,
                Err(jefe::github::GhError::ApiError(error_msg)),
                repo_id_clone_panic,
                request_id,
            );
        },
    );
}

fn dispatch_run_detail_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (repo, run_id, request_id) = {
        let state = app_state.read();
        let repo = if let Some(r) = state.selected_repository() {
            r.clone()
        } else {
            drop(state);
            persist_no_repo_error_detail(app_state, ctx);
            return;
        };
        let Some(idx) = state.actions_state.selected_run_index else {
            return;
        };
        if idx >= state.actions_state.runs.len() {
            return;
        }
        let run = &state.actions_state.runs[idx];
        let request_id = state.actions_state.next_detail_request_id.saturating_add(1);
        let repo_clone = repo.clone();
        let run_id = run.id;
        drop(state);
        (repo_clone, run_id, request_id)
    };

    {
        let mut state = app_state.write();
        state.actions_state.next_detail_request_id = request_id;
        state.actions_state.detail_pending = Some(jefe::state::ActionsDetailPending {
            scope_repo_id: repo.id.clone(),
            run_id,
            request_id,
        });
        state.actions_state.loading.detail = true;
    }

    let (repo_id, repo_id_panic) = (repo.id.clone(), repo.id.clone());
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |app_state, ctx| {
            let res = (|| {
                let (client, owner, repo_name) = gh_client_and_slug(&ctx, &repo)?;
                client.get_run_detail(owner, repo_name, run_id)
            })();
            handle_run_detail_reload_result(app_state, &ctx, res, repo_id, run_id, request_id);
        },
        move |app_state, ctx, msg| {
            let error_msg = format!("Run detail task panicked: {msg}");
            handle_run_detail_reload_result(
                app_state,
                &ctx,
                Err(jefe::github::GhError::ApiError(error_msg)),
                repo_id_panic,
                run_id,
                request_id,
            );
        },
    );
}

fn dispatch_workflow_run(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    scope_repo_id: RepositoryId,
    workflow_id: String,
    ref_name: String,
    inputs: Vec<(String, String)>,
) {
    let repo = {
        let state = app_state.read();
        let r = state
            .repositories
            .iter()
            .find(|r| r.id == scope_repo_id)
            .cloned();
        drop(state);
        r
    };
    let Some(repo) = repo else {
        return;
    };

    // Read the dispatch request id allocated by the reducer when the
    // WorkflowDispatchSubmitted message was applied, so the success/failure
    // result can be correlated against the pending operation (SHOULD-FIX G).
    let request_id = {
        let state = app_state.read();
        state
            .actions_state
            .dispatch_pending
            .as_ref()
            .map_or(0, |p| p.request_id)
    };

    let scope_repo_id_clone = scope_repo_id.clone();
    let scope_repo_id_clone_panic = scope_repo_id_clone.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |app_state, ctx| {
            let res = (|| {
                let (client, owner, repo_name) = gh_client_and_slug(&ctx, &repo)?;
                client.dispatch_workflow(owner, repo_name, &workflow_id, &ref_name, &inputs)
            })();
            handle_workflow_dispatch_result(app_state, &ctx, res, scope_repo_id_clone, request_id);
        },
        move |app_state, ctx, msg| {
            let error_msg = format!("Workflow dispatch task panicked: {msg}");
            handle_workflow_dispatch_result(
                app_state,
                &ctx,
                Err(jefe::github::GhError::ApiError(error_msg)),
                scope_repo_id_clone_panic,
                request_id,
            );
        },
    );
}
