//! Reducer contracts for the optional pull-request Changes drill-down.

use crate::domain::{PrFileBlob, PrFileChange, PrFileStatus, RepositoryId};
use crate::state::transition::TransitionExt;
use crate::state::{
    AppEvent, AppState, PrChangesBlobLoadedPayload, PrChangesFocus, PrChangesLoadFailedPayload,
    PrChangesLoadedPayload, PrDiffViewMode, PrFocus,
};

use super::prs_test_fixtures::prs_state_with_detail;

fn apply(state: AppState, event: AppEvent) -> AppState {
    state.apply(event).committed_pure()
}

fn changed_file(path: &str) -> PrFileChange {
    PrFileChange {
        blob_sha: format!("blob-{path}"),
        path: path.to_string(),
        previous_path: None,
        status: PrFileStatus::Modified,
        additions: 1,
        deletions: 1,
        changes: 2,
        patch: Some("@@ -1 +1 @@\n-old\n+new".to_string()),
    }
}

#[test]
fn entering_changes_from_loaded_detail_defaults_to_deltas_and_starts_correlated_load() {
    let mut state = prs_state_with_detail("repo-1", 376);

    state = apply(state, AppEvent::PrOpenChanges);

    assert_eq!(state.prs_state.pr_focus, PrFocus::PrChanges);
    assert_eq!(
        state.prs_state.changes.view_mode,
        PrDiffViewMode::DeltasOnly
    );
    assert_eq!(state.prs_state.changes.focus, PrChangesFocus::FileList);
    let pending = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("changes load must be pending"));
    assert_eq!(pending.scope_repo_id, RepositoryId("repo-1".to_string()));
    assert_eq!(pending.pr_number, 376);
    assert_eq!(pending.head_sha, "sha123");
}

#[test]
fn stale_file_completion_is_ignored_and_current_completion_selects_first_file() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);

    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: request_id.saturating_add(1),
            head_sha: "sha123".to_string(),
            files: vec![changed_file("stale.rs")],
            truncated: false,
        }),
    );
    assert!(state.prs_state.changes.files.is_empty());

    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("src/main.rs"), changed_file("src/lib.rs")],
            truncated: false,
        }),
    );
    assert_eq!(state.prs_state.changes.selected_file, Some(0));
    assert_eq!(state.prs_state.changes.files.len(), 2);
    assert!(state.prs_state.changes.pending.is_none());
}

#[test]
fn changes_navigation_and_back_unwind_without_altering_loaded_pr_detail() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("one.rs"), changed_file("two.rs")],
            truncated: false,
        }),
    );

    state = apply(state, AppEvent::PrNavigateDown);
    assert_eq!(state.prs_state.changes.selected_file, Some(1));
    state = apply(state, AppEvent::PrChangesFocusContent);
    assert_eq!(state.prs_state.changes.focus, PrChangesFocus::Content);
    state = apply(state, AppEvent::PrChangesBack);
    assert_eq!(state.prs_state.changes.focus, PrChangesFocus::FileList);
    state = apply(state, AppEvent::PrChangesBack);

    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);
    assert_eq!(
        state
            .prs_state
            .pr_detail
            .as_ref()
            .map(|detail| detail.number),
        Some(376)
    );
}

#[test]
fn opens_line_comment_composer_for_selected_right_anchor() {
    use crate::domain::{PrFileChange, PrFileStatus, PrReviewThreadSide};
    use crate::state::{ComposerTarget, InlineState, PrChangesFocus};

    let mut state = apply(prs_state_with_detail("repo-1", 1), AppEvent::PrOpenChanges);
    let pending = state
        .prs_state
        .changes
        .pending
        .clone()
        .unwrap_or_else(|| panic!("pending"));
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: pending.scope_repo_id,
            pr_number: pending.pr_number,
            request_id: pending.request_id,
            head_sha: "sha123".to_string(),
            files: vec![PrFileChange {
                blob_sha: "blob".to_string(),
                path: "src/main.rs".to_string(),
                previous_path: None,
                status: PrFileStatus::Modified,
                additions: 1,
                deletions: 0,
                changes: 1,
                patch: Some("@@ -1 +1 @@\n+new_call();".to_string()),
            }],
            truncated: false,
        }),
    );
    state.prs_state.changes.focus = PrChangesFocus::Content;
    state.prs_state.changes.selected_row = Some(1);

    state = apply(state, AppEvent::PrOpenChangesComment);

    let InlineState::Composer {
        target: ComposerTarget::NewReviewThread { target },
        ..
    } = state.prs_state.inline_state
    else {
        panic!("selected changed line should open the review composer");
    };
    assert_eq!(target.path, "src/main.rs");
    assert_eq!(target.line, 1);
    assert_eq!(target.side, PrReviewThreadSide::Right);
    assert_eq!(target.commit_id, "sha123");
}

