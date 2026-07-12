//! Issues-mode property-edit dispatch helpers (issue #175).
//!
//! Mirrors `issues_mutation.rs`. On `PropertyEditorConfirm`, reads the editor
//! state, spawns the gh task via `spawn_gh_task_with_panic`, delivers
//! success/failure events, then on success triggers the silent detail refresh.

use jefe::domain::RepositoryId;
use jefe::github::compute_assignee_diff;
use jefe::github::compute_label_diff;
use jefe::state::{AppEvent, IssuePropertyEditorState, IssuePropertyKind};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_app_event, gh_async, github_client,
    issues_dispatch,
};

/// Handle a property-editor confirm: read editor state, spawn the gh task,
/// deliver success/failure events, then refresh the detail.
pub fn handle_issue_property_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(action) = resolve_issue_property_action(app_state) else {
        return;
    };
    let Some(repo) = issue_repo_target(app_state) else {
        report_missing_repo(app_state, ctx, &action);
        return;
    };
    // H4: check for empty title before marking pending
    if action.kind == IssuePropertyKind::Title && action.editor.title_text.trim().is_empty() {
        set_editor_error(app_state, ctx, "Title cannot be empty");
        return;
    }
    // H4: mark mutation pending (debounce); if already pending, ignore.
    let Some(request_id) = mark_mutation_pending(app_state, &action) else {
        return;
    };
    dispatch_issue_property_edit(app_state, ctx, repo, action, request_id);
}

fn set_editor_error(app_state: &mut AppStateHandle, ctx: &SharedContext, error: &str) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::IssuePropertyEditFailed {
            scope_repo_id: RepositoryId(String::new()),
            issue_number: 0,
            kind: IssuePropertyKind::Title,
            request_id: 0,
            error: error.to_string(),
        },
    );
}

fn mark_mutation_pending(
    app_state: &mut AppStateHandle,
    action: &IssuePropertyAction,
) -> Option<u64> {
    let mut state = app_state.write();
    state.mark_issue_property_mutation_pending(action.scope_repo_id.clone(), action.issue_number)
}

#[derive(Clone)]
struct IssuePropertyAction {
    scope_repo_id: RepositoryId,
    issue_number: u64,
    kind: IssuePropertyKind,
    editor: IssuePropertyEditorState,
}

fn resolve_issue_property_action(app_state: &AppStateHandle) -> Option<IssuePropertyAction> {
    let (scope_repo_id, issue_number, kind, editor) = {
        let state = app_state.read();
        let editor = state.issues_state.property_editor.as_ref()?.clone();
        let issue_number = state.issues_state.issue_detail.as_ref()?.number;
        let scope_repo_id = issues_dispatch::current_scope_repo_id(&state);
        let kind = editor.kind;
        drop(state);
        (scope_repo_id, issue_number, kind, editor)
    };
    Some(IssuePropertyAction {
        scope_repo_id,
        issue_number,
        kind,
        editor,
    })
}

#[derive(Clone)]
struct IssueRepoTarget {
    owner: String,
    repo: String,
}

fn issue_repo_target(app_state: &AppStateHandle) -> Option<IssueRepoTarget> {
    let state = app_state.read();
    let (owner, repo) = issues_dispatch::resolve_gh_repo(&state);
    (!owner.is_empty() && !repo.is_empty()).then_some(IssueRepoTarget { owner, repo })
}

fn report_missing_repo(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: &IssuePropertyAction,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::IssuePropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
            kind: action.kind,
            request_id: 0,
            error: "No GitHub repository configured.".to_string(),
        },
    );
}

fn dispatch_issue_property_edit(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    repo: IssueRepoTarget,
    action: IssuePropertyAction,
    request_id: u64,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = issue_property_edit_event(&ctx, &repo, &action, request_id);
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssuePropertyEditFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    issue_number: panic_action.issue_number,
                    kind: panic_action.kind,
                    request_id,
                    error: format!("GitHub property edit task panicked: {message}"),
                },
            );
        },
    );
}

