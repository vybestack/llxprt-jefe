//! Actions-mode orchestration: reload, page load-more, detail, workflows, dispatch.

use jefe::domain::{ActionsFilter, RepositoryId};
use jefe::messages::ActionsMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client,
    list_loader::{ListLoad, ListLoader},
    persist_state, to_persisted_state,
};

pub(super) fn actions_repository_target(
    repo: &jefe::domain::Repository,
) -> Result<(&str, &str), jefe::github::GhError> {
    let Some((owner, name)) = repo.github_repo.split_once('/') else {
        return Err(jefe::github::GhError::ApiError(format!(
            "malformed repository slug: {}",
            repo.github_repo
        )));
    };
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return Err(jefe::github::GhError::ApiError(format!(
            "malformed repository slug: {}",
            repo.github_repo
        )));
    }
    Ok((owner, name))
}

/// Resolve the GitHub client and parse a `owner/repo` slug into its parts.
fn gh_client_and_slug<'a>(
    ctx: &SharedContext,
    repo: &'a jefe::domain::Repository,
) -> Result<(jefe::github::GhClient, &'a str, &'a str), jefe::github::GhError> {
    let client = github_client(ctx)
        .ok_or_else(|| jefe::github::GhError::ApiError("gh client unavailable".to_string()))?;
    let (owner, name) = actions_repository_target(repo)?;
    Ok((client, owner, name))
}

fn dispatch_actions_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    dir: jefe::messages::NavDir,
) {
    let previous_repo_id = app_state
        .read()
        .selected_repository()
        .map(|repo| repo.id.clone());
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::from(ActionsMessage::Navigate(dir)),
    );
    let current_repo_id = app_state
        .read()
        .selected_repository()
        .map(|repo| repo.id.clone());
    if previous_repo_id == current_repo_id {
        dispatch_run_detail_reload(app_state, ctx);
        load_more_runs_if_at_end(app_state, ctx);
    } else {
        dispatch_actions_list_reload(app_state, ctx);
        dispatch_workflows_reload(app_state, ctx);
    }
}

/// Synchronize transient Actions geometry through the typed reducer pipeline.
pub fn synchronize_actions_geometry(
    app_state: &mut AppStateHandle,
    term_cols: u16,
    term_rows: u16,
) {
    let (error_visible, filter_open, current) = {
        let state = app_state.read();
        (
            state.actions_state.error.is_some(),
            state.actions_state.ui.filter_ui_open,
            (
                state.actions_state.detail_viewport_rows,
                state.actions_state.detail_content_width,
            ),
        )
    };
    let geometry =
        jefe::layout::actions_detail_geometry(term_cols, term_rows, error_visible, filter_open);
    if current == (geometry.viewport_rows, geometry.content_width) {
        return;
    }
    let event = AppEvent::ActionsSetDetailGeometry {
        viewport_rows: geometry.viewport_rows,
        content_width: geometry.content_width,
    };
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(event);
}

fn synchronize_current_actions_geometry(app_state: &mut AppStateHandle) {
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    synchronize_actions_geometry(app_state, term_cols, term_rows);
}

