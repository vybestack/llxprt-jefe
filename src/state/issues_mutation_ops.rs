//! Issues-mode mutation state operations.

fn update_comment_body(
    detail: &mut crate::domain::IssueDetail,
    comment_id: u64,
    comment_index: usize,
    body: String,
) -> bool {
    if let Some(comment) = detail.comments.get_mut(comment_index)
        && comment.comment_id == comment_id
    {
        comment.body = body;
        return true;
    }
    if let Some(comment) = detail
        .comments
        .iter_mut()
        .find(|comment| comment.comment_id == comment_id)
    {
        comment.body = body;
        return true;
    }
    false
}
use super::{AppEvent, AppState, InlineState, IssueMutationPending};

impl AppState {
    pub(crate) fn apply_issue_mutation_event(&mut self, event: AppEvent) -> bool {
        let is_comment_created = matches!(event, AppEvent::CommentCreated { .. });
        let applied = match event {
            AppEvent::MutationSubmitted {
                scope_repo_id,
                mutation_id,
                target,
            } => {
                self.issues_state.mutation_pending = Some(IssueMutationPending {
                    scope_repo_id,
                    id: mutation_id,
                    target,
                });
                return true;
            }
            AppEvent::IssueCreated {
                scope_repo_id,
                mutation_id,
                issue_number,
            } => self.apply_issue_created(&scope_repo_id, mutation_id, issue_number),
            AppEvent::CommentCreated { .. }
            | AppEvent::IssueBodyUpdated { .. }
            | AppEvent::CommentUpdated { .. } => self.apply_issue_mutation_success(event),
            _ => return false,
        };
        if applied {
            self.clear_applied_mutation();
        }
        if applied && is_comment_created {
            self.scroll_detail_to_bottom();
        }
        true
    }

    /// Apply a successful issue creation.
    ///
    /// The scope is captured at submission time (carried on the event), so a
    /// late-arriving success cannot be misattributed to a repository the user
    /// has since switched to. Gating on the originally-submitted `mutation_id`
    /// and scope keeps this reducer deterministic and side-effect free.
    fn apply_issue_created(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        mutation_id: u64,
        issue_number: u64,
    ) -> bool {
        if !self.mutation_pending_matches(mutation_id)
            || self.selected_repository_id() != Some(scope_repo_id)
        {
            return false;
        }
        self.issues_state.draft_notice = Some(format!("Created issue #{issue_number}"));
        true
    }

    fn apply_issue_mutation_success(&mut self, event: AppEvent) -> bool {
        match event {
            AppEvent::CommentCreated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment,
            } => self.with_matching_pending_detail(
                mutation_id,
                scope_repo_id,
                issue_number,
                |detail| {
                    detail.comments.push(comment);
                    true
                },
            ),
            AppEvent::IssueBodyUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                body,
            } => self.with_matching_pending_detail(
                mutation_id,
                scope_repo_id,
                issue_number,
                |detail| {
                    detail.body = body;
                    true
                },
            ),
            AppEvent::CommentUpdated {
                scope_repo_id,
                issue_number,
                mutation_id,
                comment_id,
                comment_index,
                body,
            } => self.with_matching_pending_detail(
                mutation_id,
                scope_repo_id,
                issue_number,
                |detail| update_comment_body(detail, comment_id, comment_index, body),
            ),
            _ => false,
        }
    }

    fn clear_applied_mutation(&mut self) {
        let submitted_target = self
            .issues_state
            .mutation_pending
            .as_ref()
            .map(|pending| pending.target.clone());
        self.issues_state.error = None;
        if submitted_target.as_ref() == Some(&self.issues_state.inline_state) {
            self.issues_state.inline_state = InlineState::None;
        }
        self.issues_state.mutation_pending = None;
    }

    pub(crate) fn apply_scoped_mutation_error(
        &mut self,
        scope_repo_id: &crate::domain::RepositoryId,
        issue_number: Option<u64>,
        mutation_id: Option<u64>,
        error: String,
    ) {
        let Some(mutation_id) = mutation_id else {
            let issue_matches = match issue_number {
                Some(number) => self
                    .issues_state
                    .issue_detail
                    .as_ref()
                    .is_some_and(|detail| detail.number == number),
                None => true,
            };
            if self.issues_state.mutation_pending.is_none()
                && self.selected_repository_id() == Some(scope_repo_id)
                && issue_matches
            {
                self.issues_state.error = Some(error);
            }
            return;
        };
        if !self.mutation_pending_matches(mutation_id)
            || self.selected_repository_id() != Some(scope_repo_id)
        {
            return;
        }
        if let Some(number) = issue_number {
            let Some(detail) = self.issues_state.issue_detail.as_ref() else {
                return;
            };
            if detail.number != number {
                return;
            }
        }
        self.issues_state.error = Some(error);
        self.issues_state.mutation_pending = None;
    }

    fn mutation_pending_matches(&self, mutation_id: u64) -> bool {
        self.issues_state
            .mutation_pending
            .as_ref()
            .is_some_and(|pending| pending.id == mutation_id)
    }

    fn with_matching_pending_detail<F>(
        &mut self,
        mutation_id: u64,
        scope_repo_id: crate::domain::RepositoryId,
        issue_number: u64,
        f: F,
    ) -> bool
    where
        F: FnOnce(&mut crate::domain::IssueDetail) -> bool,
    {
        if !self.mutation_pending_matches(mutation_id)
            || self.selected_repository_id() != Some(&scope_repo_id)
        {
            return false;
        }
        let Some(detail) = &mut self.issues_state.issue_detail else {
            return false;
        };
        if detail.number == issue_number {
            f(detail)
        } else {
            false
        }
    }
}
