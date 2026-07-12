//! Issues-mode property-edit dispatch helpers (issue #175).
//!
//! Mirrors `issues_mutation.rs`. On `PropertyEditorConfirm`, reads the editor
//! state, spawns the gh task via `spawn_gh_task_with_panic`, delivers
//! success/failure events, then on success triggers the silent detail refresh.

use jefe::domain::RepositoryId;
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
    dispatch_issue_property_edit(app_state, ctx, repo, action);
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
            error: "No GitHub repository configured.".to_string(),
        },
    );
}

fn dispatch_issue_property_edit(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    repo: IssueRepoTarget,
    action: IssuePropertyAction,
) {
    let panic_action = action.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = issue_property_edit_event(&ctx, &repo, &action);
            dispatch_app_event(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssuePropertyEditFailed {
                    scope_repo_id: panic_action.scope_repo_id,
                    issue_number: panic_action.issue_number,
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
) -> AppEvent {
    let result = github_client(ctx).map(|client| execute_issue_property_edit(client, repo, action));
    match result {
        Some(Ok(())) => AppEvent::IssuePropertyEditSucceeded {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
        },
        Some(Err(error)) => AppEvent::IssuePropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
            error: error.to_string(),
        },
        None => AppEvent::IssuePropertyEditFailed {
            scope_repo_id: action.scope_repo_id.clone(),
            issue_number: action.issue_number,
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
            let (to_add, to_remove) = compute_multi_diff(&action.editor);
            client.edit_labels(target, &to_add, &to_remove)
        }
        IssuePropertyKind::Assignees => {
            let (to_add, to_remove) = compute_multi_diff(&action.editor);
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
            let type_name = action
                .editor
                .options
                .iter()
                .find(|o| o.selected && o.label != "(clear)")
                .map(|o| o.label.clone());
            execute_issue_type_edit(client, repo, number, type_name)
        }
    }
}

fn execute_milestone_edit(
    client: jefe::github::GhClient,
    repo: &IssueRepoTarget,
    action: &IssuePropertyAction,
    is_pr: bool,
) -> Result<(), jefe::github::GhError> {
    let selected = action
        .editor
        .options
        .iter()
        .find(|o| o.selected)
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
    let want_closed = action
        .editor
        .options
        .iter()
        .any(|o| o.selected && o.label == "Closed");
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
    type_name: Option<String>,
) -> Result<(), jefe::github::GhError> {
    let node_info = client.fetch_issue_node_info(&repo.owner, &repo.repo, number)?;
    let type_id = match type_name {
        None => None,
        Some(name) => {
            let types = client.fetch_issue_types(&repo.owner, &repo.repo)?;
            Some(
                types
                    .into_iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(&name))
                    .map(|(_, id)| id)
                    .ok_or_else(|| {
                        jefe::github::GhError::ApiError(format!("Issue type '{name}' not found"))
                    })?,
            )
        }
    };
    client.set_issue_type(&node_info.node_id, type_id.as_deref())
}

fn compute_multi_diff(editor: &IssuePropertyEditorState) -> (Vec<String>, Vec<String>) {
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

/// Handle property editor options loading (async fetch of repo labels/assignees/
/// milestones/issue types). Called from the dispatch layer when the editor opens.
pub fn handle_issue_property_options_load(app_state: &AppStateHandle, ctx: &SharedContext) {
    let Some(params) = resolve_options_load_params(app_state) else {
        return;
    };
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = options_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssuePropertyEditorOptionsLoaded {
                    options: Vec::new(),
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
}

fn resolve_options_load_params(app_state: &AppStateHandle) -> Option<OptionsLoadParams> {
    let (kind, owner, repo) = {
        let state = app_state.read();
        let editor = state.issues_state.property_editor.as_ref()?;
        // Detail presence is the precondition check; labels/assignees/
        // milestones/types are repo-scoped, not issue-scoped.
        let _ = state.issues_state.issue_detail.as_ref()?;
        let (owner, repo) = issues_dispatch::resolve_gh_repo(&state);
        let kind = editor.kind;
        drop(state);
        (kind, owner, repo)
    };
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(OptionsLoadParams { kind, owner, repo })
}

fn options_load_event(ctx: &SharedContext, params: &OptionsLoadParams) -> AppEvent {
    let result = github_client(ctx).map(|client| fetch_options(client, params));
    let options = match result {
        Some(Ok(opts)) => opts,
        _ => Vec::new(),
    };
    AppEvent::IssuePropertyEditorOptionsLoaded { options }
}

fn fetch_options(
    client: jefe::github::GhClient,
    params: &OptionsLoadParams,
) -> Result<Vec<(String, bool)>, jefe::github::GhError> {
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
            let types = client.fetch_issue_types(&params.owner, &params.repo)?;
            Ok(types.into_iter().map(|(n, _)| (n, false)).collect())
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
