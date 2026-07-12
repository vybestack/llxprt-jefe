//! PR-mode property-edit dispatch helpers (issue #175).
//!
//! Mirrors `prs_mutation.rs` and `issues_property_edit.rs`. On
//! `PropertyEditorConfirm`, reads the editor state, spawns the gh task,
//! delivers success/failure events, then on success triggers the silent
//! detail refresh.

use jefe::domain::RepositoryId;
use jefe::state::{AppEvent, PrPropertyEditorState, PrPropertyKind};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    prs_dispatch,
};

/// Handle a property-editor confirm for PRs.
pub fn handle_pr_property_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(action) = resolve_pr_property_action(app_state) else {
        return;
    };
    let Some(repo) = pr_repo_target(app_state) else {
        report_missing_repo(app_state, ctx, &action);
        return;
    };
    dispatch_pr_property_edit(app_state, ctx, repo, action);
}

#[derive(Clone)]
struct PrPropertyAction {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    kind: PrPropertyKind,
    editor: PrPropertyEditorState,
}

fn resolve_pr_property_action(app_state: &AppStateHandle) -> Option<PrPropertyAction> {
    let (scope_repo_id, pr_number, kind, editor) = {
        let state = app_state.read();
        let editor = state.prs_state.property_editor.as_ref()?.clone();
        let pr_number = state.prs_state.pr_detail.as_ref()?.number;
        let scope_repo_id = prs_dispatch::current_pr_scope_repo_id(&state);
        let kind = editor.kind;
        drop(state);
        (scope_repo_id, pr_number, kind, editor)
    };
    Some(PrPropertyAction {
        scope_repo_id,
        pr_number,
        kind,
        editor,
    })
}

#[derive(Clone)]
struct PrRepoTarget {
    owner: String,
    repo: String,
}

fn pr_repo_target(app_state: &AppStateHandle) -> Option<PrRepoTarget> {
    let state = app_state.read();
    let (owner, repo) = prs_dispatch::resolve_pr_gh_repo(&state);
    (!owner.is_empty() && !repo.is_empty()).then_some(PrRepoTarget { owner, repo })
}

fn report_missing_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: &PrPropertyAction,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::PrPropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            error: "No GitHub repository configured.".to_string(),
        },
    );
}

fn dispatch_pr_property_edit(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    repo: PrRepoTarget,
    action: PrPropertyAction,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_property_edit_event(&ctx, &repo, &action);
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrPropertyEditFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    pr_number: panic_action.pr_number,
                    error: format!("GitHub PR property edit task panicked: {message}"),
                },
            );
        },
    );
}

