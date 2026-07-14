//! PR-mode property-edit dispatch helpers (issue #175).
//!
//! Mirrors `prs_mutation.rs` and `issues_property_edit.rs`. On
//! `PropertyEditorConfirm`, reads the editor state, spawns the gh task,
//! delivers success/failure events, then on success triggers the silent
//! detail refresh.

use jefe::domain::RepositoryId;
use jefe::github::compute_assignee_diff;
use jefe::github::compute_label_diff;
use jefe::state::{AppEvent, PROPERTY_CLEAR_LABEL, PrPropertyEditorState, PrPropertyKind};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    prs_dispatch,
};

/// Handle a property-editor confirm for PRs.
pub fn handle_pr_property_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(action) = resolve_pr_property_action(app_state) else {
        return;
    };
    let repo = match pr_repo_target(app_state) {
        Ok(Some(repo)) => repo,
        Ok(None) => {
            report_missing_repo(app_state, ctx, &action, None);
            return;
        }
        Err(error) => {
            report_missing_repo(app_state, ctx, &action, Some(error));
            return;
        }
    };
    // F6/H1: block confirm while options are still loading or failed to load.
    if action.editor.options_loading {
        set_editor_error(app_state, ctx, "Options still loading");
        return;
    }
    if action.editor.loading_failed {
        set_editor_error(app_state, ctx, "Cannot edit: option load failed");
        return;
    }
    // H4: check for empty title before marking pending
    if action.kind == PrPropertyKind::Title && action.editor.title_text.trim().is_empty() {
        set_editor_error(app_state, ctx, "Title cannot be empty");
        return;
    }
    let Some(request_id) = mark_mutation_pending(app_state, &action) else {
        return;
    };
    dispatch_pr_property_edit(app_state, ctx, repo, action, request_id);
}

fn set_editor_error(app_state: &mut AppStateHandle, ctx: &SharedContext, error: &str) {
    // F5: emit a deterministic validation-error event that sets the open
    // editor's error directly, WITHOUT mutation correlation.
    let kind = app_state
        .read()
        .prs_state
        .property_editor
        .as_ref()
        .map_or(PrPropertyKind::Title, |e| e.kind);
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::PrPropertyEditorValidationError {
            kind,
            error: error.to_string(),
        },
    );
}

fn mark_mutation_pending(app_state: &mut AppStateHandle, action: &PrPropertyAction) -> Option<u64> {
    let mut state = app_state.write();
    state.mark_pr_property_mutation_pending(action.scope_repo_id.clone(), action.pr_number)
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

fn pr_repo_target(app_state: &AppStateHandle) -> Result<Option<PrRepoTarget>, String> {
    let state = app_state.read();
    let (owner, repo) =
        prs_dispatch::resolve_pr_gh_repo_or_error(&state).map_err(|error| error.message)?;
    Ok((!owner.is_empty() && !repo.is_empty()).then_some(PrRepoTarget { owner, repo }))
}

fn report_missing_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: &PrPropertyAction,
    malformed_message: Option<String>,
) {
    // F5: use the validation-error event so the error reaches the open editor
    // without requiring mutation correlation.
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::PrPropertyEditorValidationError {
            kind: action.kind,
            error: malformed_message
                .unwrap_or_else(|| "No GitHub repository configured.".to_string()),
        },
    );
}

fn dispatch_pr_property_edit(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    repo: PrRepoTarget,
    action: PrPropertyAction,
    request_id: u64,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_property_edit_event(&ctx, &repo, &action, request_id);
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrPropertyEditFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    pr_number: panic_action.pr_number,
                    kind: panic_action.kind,
                    request_id,
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
    request_id: u64,
) -> AppEvent {
    let result = github_client(ctx).map(|client| execute_pr_property_edit(client, repo, action));
    match result {
        Some(Ok(())) => AppEvent::PrPropertyEditSucceeded {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            kind: action.kind,
            request_id,
        },
        Some(Err(error)) => AppEvent::PrPropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            kind: action.kind,
            request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrPropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            pr_number: action.pr_number,
            kind: action.kind,
            request_id,
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
            let (to_add, to_remove) = multi_select_diff(&action.editor, compute_label_diff);
            client.edit_labels(target, &to_add, &to_remove)
        }
        PrPropertyKind::Assignees => {
            let (to_add, to_remove) = multi_select_diff(&action.editor, compute_assignee_diff);
            client.edit_assignees(target, &to_add, &to_remove)
        }
        PrPropertyKind::Milestone => {
            execute_pr_milestone_edit(client, &repo.owner, &repo.repo, number, &action.editor)
        }
        PrPropertyKind::Title => execute_title_edit(
            client,
            &repo.owner,
            &repo.repo,
            number,
            true,
            &action.editor.title_text,
        ),
        PrPropertyKind::State => {
            execute_pr_state_edit(client, &repo.owner, &repo.repo, number, &action.editor)
        }
    }
}

/// Collect the desired multi-select values and compute the add/remove diff.
fn multi_select_diff(
    editor: &jefe::state::PrPropertyEditorState,
    diff_fn: DiffFn,
) -> (Vec<String>, Vec<String>) {
    let desired: Vec<String> = editor
        .options
        .iter()
        .filter(|o| o.selected)
        .map(|o| o.label.clone())
        .collect();
    diff_fn(&editor.baseline, &desired)
}

/// Signature of the label/assignee diff helpers.
type DiffFn = fn(&[String], &[String]) -> (Vec<String>, Vec<String>);