/// Route an `ActionsMessage` to the appropriate dispatcher.
pub(super) fn dispatch_actions_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: ActionsMessage,
) {
    if !matches!(message, ActionsMessage::SetDetailGeometry { .. }) {
        synchronize_current_actions_geometry(app_state);
    }
    match message {
        m @ (ActionsMessage::EnterMode
        | ActionsMessage::EnterModeWithPrFilter { .. }
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
        ActionsMessage::Navigate(dir) => dispatch_actions_navigation(app_state, ctx, dir),
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

/// Load more runs if the selection is at the end of the list and more pages
/// are available. Mirrors `load_more_issues_if_at_end`.
pub(super) fn load_more_runs_if_at_end(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let should_load = {
        let state = app_state.read();
        let selected = state.actions_state.list.selected_index();
        state.actions_state.list.should_load_more(selected)
    };
    if should_load {
        dispatch_actions_page_fetch(app_state, ctx);
    }
}

fn handle_list_reload_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<jefe::github::WorkflowRunListResponse, jefe::github::GhError>,
    req: ActionsListRequest,
) {
    let ActionsListRequest {
        repo_id,
        filter,
        page,
        request_id,
    } = req;
    let should_reload_detail = res.is_ok()
        && page == 1
        && reload_result_matches_pending(
            &app_state,
            &repo_id,
            &filter,
            jefe::domain::ListRequestId::from_raw(request_id),
        );
    let event = match res {
        Ok(res) => AppEvent::ActionsRunsLoaded {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page,
            request_id,
            runs: res.runs,
            has_more: res.has_more,
        },
        Err(e) => {
            let error = e.to_string();
            // Offer the in-app auth dialog when gh is unauthenticated (issue #244).
            if super::auth_remediation::offer_auth_remediation(&mut app_state, ctx, &error) {
                return;
            }
            AppEvent::ActionsRunsLoadFailed {
                scope_repo_id: repo_id,
                filter: Box::new(filter),
                page,
                request_id,
                error,
            }
        }
    };
    apply_and_persist(&mut app_state, ctx, event);
    if should_reload_detail {
        dispatch_run_detail_reload(&mut app_state, ctx);
    }
}

fn reload_result_matches_pending(
    app_state: &AppStateHandle,
    repo_id: &RepositoryId,
    filter: &ActionsFilter,
    request_id: jefe::domain::ListRequestId,
) -> bool {
    let identity = jefe::state::ActionsListIdentity {
        scope_repo_id: repo_id.clone(),
        filter: filter.clone(),
    };
    let correlation = jefe::state::pagination::LoadCorrelation::Reload {
        identity,
        request_id,
    };
    !app_state.read().actions_state.list.is_stale(&correlation)
}

fn handle_list_page_result(
    mut app_state: AppStateHandle,
    ctx: &SharedContext,
    res: Result<jefe::github::WorkflowRunListResponse, jefe::github::GhError>,
    req: ActionsListRequest,
) {
    let ActionsListRequest {
        repo_id,
        filter,
        page,
        request_id,
    } = req;
    let event = match res {
        Ok(res) => AppEvent::ActionsRunsPageLoaded {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page,
            request_id,
            runs: res.runs,
            has_more: res.has_more,
        },
        Err(e) => AppEvent::ActionsRunsPageLoadFailed {
            scope_repo_id: repo_id,
            filter: Box::new(filter),
            page,
            request_id,
            error: e.to_string(),
        },
    };
    apply_and_persist(&mut app_state, ctx, event);
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

/// Parameters for a list request (reload or page).
#[derive(Clone)]
struct ActionsListRequest {
    repo_id: jefe::domain::RepositoryId,
    filter: ActionsFilter,
    page: u32,
    request_id: u64,
}

/// Clear list loading and surface a no-repo error.
fn persist_no_repo_error_list(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.list.clear();
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Clear detail loading and surface a no-repo error.
fn persist_no_repo_error_detail(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.loading.detail = false;
    state.actions_state.detail_pending = None;
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Clear workflows loading and surface a no-repo error.
fn persist_no_repo_error_workflows(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.actions_state.workflows_pending = None;
    state.actions_state.error = Some(NO_REPO_MSG.to_string());
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn dispatch_actions_list_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (repo, filter) = {
        let state = app_state.read();
        let repo = if let Some(r) = state.selected_repository() {
            r.clone()
        } else {
            drop(state);
            persist_no_repo_error_list(app_state, ctx);
            return;
        };
        let filter = state.actions_state.committed_filter.clone();
        drop(state);
        (repo, filter)
    };

    let request_id = {
        let mut write_state = app_state.write();
        let identity = jefe::state::ActionsListIdentity {
            scope_repo_id: repo.id.clone(),
            filter: filter.clone(),
        };
        let Some(id) = ListLoader::begin(
            &mut write_state.actions_state.list,
            identity,
            ListLoad::Reload,
        ) else {
            return;
        };
        let persisted = to_persisted_state(&write_state);
        drop(write_state);
        persist_state(ctx, &persisted);
        id.get()
    };

    spawn_list_task(
        app_state,
        ctx,
        ActionsListRequest {
            repo_id: repo.id.clone(),
            filter,
            page: 1,
            request_id,
        },
        repo,
        handle_list_reload_result,
    );
}

/// Fetch the next page (load-more). The page number and request id are
/// derived from the `PaginatedList` state (next_page + next_request_id).
fn dispatch_actions_page_fetch(app_state: &mut AppStateHandle, ctx: &SharedContext) {
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
        let token = state.actions_state.list.next_page().clone();
        let jefe::domain::PageToken::PageNumber(page) = token else {
            drop(state);
            return;
        };
        drop(state);

        let mut write_state = app_state.write();
        let identity = jefe::state::ActionsListIdentity {
            scope_repo_id: repo.id.clone(),
            filter: filter.clone(),
        };
        let Some(id) = ListLoader::begin(
            &mut write_state.actions_state.list,
            identity,
            ListLoad::Page(jefe::domain::PageToken::PageNumber(page)),
        ) else {
            return;
        };
        let persisted = to_persisted_state(&write_state);
        drop(write_state);
        persist_state(ctx, &persisted);
        (repo, filter, page, id.get())
    };

    spawn_list_task(
        app_state,
        ctx,
        ActionsListRequest {
            repo_id: repo.id.clone(),
            filter,
            page,
            request_id,
        },
        repo,
        handle_list_page_result,
    );
}

/// Spawn a gh task to fetch a list page and route the result through `handler`.
fn spawn_list_task(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    req: ActionsListRequest,
    repo: jefe::domain::Repository,
    handler: fn(
        AppStateHandle,
        &SharedContext,
        Result<jefe::github::WorkflowRunListResponse, jefe::github::GhError>,
        ActionsListRequest,
    ),
) {
    let page = req.page;
    let request_id = req.request_id;
    let filter_clone = req.filter.clone();
    let (repo_id, repo_id_panic) = (repo.id.clone(), repo.id.clone());
    let filter_panic = filter_clone.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |app_state, ctx| {
            let res = (|| {
                let (client, owner, repo_name) = gh_client_and_slug(&ctx, &repo)?;
                client.list_runs(owner, repo_name, &filter_clone, page, 30)
            })();
            handler(
                app_state,
                &ctx,
                res,
                ActionsListRequest {
                    repo_id,
                    filter: filter_clone,
                    page,
                    request_id,
                },
            );
        },
        move |app_state, ctx, msg| {
            let error_msg = format!("GitHub Actions list task panicked: {msg}");
            handler(
                app_state,
                &ctx,
                Err(jefe::github::GhError::ApiError(error_msg)),
                ActionsListRequest {
                    repo_id: repo_id_panic,
                    filter: filter_panic,
                    page,
                    request_id,
                },
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
        let Some(idx) = state.actions_state.list.selected_index() else {
            return;
        };
        let runs = state.actions_state.list.items();
        if idx >= runs.len() {
            return;
        }
        let run = &runs[idx];
        let request_id = state.actions_state.next_detail_request_id.saturating_add(1);
        let repo_clone = repo.clone();
        let run_id = run.id;
        drop(state);
        (repo_clone, run_id, request_id)
    };

    apply_and_persist(
        app_state,
        ctx,
        AppEvent::ActionsBeginDetailReload {
            scope_repo_id: repo.id.clone(),
            run_id,
            request_id,
        },
    );

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