#[test]
fn full_file_toggle_starts_one_correlated_blob_read_and_caches_success() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let files_request = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: files_request,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("src/main.rs")],
            truncated: false,
        }),
    );

    state = apply(state, AppEvent::PrChangesToggleView);
    let pending = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("blob read must be pending"));
    let blob_request = pending.request_id;
    assert_eq!(pending.blob_sha, "blob-src/main.rs");

    state = apply(
        state,
        AppEvent::PrChangesBlobLoaded(PrChangesBlobLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: blob_request,
            blob_sha: "blob-src/main.rs".to_string(),
            blob: PrFileBlob::Text("new\n".to_string()),
        }),
    );
    assert!(state.prs_state.changes.blob_pending.is_none());
    assert_eq!(state.prs_state.changes.blobs.len(), 1);

    state = apply(state, AppEvent::PrChangesToggleView);
    state = apply(state, AppEvent::PrChangesToggleView);
    assert_eq!(state.prs_state.changes.view_mode, PrDiffViewMode::FullFile);
    assert!(
        state.prs_state.changes.blob_pending.is_none(),
        "cached blob must not refetch"
    );
}

#[test]
fn content_navigation_and_view_toggle_keep_the_selected_row_in_bounds() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("src/main.rs")],
            truncated: false,
        }),
    );
    state = apply(state, AppEvent::PrChangesFocusContent);

    for _ in 0..20 {
        state = apply(state, AppEvent::PrNavigateDown);
    }
    assert_eq!(state.prs_state.changes.selected_row, Some(2));

    state = apply(state, AppEvent::PrChangesToggleView);
    assert_eq!(state.prs_state.changes.selected_row, Some(0));
}

#[test]
fn full_file_blob_cache_evicts_the_oldest_entries() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    let files = (0..9)
        .map(|index| changed_file(&format!("src/file-{index}.rs")))
        .collect::<Vec<_>>();
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files,
            truncated: false,
        }),
    );
    state = apply(state, AppEvent::PrChangesToggleView);

    for index in 0..9 {
        state.prs_state.changes.selected_file = Some(index);
        state = apply(state, AppEvent::PrChangesToggleView);
        state = apply(state, AppEvent::PrChangesToggleView);
        let pending = state
            .prs_state
            .changes
            .blob_pending
            .clone()
            .unwrap_or_else(|| panic!("blob {index} should be pending"));
        state = apply(
            state,
            AppEvent::PrChangesBlobLoaded(PrChangesBlobLoadedPayload {
                scope_repo_id: pending.scope_repo_id,
                pr_number: pending.pr_number,
                request_id: pending.request_id,
                blob_sha: pending.blob_sha,
                blob: PrFileBlob::Text(format!("content-{index}")),
            }),
        );
    }

    assert_eq!(state.prs_state.changes.blobs.len(), 8);
    assert_eq!(
        state.prs_state.changes.blobs[0].blob_sha,
        "blob-src/file-1.rs"
    );
}