/// Apply a single-select milestone edit (H3: uses `selected_index`).
fn execute_pr_milestone_edit(
    client: jefe::github::GhClient,
    owner: &str,
    repo: &str,
    number: u64,
    editor: &jefe::state::PrPropertyEditorState,
) -> Result<(), jefe::github::GhError> {
    let selected = editor
        .options
        .get(editor.selected_index)
        .map(|o| o.label.as_str());
    match selected {
        Some(PROPERTY_CLEAR_LABEL) | None => client.clear_milestone(owner, repo, number, true),
        Some(name) => client.set_milestone(owner, repo, number, true, name),
    }
}

/// Apply a single-select state edit (H3: uses `selected_index`).
fn execute_pr_state_edit(
    client: jefe::github::GhClient,
    owner: &str,
    repo: &str,
    number: u64,
    editor: &jefe::state::PrPropertyEditorState,
) -> Result<(), jefe::github::GhError> {
    let want_closed = editor
        .options
        .get(editor.selected_index)
        .is_some_and(|o| o.label == "Closed");
    if want_closed {
        client.close_item(owner, repo, number, true)
    } else {
        client.reopen_item(owner, repo, number, true)
    }
}

/// Apply a title edit (shared shape for issues and PRs; `is_pr` selects the
/// `gh pr edit` vs `gh issue edit` path).
fn execute_title_edit(
    client: jefe::github::GhClient,
    owner: &str,
    repo: &str,
    number: u64,
    is_pr: bool,
    title_text: &str,
) -> Result<(), jefe::github::GhError> {
    let title = title_text.trim();
    if title.is_empty() {
        return Err(jefe::github::GhError::ApiError(
            "Title cannot be empty".to_string(),
        ));
    }
    client.set_title(owner, repo, number, is_pr, title)
}

/// Handle property editor options loading for PRs.
pub fn handle_pr_property_options_load(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let params = match resolve_pr_options_load_params(app_state) {
        Ok(Some(params)) => params,
        Ok(None) => return,
        Err(error) => {
            set_editor_error(app_state, ctx, &error);
            return;
        }
    };
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_options_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            // H5: deliver OptionsFailed, NOT empty OptionsLoaded
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrPropertyEditorOptionsFailed {
                    scope_repo_id: panic_params.scope_repo_id.clone(),
                    pr_number: panic_params.pr_number,
                    kind: panic_params.kind,
                    request_id: panic_params.request_id,
                    error: "Options fetch task panicked".to_string(),
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
    scope_repo_id: RepositoryId,
    pr_number: u64,
    request_id: u64,
}

fn resolve_pr_options_load_params(
    app_state: &AppStateHandle,
) -> Result<Option<PrOptionsLoadParams>, String> {
    let (kind, owner, repo, scope_repo_id, pr_number, request_id) = {
        let state = app_state.read();
        let Some(editor) = state.prs_state.property_editor.as_ref() else {
            return Ok(None);
        };
        let Some(detail) = state.prs_state.pr_detail.as_ref() else {
            return Ok(None);
        };
        let (owner, repo) =
            prs_dispatch::resolve_pr_gh_repo_or_error(&state).map_err(|error| error.message)?;
        let scope_repo_id = prs_dispatch::current_pr_scope_repo_id(&state);
        let kind = editor.kind;
        let request_id = editor.load_request_id;
        let pr_number = detail.number;
        drop(state);
        (kind, owner, repo, scope_repo_id, pr_number, request_id)
    };
    if owner.is_empty() || repo.is_empty() {
        return Ok(None);
    }
    Ok(Some(PrOptionsLoadParams {
        kind,
        owner,
        repo,
        scope_repo_id,
        pr_number,
        request_id,
    }))
}

fn pr_options_load_event(ctx: &SharedContext, params: &PrOptionsLoadParams) -> AppEvent {
    match github_client(ctx).map(|client| fetch_pr_options(client, params)) {
        Some(Ok(options)) => AppEvent::PrPropertyEditorOptionsLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            kind: params.kind,
            request_id: params.request_id,
            options,
        },
        Some(Err(error)) => AppEvent::PrPropertyEditorOptionsFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            kind: params.kind,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrPropertyEditorOptionsFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            kind: params.kind,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn fetch_pr_options(
    client: jefe::github::GhClient,
    params: &PrOptionsLoadParams,
) -> Result<Vec<(Option<String>, String, bool)>, jefe::github::GhError> {
    // M10: page sizes are limited. Currently-applied values are preserved by
    // the reducer (added back if missing from the first page).
    match params.kind {
        PrPropertyKind::Labels => {
            let names = client.fetch_label_names(&params.owner, &params.repo)?;
            Ok(names.into_iter().map(|n| (None, n, false)).collect())
        }
        PrPropertyKind::Assignees => {
            let logins = client.fetch_assignee_logins(&params.owner, &params.repo)?;
            Ok(logins.into_iter().map(|l| (None, l, false)).collect())
        }
        PrPropertyKind::Milestone => {
            let titles = client.fetch_milestone_titles(&params.owner, &params.repo)?;
            Ok(titles.into_iter().map(|t| (None, t, false)).collect())
        }
        _ => Ok(Vec::new()),
    }
}

/// Post-mutation refresh: after a property edit succeeds, silently refresh
/// the affected PR's list row and detail (issue #175). Uses the silent
/// background-refresh path so there is no spinner flash and selection/scroll/
/// filter state is preserved.
pub fn dispatch_pr_property_post_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    event: AppEvent,
) {
    apply_and_persist(app_state, ctx, event);
    super::prs_orchestration::resume_pr_post_mutation_refresh(app_state, ctx);
}
