use crate::domain::RepositoryId;

use super::{AppState, InlineState, IssueFocus, ModalState, PaneFocus, PrFocus, ScreenIdentity};

#[derive(Clone, Copy)]
enum BlockingInteraction {
    TerminalFocused,
    Inactive,
    AgentChooser,
    PropertyEditor,
    FilterControls,
    SearchInput,
    NewIssueForm,
    DeleteConfirmation,
    CloseReasonChooser,
    MergeChooser,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct InteractionFlags(u16);

impl InteractionFlags {
    fn set_if(mut self, interaction: BlockingInteraction, blocked: bool) -> Self {
        if blocked {
            self.0 |= 1 << interaction as u16;
        }
        self
    }

    fn is_clear(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct IssueListSendContext {
    screen: ScreenIdentity,
    modal: ModalState,
    pane_focus: PaneFocus,
    focus: IssueFocus,
    repository_id: Option<RepositoryId>,
    issue_number: Option<u64>,
    inline_state: InlineState,
    interactions: InteractionFlags,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct PrListSendContext {
    screen: ScreenIdentity,
    modal: ModalState,
    pane_focus: PaneFocus,
    focus: PrFocus,
    repository_id: Option<RepositoryId>,
    pr_number: Option<u64>,
    inline_state: InlineState,
    interactions: InteractionFlags,
}

impl IssueListSendContext {
    fn accepts_list_send(&self) -> bool {
        self.screen == super::ScreenId::Issues
            && self.modal == ModalState::None
            && self.focus == IssueFocus::IssueList
            && self.inline_state == InlineState::None
            && self.interactions.is_clear()
    }
}

impl PrListSendContext {
    fn accepts_list_send(&self) -> bool {
        self.screen == super::ScreenId::PullRequests
            && self.modal == ModalState::None
            && self.focus == PrFocus::PrList
            && self.inline_state == InlineState::None
            && self.interactions.is_clear()
    }
}

impl AppState {
    fn selected_issue_number(&self) -> Option<u64> {
        self.issues_state
            .selected_issue_index()
            .and_then(|index| self.issues_state.issues().get(index))
            .map(|issue| issue.number)
    }

    fn selected_pr_number(&self) -> Option<u64> {
        self.prs_state
            .selected_pr_index()
            .and_then(|index| self.prs_state.pull_requests().get(index))
            .map(|pr| pr.number)
    }

    pub(super) fn issue_list_send_context(&self) -> IssueListSendContext {
        IssueListSendContext {
            screen: self.screen(),
            modal: self.modal.clone(),
            pane_focus: self.pane_focus,
            focus: self.issues_state.issue_focus,
            repository_id: self.selected_repository_id().cloned(),
            issue_number: self.selected_issue_number(),
            inline_state: self.issues_state.inline_state.clone(),
            interactions: InteractionFlags::default()
                .set_if(BlockingInteraction::TerminalFocused, self.terminal_focused)
                .set_if(BlockingInteraction::Inactive, !self.issues_state.active)
                .set_if(
                    BlockingInteraction::AgentChooser,
                    self.issues_state.agent_chooser.is_some(),
                )
                .set_if(
                    BlockingInteraction::PropertyEditor,
                    self.issues_state.property_editor.is_some(),
                )
                .set_if(
                    BlockingInteraction::FilterControls,
                    self.issues_state.filter_ui.controls_open,
                )
                .set_if(
                    BlockingInteraction::SearchInput,
                    self.issues_state.search_input_focused,
                )
                .set_if(
                    BlockingInteraction::NewIssueForm,
                    self.issues_state.new_issue_form.is_some(),
                )
                .set_if(
                    BlockingInteraction::DeleteConfirmation,
                    self.issues_state.delete_confirm.is_some(),
                )
                .set_if(
                    BlockingInteraction::CloseReasonChooser,
                    self.issues_state.close_reason_chooser.is_some(),
                ),
        }
    }

    pub(super) fn pr_list_send_context(&self) -> PrListSendContext {
        PrListSendContext {
            screen: self.screen(),
            modal: self.modal.clone(),
            pane_focus: self.pane_focus,
            focus: self.prs_state.pr_focus,
            repository_id: self.selected_repository_id().cloned(),
            pr_number: self.selected_pr_number(),
            inline_state: self.prs_state.inline_state.clone(),
            interactions: InteractionFlags::default()
                .set_if(BlockingInteraction::TerminalFocused, self.terminal_focused)
                .set_if(BlockingInteraction::Inactive, !self.prs_state.active)
                .set_if(
                    BlockingInteraction::AgentChooser,
                    self.prs_state.agent_chooser.is_some(),
                )
                .set_if(
                    BlockingInteraction::MergeChooser,
                    self.prs_state.merge_chooser.is_some(),
                )
                .set_if(
                    BlockingInteraction::PropertyEditor,
                    self.prs_state.property_editor.is_some(),
                )
                .set_if(
                    BlockingInteraction::FilterControls,
                    self.prs_state.filter_ui.controls_open,
                )
                .set_if(
                    BlockingInteraction::SearchInput,
                    self.prs_state.search_input_focused,
                ),
        }
    }

    pub(super) fn invalidate_changed_list_send_contexts(
        &mut self,
        issue_before: &IssueListSendContext,
        pr_before: &PrListSendContext,
    ) {
        let issue_after = self.issue_list_send_context();
        if *issue_before != issue_after {
            self.cancel_issue_list_send();
            if self
                .issues_state
                .detail_pending
                .as_ref()
                .is_some_and(|pending| {
                    issue_after.repository_id.as_ref() != Some(&pending.scope_repo_id)
                        || issue_after.issue_number != Some(pending.issue_number)
                })
            {
                self.issues_state.detail_pending = None;
                self.issues_state.loading.detail = false;
            }
        }
        let pr_after = self.pr_list_send_context();
        if *pr_before != pr_after {
            self.cancel_pr_list_send();
            if self
                .prs_state
                .detail_pending
                .as_ref()
                .is_some_and(|pending| {
                    pr_after.repository_id.as_ref() != Some(&pending.scope_repo_id)
                        || pr_after.pr_number != Some(pending.pr_number)
                })
            {
                self.prs_state.detail_pending = None;
                self.prs_state.loading.detail = false;
            }
        }
    }

    #[must_use]
    pub fn issue_detail_request_is_current(
        &self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self.issue_selection_matches(issue_number)
            && self
                .issues_state
                .detail_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.issue_number == issue_number
                        && pending.request_id == request_id
                })
    }

    fn issue_selection_matches(&self, issue_number: u64) -> bool {
        self.issues_state.issues().is_empty() || self.selected_issue_number() == Some(issue_number)
    }

    #[must_use]
    pub fn issue_list_send_request_is_current(
        &self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) -> bool {
        self.issue_detail_request_is_current(scope_repo_id, issue_number, request_id)
            && self.issue_list_send_request_is_pending(scope_repo_id, issue_number, request_id)
    }

    #[must_use]
    pub fn pr_detail_request_is_current(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        self.selected_repository_id() == Some(scope_repo_id)
            && self.selected_pr_number() == Some(pr_number)
            && self
                .prs_state
                .detail_pending
                .as_ref()
                .is_some_and(|pending| {
                    pending.scope_repo_id == *scope_repo_id
                        && pending.pr_number == pr_number
                        && pending.request_id == request_id
                })
    }

    #[must_use]
    pub fn pr_list_send_request_is_current(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        self.pr_detail_request_is_current(scope_repo_id, pr_number, request_id)
            && self.pr_list_send_request_is_pending(scope_repo_id, pr_number, request_id)
    }

    #[must_use]
    pub fn issue_list_send_request_is_pending(
        &self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) -> bool {
        self.issues_state
            .list_send_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.issue_number == issue_number
                    && pending.request_id == request_id
            })
    }