#[test]
fn review_comment_success_waits_for_thread_refresh_without_appending_issue_comment() {
    use crate::domain::{IssueComment, PrReviewCommentTarget, PrReviewThreadSide};
    use crate::state::{ComposerTarget, InlineState, PrMutationPending};

    let mut state = prs_state_with_detail("repo-1", 376);
    state.prs_state.pr_focus = PrFocus::PrChanges;
    let target = ComposerTarget::NewReviewThread {
        target: PrReviewCommentTarget {
            path: "src/main.rs".to_string(),
            line: 3,
            side: PrReviewThreadSide::Right,
            commit_id: "sha123".to_string(),
        },
    };
    state.prs_state.inline_state = InlineState::Composer {
        target: target.clone(),
        text: "Preserve the fallback".to_string(),
        cursor: 21,
    };
    state.prs_state.mutation_pending = Some(PrMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 7,
        target,
    });
    let original_comment_count = state
        .prs_state
        .pr_detail
        .as_ref()
        .map_or(0, |detail| detail.comments.len());

    state = apply(
        state,
        AppEvent::PrCommentCreated {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            mutation_id: 7,
            comment: IssueComment {
                comment_id: 9001,
                author_login: "reviewer".to_string(),
                created_at: "2026-07-27T00:00:00Z".to_string(),
                edited_at: None,
                body: "Preserve the fallback".to_string(),
            },
        },
    );

    assert_eq!(state.prs_state.pr_focus, PrFocus::PrChanges);
    assert_eq!(
        state
            .prs_state
            .pr_detail
            .as_ref()
            .map_or(0, |detail| detail.comments.len()),
        original_comment_count
    );
    assert_eq!(state.prs_state.inline_state, InlineState::None);
    assert!(state.prs_state.mutation_pending.is_none());
}

#[test]
fn accepted_changes_refresh_clears_obsolete_blob_activity() {
    use crate::state::PrChangesBlobPending;

    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    state.prs_state.changes.blob_pending = Some(PrChangesBlobPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 376,
        request_id: 99,
        blob_sha: "obsolete-blob".to_string(),
    });
    state.prs_state.changes.blob_error = Some("obsolete failure".to_string());

    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("fresh.rs")],
            truncated: false,
        }),
    );

    assert!(state.prs_state.changes.blob_pending.is_none());
    assert!(state.prs_state.changes.blob_error.is_none());
}

#[test]
fn failed_changes_refresh_clears_obsolete_blob_activity() {
    use crate::state::PrChangesBlobPending;

    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    state.prs_state.changes.blob_pending = Some(PrChangesBlobPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 376,
        request_id: 99,
        blob_sha: "obsolete-blob".to_string(),
    });
    state.prs_state.changes.blob_error = Some("obsolete failure".to_string());

    state = apply(
        state,
        AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            error: "refresh failed".to_string(),
        }),
    );

    assert!(state.prs_state.changes.blob_pending.is_none());
    assert!(state.prs_state.changes.blob_error.is_none());
}

#[test]
fn selecting_cached_full_file_invalidates_previous_blob_activity() {
    use crate::state::{PrChangesBlobCache, PrChangesBlobPending};

    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("first.rs"), changed_file("cached.rs")],
            truncated: false,
        }),
    );
    state.prs_state.changes.view_mode = PrDiffViewMode::FullFile;
    state.prs_state.changes.blobs.push(PrChangesBlobCache {
        blob_sha: "blob-cached.rs".to_string(),
        blob: PrFileBlob::Text("cached\n".to_string()),
    });
    state.prs_state.changes.blob_pending = Some(PrChangesBlobPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 376,
        request_id: 99,
        blob_sha: "blob-first.rs".to_string(),
    });
    state.prs_state.changes.blob_error = Some("first failed".to_string());

    state = apply(state, AppEvent::PrNavigateDown);

    assert_eq!(state.prs_state.changes.selected_file, Some(1));
    assert!(state.prs_state.changes.blob_pending.is_none());
    assert!(state.prs_state.changes.blob_error.is_none());
}

// ─── Issue #376 regression tests ────────────────────────────────────────────

fn enter_loaded_changes_with_two_files() -> AppState {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);
    apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("a.rs"), changed_file("b.rs")],
            truncated: false,
        }),
    )
}

#[test]
fn home_key_in_full_file_mode_stages_uncached_blob_read() {
    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.changes.view_mode = PrDiffViewMode::FullFile;
    // Navigate to file 1 (index 1), which stages a blob read for b.rs.
    state = apply(state, AppEvent::PrNavigateDown);
    let first_blob_request = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("blob pending after down"))
        .request_id;
    let first_blob_sha = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("blob pending"))
        .blob_sha
        .clone();

    // Home goes back to file 0 (a.rs) — a different blob.
    state = apply(state, AppEvent::PrNavigateHome);
    assert_eq!(state.prs_state.changes.selected_file, Some(0));
    assert_eq!(state.prs_state.changes.selected_row, Some(0));
    let pending = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("home must stage an uncached blob read"));
    assert_ne!(pending.blob_sha, first_blob_sha);
    assert_ne!(pending.request_id, first_blob_request);
}

