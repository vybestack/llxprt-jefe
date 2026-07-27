//! Deterministic state transitions for the optional PR Changes drill-down.

use super::{
    AppEvent, AppState, PrChangesBlobCache, PrChangesBlobPending, PrChangesFocus,
    PrChangesIdentity, PrChangesPending, PrChangesState, PrDiffViewMode, PrFocus,
};

const PR_CHANGES_BLOB_CACHE_CAPACITY: usize = 8;

impl AppState {
    /// Apply one Changes event, returning whether this reducer owns it.
    pub(super) fn apply_pr_changes_event(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrOpenChanges => self.open_pr_changes(),
            AppEvent::PrChangesLoaded { .. } => self.accept_pr_changes(event),
            AppEvent::PrChangesLoadFailed { .. } => self.fail_pr_changes(event),
            AppEvent::PrChangesBlobLoaded { .. } => self.accept_pr_changes_blob(event),
            AppEvent::PrChangesBlobLoadFailed { .. } => self.fail_pr_changes_blob(event),
            AppEvent::PrChangesFocusContent => self.focus_pr_changes_content(),
            AppEvent::PrChangesFocusFiles => self.focus_pr_changes_files(),
            AppEvent::PrChangesToggleView => self.toggle_pr_changes_view(),
            AppEvent::PrOpenChangesComment => self.open_pr_changes_comment(),
            AppEvent::PrChangesBack => self.back_from_pr_changes(),
            AppEvent::PrNavigateUp => self.navigate_pr_changes(-1),
            AppEvent::PrNavigateDown => self.navigate_pr_changes(1),
            AppEvent::PrNavigatePageUp(page) => self.navigate_pr_changes(-page_delta(page.get())),
            AppEvent::PrNavigatePageDown(page) => self.navigate_pr_changes(page_delta(page.get())),
            AppEvent::PrNavigateHome => self.navigate_pr_changes_home(),
            AppEvent::PrNavigateEnd => self.navigate_pr_changes_end(),
            _ => false,
        }
    }

    fn open_pr_changes(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrDetail {
            return true;
        }
        let Some(detail) = self.prs_state.pr_detail.as_ref() else {
            self.prs_state.error = Some("Load a pull request before opening Changes".to_string());
            return true;
        };
        let Some(scope_repo_id) = self.selected_repository_id().cloned() else {
            self.prs_state.error = Some("No repository selected for Changes".to_string());
            return true;
        };
        let request_id = self.prs_state.changes.next_request_id.saturating_add(1);
        let identity = PrChangesIdentity {
            scope_repo_id: scope_repo_id.clone(),
            pr_number: detail.number,
            head_sha: detail.head_sha.clone(),
        };
        self.prs_state.changes = PrChangesState {
            identity: Some(identity.clone()),
            pending: Some(PrChangesPending {
                scope_repo_id,
                pr_number: identity.pr_number,
                head_sha: identity.head_sha,
                request_id,
            }),
            next_request_id: request_id,
            ..PrChangesState::default()
        };
        self.prs_state.pr_focus = PrFocus::PrChanges;
        true
    }

    fn accept_pr_changes(&mut self, event: &AppEvent) -> bool {
        let AppEvent::PrChangesLoaded {
            scope_repo_id,
            pr_number,
            request_id,
            files,
            truncated,
        } = event
        else {
            return false;
        };
        if !self.pr_changes_pending_matches(scope_repo_id, *pr_number, *request_id) {
            return true;
        }
        self.prs_state.changes.files.clone_from(files);
        self.prs_state.changes.selected_file = (!files.is_empty()).then_some(0);
        self.prs_state.changes.truncated = *truncated;
        self.prs_state.changes.pending = None;
        self.prs_state.changes.error = None;
        self.clear_pr_changes_blob_activity();
        true
    }

    fn fail_pr_changes(&mut self, event: &AppEvent) -> bool {
        let AppEvent::PrChangesLoadFailed {
            scope_repo_id,
            pr_number,
            request_id,
            error,
        } = event
        else {
            return false;
        };
        if !self.pr_changes_pending_matches(scope_repo_id, *pr_number, *request_id) {
            return true;
        }
        self.prs_state.changes.pending = None;
        self.prs_state.changes.error = Some(error.clone());
        self.clear_pr_changes_blob_activity();
        true
    }

    fn clear_pr_changes_blob_activity(&mut self) {
        self.prs_state.changes.blob_pending = None;
        self.prs_state.changes.blob_error = None;
    }

    fn accept_pr_changes_blob(&mut self, event: &AppEvent) -> bool {
        let AppEvent::PrChangesBlobLoaded {
            scope_repo_id,
            pr_number,
            request_id,
            blob_sha,
            blob,
        } = event
        else {
            return false;
        };
        if !self.pr_changes_blob_pending_matches(scope_repo_id, *pr_number, *request_id, blob_sha) {
            return true;
        }
        self.prs_state
            .changes
            .blobs
            .retain(|entry| entry.blob_sha != *blob_sha);
        self.prs_state.changes.blobs.push(PrChangesBlobCache {
            blob_sha: blob_sha.clone(),
            blob: blob.clone(),
        });
        let excess = self
            .prs_state
            .changes
            .blobs
            .len()
            .saturating_sub(PR_CHANGES_BLOB_CACHE_CAPACITY);
        drop(self.prs_state.changes.blobs.drain(..excess));
        self.prs_state.changes.blob_pending = None;
        self.prs_state.changes.blob_error = None;
        true
    }

    fn fail_pr_changes_blob(&mut self, event: &AppEvent) -> bool {
        let AppEvent::PrChangesBlobLoadFailed {
            scope_repo_id,
            pr_number,
            request_id,
            blob_sha,
            error,
        } = event
        else {
            return false;
        };
        if !self.pr_changes_blob_pending_matches(scope_repo_id, *pr_number, *request_id, blob_sha) {
            return true;
        }
        self.prs_state.changes.blob_pending = None;
        self.prs_state.changes.blob_error = Some(error.clone());
        true
    }

    fn pr_changes_blob_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        request_id: u64,
        blob_sha: &str,
    ) -> bool {
        self.prs_state
            .changes
            .blob_pending
            .as_ref()
            .is_some_and(|pending| {
                &pending.scope_repo_id == scope_repo_id
                    && pending.pr_number == pr_number
                    && pending.request_id == request_id
                    && pending.blob_sha == blob_sha
            })
    }

    fn pr_changes_pending_matches(
        &self,
        scope_repo_id: &crate::domain::RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        self.prs_state
            .changes
            .pending
            .as_ref()
            .is_some_and(|pending| {
                &pending.scope_repo_id == scope_repo_id
                    && pending.pr_number == pr_number
                    && pending.request_id == request_id
            })
    }

    fn focus_pr_changes_content(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return true;
        }
        if self.prs_state.changes.selected_file.is_some() {
            self.prs_state.changes.focus = PrChangesFocus::Content;
            self.prs_state.changes.selected_row = Some(0);
        }
        true
    }

    fn focus_pr_changes_files(&mut self) -> bool {
        if self.prs_state.pr_focus == PrFocus::PrChanges {
            self.prs_state.changes.focus = PrChangesFocus::FileList;
        }
        true
    }

    fn toggle_pr_changes_view(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return true;
        }
        self.prs_state.changes.view_mode = match self.prs_state.changes.view_mode {
            PrDiffViewMode::DeltasOnly => PrDiffViewMode::FullFile,
            PrDiffViewMode::FullFile => PrDiffViewMode::DeltasOnly,
        };
        if self.prs_state.changes.view_mode == PrDiffViewMode::FullFile {
            self.stage_selected_blob_read();
        } else {
            self.prs_state.changes.blob_pending = None;
        }
        self.clamp_pr_changes_selected_row();
        true
    }

    fn stage_selected_blob_read(&mut self) {
        let Some(blob_sha) = self
            .prs_state
            .changes
            .selected_file
            .and_then(|index| self.prs_state.changes.files.get(index))
            .map(|file| file.blob_sha.clone())
        else {
            self.clear_pr_changes_blob_activity();
            return;
        };
        let same_pending = self
            .prs_state
            .changes
            .blob_pending
            .as_ref()
            .is_some_and(|pending| pending.blob_sha == blob_sha);
        self.prs_state.changes.blob_error = None;
        if same_pending {
            return;
        }
        self.prs_state.changes.blob_pending = None;
        if self
            .prs_state
            .changes
            .blobs
            .iter()
            .any(|entry| entry.blob_sha == blob_sha)
        {
            return;
        }
        let Some(identity) = self.prs_state.changes.identity.as_ref() else {
            return;
        };
        let request_id = self.prs_state.changes.next_request_id.saturating_add(1);
        self.prs_state.changes.next_request_id = request_id;
        self.prs_state.changes.blob_pending = Some(PrChangesBlobPending {
            scope_repo_id: identity.scope_repo_id.clone(),
            pr_number: identity.pr_number,
            request_id,
            blob_sha,
        });
    }

    fn back_from_pr_changes(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return true;
        }
        if self.prs_state.changes.focus == PrChangesFocus::Content {
            self.prs_state.changes.focus = PrChangesFocus::FileList;
        } else {
            self.prs_state.pr_focus = PrFocus::PrDetail;
            self.prs_state.changes.pending = None;
        }
        true
    }

    fn clamp_pr_changes_selected_row(&mut self) {
        let threads = pr_review_threads(self.prs_state.pr_detail.as_ref());
        let Some(file) = self
            .prs_state
            .changes
            .selected_file
            .and_then(|index| self.prs_state.changes.files.get(index))
            .cloned()
        else {
            self.prs_state.changes.selected_row = None;
            return;
        };
        let base = pr_changes_base_document(&self.prs_state.changes, &file).unwrap_or_else(|| {
            crate::pr_diff_content::DiffDocument {
                rows: vec![crate::pr_diff_content::DiffDocumentRow {
                    text: "Loading full file…".to_string(),
                    role: crate::pr_diff_content::DiffRowRole::Notice,
                    anchor: None,
                    thread_index: None,
                }],
            }
        });
        let rows = crate::pr_diff_content::build_threaded_document(&file, base, &threads)
            .rows
            .len();
        self.prs_state.changes.selected_row = rows
            .checked_sub(1)
            .map(|last| self.prs_state.changes.selected_row.unwrap_or(0).min(last));
    }

    fn open_pr_changes_comment(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges
            || self.prs_state.changes.focus != PrChangesFocus::Content
            || self.prs_state.inline_state != super::InlineState::None
        {
            return true;
        }
        let Some((path, anchor)) = self.selected_pr_changes_anchor() else {
            self.prs_state.error = Some("Select a changed diff line to comment".to_string());
            return true;
        };
        let side = match anchor.side {
            crate::pr_diff_content::DiffAnchorSide::Left => crate::domain::PrReviewThreadSide::Left,
            crate::pr_diff_content::DiffAnchorSide::Right => {
                crate::domain::PrReviewThreadSide::Right
            }
        };
        let commit_id = self
            .prs_state
            .changes
            .identity
            .as_ref()
            .map_or_else(String::new, |identity| identity.head_sha.clone());
        self.prs_state.inline_state = super::InlineState::Composer {
            target: super::ComposerTarget::NewReviewThread {
                target: crate::domain::PrReviewCommentTarget {
                    path,
                    line: anchor.line,
                    side,
                    commit_id,
                },
            },
            text: String::new(),
            cursor: 0,
        };
        true
    }

    fn selected_pr_changes_anchor(
        &self,
    ) -> Option<(String, crate::pr_diff_content::DiffRowAnchor)> {
        let changes = &self.prs_state.changes;
        let file = changes
            .selected_file
            .and_then(|index| changes.files.get(index))?;
        let base = if changes.view_mode == PrDiffViewMode::FullFile {
            let blob = changes
                .blobs
                .iter()
                .find(|entry| entry.blob_sha == file.blob_sha)?;
            crate::pr_diff_content::build_full_document(file, &blob.blob)
        } else {
            crate::pr_diff_content::build_delta_document(file)
        };
        let threads = pr_review_threads(self.prs_state.pr_detail.as_ref());
        let document = crate::pr_diff_content::build_threaded_document(file, base, &threads);
        let row = changes
            .selected_row
            .and_then(|index| document.rows.get(index))?;
        Some((file.path.clone(), row.anchor.clone()?))
    }

    fn navigate_pr_changes(&mut self, delta: isize) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return false;
        }
        match self.prs_state.changes.focus {
            PrChangesFocus::FileList => {
                move_selection(
                    &mut self.prs_state.changes.selected_file,
                    self.prs_state.changes.files.len(),
                    delta,
                );
                self.prs_state.changes.selected_row = Some(0);
                if self.prs_state.changes.view_mode == PrDiffViewMode::FullFile {
                    self.stage_selected_blob_read();
                }
            }
            PrChangesFocus::Content => {
                let len = self.pr_changes_document_len();
                move_bounded_row(&mut self.prs_state.changes.selected_row, len, delta);
            }
        }
        true
    }

    fn navigate_pr_changes_home(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return false;
        }
        match self.prs_state.changes.focus {
            PrChangesFocus::FileList => {
                self.prs_state.changes.selected_file =
                    (!self.prs_state.changes.files.is_empty()).then_some(0);
            }
            PrChangesFocus::Content => self.prs_state.changes.selected_row = Some(0),
        }
        true
    }

    fn navigate_pr_changes_end(&mut self) -> bool {
        if self.prs_state.pr_focus != PrFocus::PrChanges {
            return false;
        }
        match self.prs_state.changes.focus {
            PrChangesFocus::FileList => {
                self.prs_state.changes.selected_file =
                    self.prs_state.changes.files.len().checked_sub(1);
            }
            PrChangesFocus::Content => {
                self.prs_state.changes.selected_row = self.pr_changes_document_len().checked_sub(1);
            }
        }
        true
    }

    fn pr_changes_document_len(&self) -> usize {
        let changes = &self.prs_state.changes;
        let Some(file) = changes
            .selected_file
            .and_then(|index| changes.files.get(index))
        else {
            return 0;
        };
        let base = if changes.view_mode == PrDiffViewMode::FullFile {
            let Some(entry) = changes
                .blobs
                .iter()
                .find(|entry| entry.blob_sha == file.blob_sha)
            else {
                return 0;
            };
            crate::pr_diff_content::build_full_document(file, &entry.blob)
        } else {
            crate::pr_diff_content::build_delta_document(file)
        };
        let threads = pr_review_threads(self.prs_state.pr_detail.as_ref());
        crate::pr_diff_content::build_threaded_document(file, base, &threads)
            .rows
            .len()
    }
}
fn pr_review_threads(
    detail: Option<&crate::domain::PullRequestDetail>,
) -> Vec<crate::domain::PrReviewThread> {
    detail.map_or_else(Vec::new, |detail| {
        detail
            .reviews
            .iter()
            .flat_map(|review| review.review_threads.iter().cloned())
            .collect()
    })
}

fn pr_changes_base_document(
    changes: &PrChangesState,
    file: &crate::domain::PrFileChange,
) -> Option<crate::pr_diff_content::DiffDocument> {
    if changes.view_mode == PrDiffViewMode::FullFile {
        let entry = changes
            .blobs
            .iter()
            .find(|entry| entry.blob_sha == file.blob_sha)?;
        Some(crate::pr_diff_content::build_full_document(
            file,
            &entry.blob,
        ))
    } else {
        Some(crate::pr_diff_content::build_delta_document(file))
    }
}

fn move_selection(selected: &mut Option<usize>, len: usize, delta: isize) {
    if len == 0 {
        *selected = None;
        return;
    }
    let current = selected.unwrap_or(0);
    *selected = Some(if delta < 0 {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta.unsigned_abs()).min(len - 1)
    });
}

fn move_bounded_row(selected: &mut Option<usize>, len: usize, delta: isize) {
    move_selection(selected, len, delta);
}

fn page_delta(items: usize) -> isize {
    isize::try_from(items).unwrap_or(isize::MAX)
}
