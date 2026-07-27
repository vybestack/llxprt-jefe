//! Non-blocking changed-file loading for the PR Changes drill-down.

use std::path::PathBuf;

use jefe::domain::{PrFileBlob, RepositoryId};
use jefe::state::AppEvent;

use super::prs_dispatch::{current_pr_scope_repo_id, resolve_pr_gh_repo_or_error};
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

#[derive(Clone)]
struct PrChangesLoadParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    owner: String,
    repo: String,
    request_id: u64,
}

#[derive(Clone)]
struct PrBlobLoadParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    owner: String,
    repo: String,
    request_id: u64,
    blob_sha: String,
    local_dir: Option<PathBuf>,
}

const MAX_FULL_FILE_BYTES: u64 = 1_048_576;

/// Apply Changes entry and start the correlated changed-files read.
pub(super) fn open_and_load_changes(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::PrOpenChanges);
    let params = match changes_load_params(app_state) {
        Ok(Some(params)) => params,
        Ok(None) => return,
        Err(event) => {
            apply_and_persist(app_state, ctx, *event);
            return;
        }
    };
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = changes_load_event(&ctx, &params);
            if let AppEvent::PrChangesLoadFailed { error, .. } = &event
                && super::auth_remediation::offer_auth_remediation(&mut app_state, &ctx, error)
            {
                return;
            }
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                failure_event(
                    &panic_params,
                    format!("GitHub PR files task panicked: {message}"),
                ),
            );
        },
    );
}
/// Start a correlated lazy full-file read when the reducer staged one.
pub(super) fn load_pending_blob(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let params = match blob_load_params(app_state) {
        Ok(Some(params)) => params,
        Ok(None) => return,
        Err(event) => {
            apply_and_persist(app_state, ctx, *event);
            return;
        }
    };
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = blob_load_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                blob_failure_event(&panic_params, format!("PR blob task panicked: {message}")),
            );
        },
    );
}

fn blob_load_params(app_state: &AppStateHandle) -> Result<Option<PrBlobLoadParams>, Box<AppEvent>> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.changes.blob_pending.as_ref() else {
        return Ok(None);
    };
    let (owner, repo) = resolve_pr_gh_repo_or_error(&state)
        .map_err(|error| Box::new(blob_failure_from_pending(pending, error.message)))?;
    let local_dir = state
        .selected_repository()
        .filter(|repo| !repo.remote.enabled)
        .map(|repo| repo.base_dir.clone());
    let params = PrBlobLoadParams {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        owner,
        repo,
        request_id: pending.request_id,
        blob_sha: pending.blob_sha.clone(),
        local_dir,
    };
    drop(state);
    Ok(Some(params))
}

fn blob_load_event(ctx: &SharedContext, params: &PrBlobLoadParams) -> AppEvent {
    let result = read_local_blob(params).or_else(|| {
        github_client(ctx).map(|client| {
            client
                .get_pr_file_blob(&params.owner, &params.repo, &params.blob_sha)
                .map_err(|error| error.to_string())
        })
    });
    match result {
        Some(Ok(blob)) => AppEvent::PrChangesBlobLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            blob_sha: params.blob_sha.clone(),
            blob,
        },
        Some(Err(error)) => blob_failure_event(params, error),
        None => blob_failure_event(params, "Application context unavailable".to_string()),
    }
}

fn read_local_blob(params: &PrBlobLoadParams) -> Option<Result<PrFileBlob, String>> {
    let directory = params.local_dir.as_ref()?;
    let mut size = jefe::local_command::command(jefe::local_command::LocalTool::Git).ok()?;
    let output = size
        .args(["-C"])
        .arg(directory)
        .args(["cat-file", "-s", &params.blob_sha])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let byte_size = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()?;
    if byte_size > MAX_FULL_FILE_BYTES {
        return Some(Ok(PrFileBlob::Truncated { byte_size }));
    }
    let mut content = jefe::local_command::command(jefe::local_command::LocalTool::Git).ok()?;
    let output = content
        .args(["-C"])
        .arg(directory)
        .args(["cat-file", "blob", &params.blob_sha])
        .output()
        .ok()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Some(Err(format!("git cat-file blob failed: {stderr}")));
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|text| Ok(PrFileBlob::Text(text)))
}

fn blob_failure_event(params: &PrBlobLoadParams, error: String) -> AppEvent {
    AppEvent::PrChangesBlobLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        blob_sha: params.blob_sha.clone(),
        error,
    }
}

fn blob_failure_from_pending(
    pending: &jefe::state::PrChangesBlobPending,
    error: String,
) -> AppEvent {
    AppEvent::PrChangesBlobLoadFailed {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        request_id: pending.request_id,
        blob_sha: pending.blob_sha.clone(),
        error,
    }
}

fn changes_load_params(
    app_state: &AppStateHandle,
) -> Result<Option<PrChangesLoadParams>, Box<AppEvent>> {
    let state = app_state.read();
    let Some(pending) = state.prs_state.changes.pending.as_ref() else {
        return Ok(None);
    };
    let (owner, repo) = resolve_pr_gh_repo_or_error(&state)
        .map_err(|error| Box::new(failure_event_from_pending(pending, error.message)))?;
    if owner.is_empty() || repo.is_empty() {
        return Err(Box::new(failure_event_from_pending(
            pending,
            "No GitHub repository configured for PR changes".to_string(),
        )));
    }
    let params = PrChangesLoadParams {
        scope_repo_id: current_pr_scope_repo_id(&state),
        pr_number: pending.pr_number,
        owner,
        repo,
        request_id: pending.request_id,
    };
    drop(state);
    Ok(Some(params))
}

fn changes_load_event(ctx: &SharedContext, params: &PrChangesLoadParams) -> AppEvent {
    match github_client(ctx)
        .map(|client| client.list_pr_files(&params.owner, &params.repo, params.pr_number))
    {
        Some(Ok(response)) => AppEvent::PrChangesLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            files: response.files,
            truncated: response.truncated,
        },
        Some(Err(error)) => failure_event(params, error.to_string()),
        None => failure_event(params, "Application context unavailable".to_string()),
    }
}

fn failure_event(params: &PrChangesLoadParams, error: String) -> AppEvent {
    AppEvent::PrChangesLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
        error,
    }
}

fn failure_event_from_pending(pending: &jefe::state::PrChangesPending, error: String) -> AppEvent {
    AppEvent::PrChangesLoadFailed {
        scope_repo_id: pending.scope_repo_id.clone(),
        pr_number: pending.pr_number,
        request_id: pending.request_id,
        error,
    }
}