#[test]
fn end_key_in_full_file_mode_stages_uncached_blob_read() {
    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.changes.view_mode = PrDiffViewMode::FullFile;
    // Default selection is file 0 (a.rs). End moves to file 1 (b.rs).
    state = apply(state, AppEvent::PrNavigateEnd);
    assert_eq!(state.prs_state.changes.selected_file, Some(1));
    assert_eq!(state.prs_state.changes.selected_row, Some(0));
    assert_eq!(
        state
            .prs_state
            .changes
            .blob_pending
            .as_ref()
            .unwrap_or_else(|| panic!("end must stage blob read"))
            .blob_sha,
        "blob-b.rs"
    );
}

#[test]
fn esc_from_file_list_invalidates_blob_pending_so_late_completion_is_ignored() {
    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.changes.view_mode = PrDiffViewMode::FullFile;
    state = apply(state, AppEvent::PrNavigateDown);
    let blob_request = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("blob pending"))
        .request_id;
    let blob_sha = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("blob pending"))
        .blob_sha
        .clone();

    // Esc from file-list back to PR detail.
    state = apply(state, AppEvent::PrChangesBack);
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);
    assert!(state.prs_state.changes.blob_pending.is_none());
    assert!(state.prs_state.changes.pending.is_none());

    // Late blob completion arrives with the ORIGINAL request_id — it must be
    // ignored because Esc cleared the pending correlation.
    state = apply(
        state,
        AppEvent::PrChangesBlobLoaded(PrChangesBlobLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: blob_request,
            blob_sha,
            blob: PrFileBlob::Text("late-content".to_string()),
        }),
    );
    assert!(
        state.prs_state.changes.blobs.is_empty(),
        "late completion after Esc must not populate the cache"
    );
}

#[test]
fn retry_on_files_failure_restages_fresh_head_correlated_files_read() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);
    // Fail the files read.
    state = apply(
        state,
        AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            error: "network down".to_string(),
        }),
    );
    assert!(state.prs_state.changes.pending.is_none());
    assert_eq!(
        state.prs_state.changes.error.as_deref(),
        Some("network down")
    );
    assert_eq!(state.prs_state.error.as_deref(), Some("network down"));

    // Retry: restages a fresh files read with a new request_id.
    state = apply(state, AppEvent::PrChangesRetryFiles);
    let pending = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("retry must restage files pending"));
    assert_ne!(pending.request_id, request_id);
    assert_eq!(pending.head_sha, "sha123");
    assert!(state.prs_state.changes.error.is_none());
    assert!(
        state.prs_state.error.is_none(),
        "retry must clear the owned PR error"
    );
}

#[test]
fn retry_on_blob_failure_restages_selected_blob_read() {
    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.changes.view_mode = PrDiffViewMode::FullFile;
    state = apply(state, AppEvent::PrNavigateDown);
    let blob_request = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .map_or(0, |p| p.request_id);
    // Fail the blob read.
    state = apply(
        state,
        AppEvent::PrChangesBlobLoadFailed(crate::state::PrChangesBlobLoadFailedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: blob_request,
            blob_sha: "blob-b.rs".to_string(),
            error: "blob unavailable".to_string(),
        }),
    );
    assert!(state.prs_state.changes.blob_pending.is_none());
    assert_eq!(
        state.prs_state.changes.blob_error.as_deref(),
        Some("blob unavailable")
    );

    // Retry: restages the selected blob.
    state = apply(state, AppEvent::PrChangesRetryBlob);
    let pending = state
        .prs_state
        .changes
        .blob_pending
        .as_ref()
        .unwrap_or_else(|| panic!("retry must restage blob pending"));
    assert_eq!(pending.blob_sha, "blob-b.rs");
    assert_ne!(pending.request_id, blob_request);
    assert!(state.prs_state.changes.blob_error.is_none());
}

#[test]
fn changes_failure_propagates_to_pr_error_slot_for_error_store_capture() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);

    state = apply(
        state,
        AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            error: "parse error".to_string(),
        }),
    );

    // The owned changes.error retains the inline error...
    assert_eq!(
        state.prs_state.changes.error.as_deref(),
        Some("parse error")
    );
    // ...and it also reaches prs_state.error so the existing PR Error Store
    // capture boundary records it.
    assert_eq!(state.prs_state.error.as_deref(), Some("parse error"));
}