fn pr_property_edit_event(
    ctx: &SharedContext,
    repo: &PrRepoTarget,
    action: &PrPropertyAction,
) -> AppEvent {
    let result = github_client(ctx).map(|client| execute_pr_property_edit(client, repo, action));
    match result {
        Some(Ok(())) => AppEvent::PrPropertyEditSucceeded {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
        },
        Some(Err(error)) => AppEvent::PrPropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            error: error.to_string(),
        },
        None => AppEvent::PrPropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn execute_pr_property_edit(
    client: jefe::github::GhClient,
    repo: &PrRepoTarget,
    action: &PrPropertyAction,
) -> Result<(), jefe::github::GhError> {
    let number = action.pr_number;
    let target = jefe::github::PropertyEditTarget {
        owner: &repo.owner,
        repo: &repo.repo,
        number,
        is_pr: true,
    };
    match action.kind {
        PrPropertyKind::Labels => {
            let (to_add, to_remove) = compute_pr_multi_diff(&action.editor);
            client.edit_labels(target, &to_add, &to_remove)
        }
        PrPropertyKind::Assignees => {
            let (to_add, to_remove) = compute_pr_multi_diff(&action.editor);
            client.edit_assignees(target, &to_add, &to_remove)
        }
        PrPropertyKind::Milestone => {
            let selected = action
                .editor
                .options
                .iter()
                .find(|o| o.selected)
                .map(|o| o.label.as_str());
            match selected {
                Some("(clear)") | None => {
                    client.clear_milestone(&repo.owner, &repo.repo, number, true)
                }
                Some(name) => client.set_milestone(&repo.owner, &repo.repo, number, true, name),
            }
        }
        PrPropertyKind::Title => {
            let title = action.editor.title_text.trim();
            if title.is_empty() {
                return Err(jefe::github::GhError::ApiError(
                    "Title cannot be empty".to_string(),
                ));
            }
            client.set_title(&repo.owner, &repo.repo, number, true, title)
        }
        PrPropertyKind::State => {
            let want_closed = action
                .editor
                .options
                .iter()
                .any(|o| o.selected && o.label == "Closed");
            if want_closed {
                client.close_item(&repo.owner, &repo.repo, number, true)
            } else {
                client.reopen_item(&repo.owner, &repo.repo, number, true)
            }
        }
    }
}

fn compute_pr_multi_diff(editor: &PrPropertyEditorState) -> (Vec<String>, Vec<String>) {
    let to_add: Vec<String> = editor
        .options
        .iter()
        .filter(|o| o.selected)
        .map(|o| o.label.clone())
        .collect();
    let to_remove: Vec<String> = editor
        .options
        .iter()
        .filter(|o| !o.selected)
        .map(|o| o.label.clone())
        .collect();
    (to_add, to_remove)
}

/// Handle property editor options loading for PRs.
pub fn handle_pr_property_options_load(app_state: &AppStateHandle, ctx: &SharedContext) {
    let Some(params) = resolve_pr_options_load_params(app_state) else {
        return;
    };
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_options_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrPropertyEditorOptionsLoaded {
                    options: Vec::new(),
                },
            );
        },
    );
}

#[derive(Clone)]
struct PrOptionsLoadParams {
    kind: PrPropertyKind,
    owner: String,
    repo: String,
}

fn resolve_pr_options_load_params(app_state: &AppStateHandle) -> Option<PrOptionsLoadParams> {
    let (kind, owner, repo) = {
        let state = app_state.read();
        let editor = state.prs_state.property_editor.as_ref()?;
        let (owner, repo) = prs_dispatch::resolve_pr_gh_repo(&state);
        let kind = editor.kind;
        drop(state);
        (kind, owner, repo)
    };
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(PrOptionsLoadParams { kind, owner, repo })
}

fn pr_options_load_event(ctx: &SharedContext, params: &PrOptionsLoadParams) -> AppEvent {
    let result = github_client(ctx).map(|client| fetch_pr_options(client, params));
    let options = match result {
        Some(Ok(opts)) => opts,
        _ => Vec::new(),
    };
    AppEvent::PrPropertyEditorOptionsLoaded { options }
}

fn fetch_pr_options(
    client: jefe::github::GhClient,
    params: &PrOptionsLoadParams,
) -> Result<Vec<(String, bool)>, jefe::github::GhError> {
    match params.kind {
        PrPropertyKind::Labels => {
            let names = client.fetch_label_names(&params.owner, &params.repo)?;
            Ok(names.into_iter().map(|n| (n, false)).collect())
        }
        PrPropertyKind::Assignees => {
            let logins = client.fetch_assignee_logins(&params.owner, &params.repo)?;
            Ok(logins.into_iter().map(|l| (l, false)).collect())
        }
        PrPropertyKind::Milestone => {
            let titles = client.fetch_milestone_titles(&params.owner, &params.repo)?;
            Ok(titles.into_iter().map(|t| (t, false)).collect())
        }
        _ => Ok(Vec::new()),
    }
}

/// Post-mutation refresh: after a property edit succeeds, reload the PR detail.
pub fn dispatch_pr_property_post_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    event: AppEvent,
) {
    apply_and_persist(app_state, ctx, event);
    prs_dispatch::load_pr_detail_for_selection(app_state, ctx);
}