fn issue_property_edit_event(
    ctx: &SharedContext,
    repo: &IssueRepoTarget,
    action: &IssuePropertyAction,
    request_id: u64,
) -> AppEvent {
    let result = github_client(ctx).map(|client| execute_issue_property_edit(client, repo, action));
    match result {
        Some(Ok(())) => AppEvent::IssuePropertyEditSucceeded {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
            kind: action.kind,
            request_id,
        },
        Some(Err(error)) => AppEvent::IssuePropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
            kind: action.kind,
            request_id,
            error: error.to_string(),
        },
        None => AppEvent::IssuePropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
            kind: action.kind,
            request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn execute_issue_property_edit(
    client: jefe::github::GhClient,
    repo: &IssueRepoTarget,
    action: &IssuePropertyAction,
) -> Result<(), jefe::github::GhError> {
    let number = action.issue_number;
    let target = jefe::github::PropertyEditTarget {
        owner: &repo.owner,
        repo: &repo.repo,
        number,
        is_pr: false,
    };
    match action.kind {
        IssuePropertyKind::Labels => {
            // M8: use diff functions with baseline
            let desired: Vec<String> = action
                .editor
                .options
                .iter()
                .filter(|o| o.selected)
                .map(|o| o.label.clone())
                .collect();
            let (to_add, to_remove) = compute_label_diff(&action.editor.baseline, &desired);
            client.edit_labels(target, &to_add, &to_remove)
        }
        IssuePropertyKind::Assignees => {
            let desired: Vec<String> = action
                .editor
                .options
                .iter()
                .filter(|o| o.selected)
                .map(|o| o.label.clone())
                .collect();
            let (to_add, to_remove) = compute_assignee_diff(&action.editor.baseline, &desired);
            client.edit_assignees(target, &to_add, &to_remove)
        }
        IssuePropertyKind::Milestone => execute_milestone_edit(client, repo, action, false),
        IssuePropertyKind::Title => {
            let title = action.editor.title_text.trim();
            if title.is_empty() {
                return Err(jefe::github::GhError::ApiError(
                    "Title cannot be empty".to_string(),
                ));
            }
            client.set_title(&repo.owner, &repo.repo, number, false, title)
        }
        IssuePropertyKind::State => execute_state_edit(client, repo, action, false),
        IssuePropertyKind::Type => {
            // H3: single-select uses selected_index, not selected flag
            let selected_opt = action
                .editor
                .options
                .get(action.editor.selected_index)
                .filter(|o| o.label != "(clear)");
            let type_id = selected_opt.and_then(|o| o.id.clone());
            execute_issue_type_edit(client, repo, number, type_id)
        }
    }
}

fn execute_milestone_edit(
    client: jefe::github::GhClient,
    repo: &IssueRepoTarget,
    action: &IssuePropertyAction,
    is_pr: bool,
) -> Result<(), jefe::github::GhError> {
    // H3: single-select uses selected_index, not selected flag
    let selected = action
        .editor
        .options
        .get(action.editor.selected_index)
        .map(|o| o.label.as_str());
    match selected {
        Some("(clear)") | None => {
            client.clear_milestone(&repo.owner, &repo.repo, action.issue_number, is_pr)
        }
        Some(name) => {
            client.set_milestone(&repo.owner, &repo.repo, action.issue_number, is_pr, name)
        }
    }
}

fn execute_state_edit(
    client: jefe::github::GhClient,
    repo: &IssueRepoTarget,
    action: &IssuePropertyAction,
    is_pr: bool,
) -> Result<(), jefe::github::GhError> {
    // H3: single-select uses selected_index, not selected flag
    let want_closed = action
        .editor
        .options
        .get(action.editor.selected_index)
        .is_some_and(|o| o.label == "Closed");
    if want_closed {
        client.close_item(&repo.owner, &repo.repo, action.issue_number, is_pr)
    } else {
        client.reopen_item(&repo.owner, &repo.repo, action.issue_number, is_pr)
    }
}

fn execute_issue_type_edit(
    client: jefe::github::GhClient,
    repo: &IssueRepoTarget,
    number: u64,
    type_id: Option<String>,
) -> Result<(), jefe::github::GhError> {
    let node_info = client.fetch_issue_node_info(&repo.owner, &repo.repo, number)?;
    client.set_issue_type(&node_info.node_id, type_id.as_deref())
}

/// Handle property editor options loading (async fetch of repo labels/assignees/
/// milestones/issue types). Called from the dispatch layer when the editor opens.
pub fn handle_issue_property_options_load(app_state: &AppStateHandle, ctx: &SharedContext) {
    let Some(params) = resolve_options_load_params(app_state) else {
        return;
    };
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = options_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            // H5: deliver OptionsFailed, NOT empty OptionsLoaded
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssuePropertyEditorOptionsFailed {
                    scope_repo_id: panic_params.scope_repo_id.clone(),
                    issue_number: panic_params.issue_number,
                    kind: panic_params.kind,
                    request_id: panic_params.request_id,
                    error: "Options fetch task panicked".to_string(),
                },
            );
        },
    );
}