#[test]
fn changes_success_clears_owned_pr_error_set_by_prior_changes_failure() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);
    state = apply(
        state,
        AppEvent::PrChangesLoadFailed(PrChangesLoadFailedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            error: "transient".to_string(),
        }),
    );
    assert!(state.prs_state.error.is_some());

    // Retry restages with a new request_id.
    state = apply(state, AppEvent::PrChangesRetryFiles);
    let new_request = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .unwrap_or_else(|| panic!("pending"))
        .request_id;

    // Success clears the owned PR error.
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: new_request,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("fresh.rs")],
            truncated: false,
        }),
    );
    assert!(state.prs_state.changes.error.is_none());
    assert!(state.prs_state.error.is_none());
}

#[test]
fn same_head_detail_refresh_preserves_changes_visit() {
    use crate::domain::{CommentDetailIdentity, PageToken, PullRequestDetail};

    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.pr_focus = PrFocus::PrChanges;

    // Set up a detail_pending so the refresh is accepted.
    state.prs_state.detail_pending = Some(crate::state::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 376,
        request_id: 1,
    });
    // Build a refreshed detail with the SAME head_sha.
    let mut detail = PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 376,
        title: "PR #376".to_string(),
        state: crate::domain::PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-03T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Updated body".to_string(),
        external_url: "https://github.com/owner/repo/pull/376".to_string(),
        review_decision: None,
        checks_status: crate::domain::PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: crate::domain::PaginatedList::from_loaded(
            CommentDetailIdentity {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                number: 376,
            },
            Vec::new(),
            PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
    };
    detail.comments.rebind_identity(CommentDetailIdentity {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        number: 376,
    });

    state = apply(
        state,
        AppEvent::PrDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: 1,
            detail: Box::new(detail),
        },
    );

    // Same head → Changes visit is preserved.
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrChanges);
    assert!(state.prs_state.changes.identity.is_some());
    assert_eq!(state.prs_state.changes.files.len(), 2);
}

#[test]
fn changed_head_detail_refresh_invalidates_changes_visit() {
    use crate::domain::{CommentDetailIdentity, PageToken, PullRequestDetail};

    let mut state = enter_loaded_changes_with_two_files();
    state.prs_state.pr_focus = PrFocus::PrChanges;

    state.prs_state.detail_pending = Some(crate::state::PrDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 376,
        request_id: 1,
    });
    // Build a refreshed detail with a DIFFERENT head_sha.
    let mut detail = PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 376,
        title: "PR #376".to_string(),
        state: crate::domain::PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-03T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha999".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Force-pushed".to_string(),
        external_url: "https://github.com/owner/repo/pull/376".to_string(),
        review_decision: None,
        checks_status: crate::domain::PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: crate::domain::PaginatedList::from_loaded(
            CommentDetailIdentity {
                scope_repo_id: RepositoryId("repo-1".to_string()),
                number: 376,
            },
            Vec::new(),
            PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
    };
    detail.comments.rebind_identity(CommentDetailIdentity {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        number: 376,
    });

    state = apply(
        state,
        AppEvent::PrDetailLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id: 1,
            detail: Box::new(detail),
        },
    );

    // Changed head → Changes visit is invalidated and focus returns to detail.
    assert_eq!(state.prs_state.pr_focus, PrFocus::PrDetail);
    assert!(state.prs_state.changes.identity.is_none());
    assert!(state.prs_state.changes.files.is_empty());
}

#[test]
fn head_sha_mismatch_rejects_changes_loaded_completion() {
    let mut state = prs_state_with_detail("repo-1", 376);
    state = apply(state, AppEvent::PrOpenChanges);
    let request_id = state
        .prs_state
        .changes
        .pending
        .as_ref()
        .map_or(0, |p| p.request_id);

    // Completion with a WRONG head_sha must be rejected.
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "stale-sha".to_string(),
            files: vec![changed_file("wrong.rs")],
            truncated: false,
        }),
    );
    assert!(
        state.prs_state.changes.files.is_empty(),
        "stale-head completion must be rejected"
    );

    // Correct head_sha is accepted.
    state = apply(
        state,
        AppEvent::PrChangesLoaded(PrChangesLoadedPayload {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 376,
            request_id,
            head_sha: "sha123".to_string(),
            files: vec![changed_file("right.rs")],
            truncated: false,
        }),
    );
    assert_eq!(state.prs_state.changes.files.len(), 1);
}