    #[must_use]
    pub fn pr_list_send_request_is_pending(
        &self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) -> bool {
        self.prs_state
            .list_send_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.pr_number == pr_number
                    && pending.request_id == request_id
            })
    }

    pub(super) fn begin_issue_list_send_detail(
        &mut self,
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    ) {
        self.cancel_issue_list_send();
        let context_available = self.issue_list_send_context().accepts_list_send();
        let (Some(scope_repo_id), Some(issue_number)) = (
            self.selected_repository_id().cloned(),
            self.selected_issue_number(),
        ) else {
            return;
        };
        if !context_available {
            return;
        }
        let request_id = self.next_issue_detail_request_id();
        self.mark_issue_detail_loading_with_request_id(
            scope_repo_id.clone(),
            issue_number,
            request_id,
        );
        self.issues_state.list_send_pending = Some(super::IssueListSendPending {
            scope_repo_id,
            issue_number,
            request_id,
            metadata,
            ready: false,
        });
    }

    pub(super) fn begin_pr_list_send_detail(
        &mut self,
        metadata: Vec<crate::domain::AgentChooserGitMetadata>,
    ) {
        self.cancel_pr_list_send();
        let context_available = self.pr_list_send_context().accepts_list_send();
        let (Some(scope_repo_id), Some(pr_number)) = (
            self.selected_repository_id().cloned(),
            self.selected_pr_number(),
        ) else {
            return;
        };
        if !context_available {
            return;
        }
        let request_id = self.next_pr_detail_request_id();
        self.mark_pr_detail_loading(scope_repo_id.clone(), pr_number, request_id);
        self.prs_state.list_send_pending = Some(super::PrListSendPending {
            scope_repo_id,
            pr_number,
            request_id,
            metadata,
            ready: false,
        });
    }

    pub(super) fn mark_issue_list_send_ready(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        if let Some(pending) = &mut self.issues_state.list_send_pending
            && pending.scope_repo_id == *scope_repo_id
            && pending.issue_number == issue_number
            && pending.request_id == request_id
        {
            pending.ready = true;
        }
    }

    pub(super) fn mark_pr_list_send_ready(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
        if let Some(pending) = &mut self.prs_state.list_send_pending
            && pending.scope_repo_id == *scope_repo_id
            && pending.pr_number == pr_number
            && pending.request_id == request_id
        {
            pending.ready = true;
        }
    }

    pub(super) fn clear_issue_detail_request(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        self.clear_issue_list_send_request(scope_repo_id, issue_number, request_id);
        if self
            .issues_state
            .detail_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.issue_number == issue_number
                    && pending.request_id == request_id
            })
        {
            self.issues_state.loading.detail = false;
            self.issues_state.detail_pending = None;
            self.issues_state.error = None;
        }
    }

    pub(super) fn clear_pr_detail_request(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
        self.clear_pr_list_send_request(scope_repo_id, pr_number, request_id);
        if self
            .prs_state
            .detail_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == *scope_repo_id
                    && pending.pr_number == pr_number
                    && pending.request_id == request_id
            })
        {
            self.prs_state.loading.detail = false;
            self.prs_state.detail_pending = None;
            self.prs_state.error = None;
        }
    }
    pub(super) fn clear_issue_list_send_request(
        &mut self,
        scope_repo_id: &RepositoryId,
        issue_number: u64,
        request_id: u64,
    ) {
        if self.issue_list_send_request_is_pending(scope_repo_id, issue_number, request_id) {
            self.issues_state.list_send_pending = None;
        }
    }

    pub(super) fn cancel_issue_list_send(&mut self) {
        let Some(pending) = self.issues_state.list_send_pending.take() else {
            return;
        };
        if self
            .issues_state
            .detail_pending
            .as_ref()
            .is_some_and(|detail| {
                detail.scope_repo_id == pending.scope_repo_id
                    && detail.issue_number == pending.issue_number
                    && detail.request_id == pending.request_id
            })
        {
            self.issues_state.detail_pending = None;
            self.issues_state.loading.detail = false;
        }
    }

    pub(super) fn clear_pr_list_send_request(
        &mut self,
        scope_repo_id: &RepositoryId,
        pr_number: u64,
        request_id: u64,
    ) {
        if self.pr_list_send_request_is_pending(scope_repo_id, pr_number, request_id) {
            self.prs_state.list_send_pending = None;
        }
    }

    pub(super) fn cancel_pr_list_send(&mut self) {
        let Some(pending) = self.prs_state.list_send_pending.take() else {
            return;
        };
        if self
            .prs_state
            .detail_pending
            .as_ref()
            .is_some_and(|detail| {
                detail.scope_repo_id == pending.scope_repo_id
                    && detail.pr_number == pending.pr_number
                    && detail.request_id == pending.request_id
            })
        {
            self.prs_state.detail_pending = None;
            self.prs_state.loading.detail = false;
        }
    }
}