#[derive(Clone)]
struct OptionsLoadParams {
    kind: IssuePropertyKind,
    owner: String,
    repo: String,
    scope_repo_id: RepositoryId,
    issue_number: u64,
    request_id: u64,
}

fn resolve_options_load_params(app_state: &AppStateHandle) -> Option<OptionsLoadParams> {
    let (kind, owner, repo, scope_repo_id, issue_number, request_id) = {
        let state = app_state.read();
        let editor = state.issues_state.property_editor.as_ref()?;
        let detail = state.issues_state.issue_detail.as_ref()?;
        let (owner, repo) = issues_dispatch::resolve_gh_repo(&state);
        let scope_repo_id = issues_dispatch::current_scope_repo_id(&state);
        let kind = editor.kind;
        let request_id = editor.load_request_id;
        let issue_number = detail.number;
        drop(state);
        (kind, owner, repo, scope_repo_id, issue_number, request_id)
    };
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(OptionsLoadParams {
        kind,
        owner,
        repo,
        scope_repo_id,
        issue_number,
        request_id,
    })
}

fn options_load_event(ctx: &SharedContext, params: &OptionsLoadParams) -> AppEvent {
    match github_client(ctx).map(|client| fetch_options(client, params)) {
        Some(Ok(options)) => AppEvent::IssuePropertyEditorOptionsLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            kind: params.kind,
            request_id: params.request_id,
            options,
        },
        Some(Err(error)) => AppEvent::IssuePropertyEditorOptionsFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            kind: params.kind,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::IssuePropertyEditorOptionsFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            kind: params.kind,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn fetch_options(
    client: jefe::github::GhClient,
    params: &OptionsLoadParams,
) -> Result<Vec<(String, bool)>, jefe::github::GhError> {
    // M10: page sizes are limited (labels: 100, milestones: 50, assignees:
    // 100, issue types: 50). Currently-applied values are preserved by the
    // reducer (added back if missing from the first page). Full pagination
    // can be deferred; the current-values-preservation is required.
    match params.kind {
        IssuePropertyKind::Labels => {
            let names = client.fetch_label_names(&params.owner, &params.repo)?;
            Ok(names.into_iter().map(|n| (n, false)).collect())
        }
        IssuePropertyKind::Assignees => {
            let logins = client.fetch_assignee_logins(&params.owner, &params.repo)?;
            Ok(logins.into_iter().map(|l| (l, false)).collect())
        }
        IssuePropertyKind::Milestone => {
            let titles = client.fetch_milestone_titles(&params.owner, &params.repo)?;
            Ok(titles.into_iter().map(|t| (t, false)).collect())
        }
        IssuePropertyKind::Type => {
            // H2: display name, carry id. Store id on the option via the
            // reducer (the option's `id` field). Here we return (name, false)
            // pairs; the IDs are fetched separately by the reducer.
            let types = client.fetch_issue_types(&params.owner, &params.repo)?;
            Ok(types.into_iter().map(|(_id, name)| (name, false)).collect())
        }
        _ => Ok(Vec::new()),
    }
}

/// Post-mutation refresh: after a property edit succeeds, reload the detail.
pub fn dispatch_issue_property_post_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    event: AppEvent,
) {
    apply_and_persist(app_state, ctx, event);
    issues_dispatch::load_issue_detail_for_selection(app_state, ctx);
}
