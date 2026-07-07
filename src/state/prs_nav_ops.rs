//! Pull Requests mode navigation state operations.
//!
//! Focus cycling, PR-list navigation, detail subfocus cycling, and detail
//! scroll — extracted from prs_ops.rs to keep each handler module under the
//! 850-line architecture boundary limit.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-003
//! @requirement REQ-PR-006
//! @requirement REQ-PR-009
//! @requirement REQ-PR-NFR-002
//! @pseudocode component-001 lines 99-124,134-208

use super::{AppEvent, AppState, ComposerTarget, InlineState, PrFocus};
use crate::messages::ScrollDir;

impl AppState {
    /// Cycle PR focus forward: RepoList -> PrList -> PrDetail -> RepoList.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 154-162
    fn cycle_prs_focus(&mut self) {
        self.prs_state.pr_focus = match self.prs_state.pr_focus {
            PrFocus::RepoList => PrFocus::PrList,
            PrFocus::PrList => PrFocus::PrDetail,
            PrFocus::PrDetail => PrFocus::RepoList,
        };
    }

    /// Cycle PR focus reverse: RepoList -> PrDetail -> PrList -> RepoList.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 154-162
    fn cycle_prs_focus_reverse(&mut self) {
        self.prs_state.pr_focus = match self.prs_state.pr_focus {
            PrFocus::RepoList => PrFocus::PrDetail,
            PrFocus::PrList => PrFocus::RepoList,
            PrFocus::PrDetail => PrFocus::PrList,
        };
    }

    /// Navigate repo up in PR mode (thin wrapper over shared helper).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 146-153
    fn navigate_repo_up_in_prs_mode(&mut self) {
        if self.move_repo_selection(crate::messages::NavDir::Up) {
            self.reset_prs_for_repo_change();
        }
    }

    /// Navigate repo down in PR mode (thin wrapper over shared helper).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 146-153
    fn navigate_repo_down_in_prs_mode(&mut self) {
        if self.move_repo_selection(crate::messages::NavDir::Down) {
            self.reset_prs_for_repo_change();
        }
    }

    // ---- PR list navigation ----

    /// Navigate PR list up by one.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 99-106
    fn navigate_pr_list_up(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        if let Some(idx) = previous
            && idx > 0
        {
            self.prs_state.selected_pr_index = Some(idx - 1);
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Navigate PR list down by one.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 108-117
    fn navigate_pr_list_down(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        if let Some(idx) = previous
            && idx + 1 < self.prs_state.pull_requests.len()
        {
            self.prs_state.selected_pr_index = Some(idx + 1);
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Navigate PR list up by one page.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 119-124
    fn navigate_pr_list_page_up(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        let page = self.prs_state.list_viewport_rows.max(1);
        if let Some(idx) = previous {
            self.prs_state.selected_pr_index = Some(idx.saturating_sub(page));
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Navigate PR list down by one page.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 119-124
    fn navigate_pr_list_page_down(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        let page = self.prs_state.list_viewport_rows.max(1);
        if let Some(idx) = previous {
            let max = self.prs_state.pull_requests.len().saturating_sub(1);
            self.prs_state.selected_pr_index = Some((idx + page).min(max));
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Navigate PR list to first row.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 119-124
    fn navigate_pr_list_home(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        if !self.prs_state.pull_requests.is_empty() {
            self.prs_state.selected_pr_index = Some(0);
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Navigate PR list to last row.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 119-124
    fn navigate_pr_list_end(&mut self) {
        let previous = self.prs_state.selected_pr_index;
        if !self.prs_state.pull_requests.is_empty() {
            self.prs_state.selected_pr_index = Some(self.prs_state.pull_requests.len() - 1);
        }
        self.update_pr_list_scroll_offset();
        self.invalidate_detail_requests_if_pr_selection_changed(previous);
    }

    /// Update list_scroll_offset via the shared selection-follow helper.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 182-189
    fn update_pr_list_scroll_offset(&mut self) {
        let sel = self.prs_state.selected_pr_index.unwrap_or(0);
        let len = self.prs_state.pull_requests.len();
        let vp = self.prs_state.list_viewport_rows;
        self.prs_state.list_scroll_offset = crate::layout::list_first_visible_index(sel, len, vp);
    }

    /// Invalidate detail requests when the PR selection changes.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-NFR-002
    /// @pseudocode component-001 lines 88-98
    fn invalidate_detail_requests_if_pr_selection_changed(&mut self, previous: Option<usize>) {
        if self.prs_state.selected_pr_index == previous {
            return;
        }
        self.prs_state.loading.detail = false;
        self.prs_state.loading.comments = false;
        self.prs_state.detail_pending = None;
        self.prs_state.comments_page_pending = None;
        self.prs_state.detail_scroll_offset = 0;
    }

    // ---- Detail subfocus cycling ----

    /// Advance detail subfocus: Body -> Review -> Check -> Comment -> NewComment.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn pr_detail_subfocus_next(&mut self) {
        use super::PrDetailSubfocus;
        let Some(detail) = &self.prs_state.pr_detail else {
            return;
        };
        let review_count = detail.reviews.len();
        let thread_count = count_review_threads(detail);
        let check_count = detail.checks.len();
        let comment_count = detail.comments.len();
        self.prs_state.detail_subfocus = match self.prs_state.detail_subfocus {
            PrDetailSubfocus::Body => {
                Self::next_after_body(review_count, thread_count, check_count, comment_count)
            }
            PrDetailSubfocus::Review(i) => {
                Self::next_after_review(i, review_count, thread_count, check_count, comment_count)
            }
            PrDetailSubfocus::ReviewThread(i) => {
                Self::next_after_thread(i, thread_count, check_count, comment_count)
            }
            PrDetailSubfocus::Check(i) => Self::next_after_check(i, check_count, comment_count),
            PrDetailSubfocus::Comment(i) => Self::next_after_comment(i, comment_count),
            PrDetailSubfocus::NewComment => PrDetailSubfocus::Body,
        };
    }

    /// Reverse detail subfocus: NewComment -> Comment -> Check -> Review -> Body.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn pr_detail_subfocus_prev(&mut self) {
        use super::PrDetailSubfocus;
        let Some(detail) = &self.prs_state.pr_detail else {
            return;
        };
        let review_count = detail.reviews.len();
        let thread_count = count_review_threads(detail);
        let check_count = detail.checks.len();
        let comment_count = detail.comments.len();
        self.prs_state.detail_subfocus = match self.prs_state.detail_subfocus {
            PrDetailSubfocus::Body => {
                Self::prev_from_body(comment_count, check_count, thread_count, review_count)
            }
            PrDetailSubfocus::Review(0) => PrDetailSubfocus::Body,
            PrDetailSubfocus::Review(i) => PrDetailSubfocus::Review(i - 1),
            PrDetailSubfocus::ReviewThread(0) => Self::prev_from_thread_zero(review_count),
            PrDetailSubfocus::ReviewThread(i) => PrDetailSubfocus::ReviewThread(i - 1),
            PrDetailSubfocus::Check(0) => Self::prev_from_check_zero(thread_count, review_count),
            PrDetailSubfocus::Check(i) => PrDetailSubfocus::Check(i - 1),
            PrDetailSubfocus::Comment(0) => {
                Self::prev_from_comment_zero(review_count, check_count, thread_count)
            }
            PrDetailSubfocus::Comment(i) => PrDetailSubfocus::Comment(i - 1),
            PrDetailSubfocus::NewComment => {
                if comment_count > 0 {
                    PrDetailSubfocus::Comment(comment_count - 1)
                } else {
                    PrDetailSubfocus::Body
                }
            }
        };
    }

    /// Compute the next subfocus from Body (skip empty sections).
    /// Cycle: Body -> Review -> ReviewThread -> Check -> Comment -> NewComment.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn next_after_body(
        reviews: usize,
        threads: usize,
        checks: usize,
        comments: usize,
    ) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if reviews > 0 {
            PrDetailSubfocus::Review(0)
        } else if threads > 0 {
            PrDetailSubfocus::ReviewThread(0)
        } else if checks > 0 {
            PrDetailSubfocus::Check(0)
        } else if comments > 0 {
            PrDetailSubfocus::Comment(0)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the next subfocus from Review(i) (advance, threads, or fall through).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn next_after_review(
        i: usize,
        reviews: usize,
        threads: usize,
        checks: usize,
        comments: usize,
    ) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if i + 1 < reviews {
            PrDetailSubfocus::Review(i + 1)
        } else if threads > 0 {
            PrDetailSubfocus::ReviewThread(0)
        } else if checks > 0 {
            PrDetailSubfocus::Check(0)
        } else if comments > 0 {
            PrDetailSubfocus::Comment(0)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the next subfocus from ReviewThread(i) (advance or fall through).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    fn next_after_thread(
        i: usize,
        threads: usize,
        checks: usize,
        comments: usize,
    ) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if i + 1 < threads {
            PrDetailSubfocus::ReviewThread(i + 1)
        } else if checks > 0 {
            PrDetailSubfocus::Check(0)
        } else if comments > 0 {
            PrDetailSubfocus::Comment(0)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the next subfocus from Check(i) (advance or fall through).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn next_after_check(i: usize, checks: usize, comments: usize) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if i + 1 < checks {
            PrDetailSubfocus::Check(i + 1)
        } else if comments > 0 {
            PrDetailSubfocus::Comment(0)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the next subfocus from Comment(i) (advance or NewComment).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @pseudocode component-001 lines 201-208
    fn next_after_comment(i: usize, comments: usize) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if i + 1 < comments {
            PrDetailSubfocus::Comment(i + 1)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the previous subfocus from Body (reverse: Comment, Check,
    /// ReviewThread, Review, NewComment).
    fn prev_from_body(
        comments: usize,
        checks: usize,
        threads: usize,
        reviews: usize,
    ) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if comments > 0 {
            PrDetailSubfocus::Comment(comments - 1)
        } else if checks > 0 {
            PrDetailSubfocus::Check(checks - 1)
        } else if threads > 0 {
            PrDetailSubfocus::ReviewThread(threads - 1)
        } else if reviews > 0 {
            PrDetailSubfocus::Review(reviews - 1)
        } else {
            PrDetailSubfocus::NewComment
        }
    }

    /// Compute the previous subfocus from ReviewThread(0) (go to last Review).
    fn prev_from_thread_zero(reviews: usize) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if reviews > 0 {
            PrDetailSubfocus::Review(reviews - 1)
        } else {
            PrDetailSubfocus::Body
        }
    }

    /// Compute the previous subfocus from Check(0) (reverse to ReviewThread/Review).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    fn prev_from_check_zero(threads: usize, reviews: usize) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if threads > 0 {
            PrDetailSubfocus::ReviewThread(threads - 1)
        } else if reviews > 0 {
            PrDetailSubfocus::Review(reviews - 1)
        } else {
            PrDetailSubfocus::Body
        }
    }

    /// Compute the previous subfocus from Comment(0) (reverse traversal).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    fn prev_from_comment_zero(
        reviews: usize,
        checks: usize,
        threads: usize,
    ) -> super::PrDetailSubfocus {
        use super::PrDetailSubfocus;
        if checks > 0 {
            PrDetailSubfocus::Check(checks - 1)
        } else if threads > 0 {
            PrDetailSubfocus::ReviewThread(threads - 1)
        } else if reviews > 0 {
            PrDetailSubfocus::Review(reviews - 1)
        } else {
            PrDetailSubfocus::Body
        }
    }

    // ---- Detail scroll ----

    /// Rows available to the read-only PR detail document after any embedded
    /// editor/composer reserves local rows inside the detail pane.
    ///
    /// The UI subtracts the same `PR_COMPOSER_VIEWPORT_ROWS` from the
    /// ScrollableText viewport when the NewComment TextBox is active. State uses
    /// this helper for open-reveal and scroll bounds so the document anchor is
    /// revealed above the embedded TextBox instead of under-scrolling.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    pub(super) fn pr_detail_scroll_viewport_rows(&self) -> usize {
        let text_box_active = matches!(
            &self.prs_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment
                    | ComposerTarget::Reply { .. }
                    | ComposerTarget::ReplyToReviewThread { .. },
                ..
            }
        );
        crate::layout::pr_detail_document_viewport_rows(
            self.prs_state.detail_viewport_rows,
            text_box_active,
        )
    }

    /// Apply a PR detail scroll event (bounded by rendered length).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    fn apply_pr_scroll_event(&mut self, dir: ScrollDir) -> bool {
        let max = self.pr_detail_max_scroll_offset();
        let current = self.prs_state.detail_scroll_offset;
        let page_rows = self.pr_detail_scroll_viewport_rows().max(1);
        let next = match dir {
            ScrollDir::Up => current.saturating_sub(1),
            ScrollDir::Down => (current + 1).min(max),
            ScrollDir::PageUp => current.saturating_sub(page_rows),
            ScrollDir::PageDown => current.saturating_add(page_rows).min(max),
        };
        if next == current {
            return false;
        }
        self.prs_state.detail_scroll_offset = next;
        true
    }

    /// Maximum detail scroll offset derived from the REAL rendered content
    /// length (mirrors `IssuesState::max_detail_scroll_offset_for_viewport`),
    /// so the clamp cannot drift from `build_pr_detail_content`.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    fn pr_detail_max_scroll_offset(&self) -> usize {
        let Some(detail) = &self.prs_state.pr_detail else {
            return 0;
        };
        crate::pr_detail_content::pr_detail_content_line_count(
            detail,
            self.prs_state.detail_subfocus,
            &self.prs_state.inline_state,
            self.prs_state.loading.detail,
            self.prs_state.loading.comments,
        )
        .saturating_sub(self.pr_detail_scroll_viewport_rows())
    }

    // ---- Navigation dispatch ----

    /// Apply PR navigation events.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 99-124
    pub(super) fn apply_pr_navigation_event(&mut self, event: &AppEvent) -> bool {
        if self.apply_pr_directional_navigation(event) {
            return true;
        }
        if self.apply_pr_scroll_detail_event(event) {
            return true;
        }
        match event {
            AppEvent::PrListEnter => {
                if self.prs_state.pr_focus == PrFocus::PrList
                    && self.prs_state.selected_pr_index.is_some()
                {
                    self.prs_state.pr_focus = PrFocus::PrDetail;
                    self.prs_state.detail_subfocus = super::PrDetailSubfocus::Body;
                    self.prs_state.detail_scroll_offset = 0;
                }
                true
            }
            AppEvent::PrCycleFocus => {
                self.cycle_prs_focus();
                true
            }
            AppEvent::PrCycleFocusReverse => {
                self.cycle_prs_focus_reverse();
                true
            }
            AppEvent::PrDetailSubfocusNext => {
                self.pr_detail_subfocus_next();
                true
            }
            AppEvent::PrDetailSubfocusPrev => {
                self.pr_detail_subfocus_prev();
                true
            }
            _ => false,
        }
    }

    /// Handle directional navigation (Up/Down/PageUp/PageDown/Home/End).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 99-124
    fn apply_pr_directional_navigation(&mut self, event: &AppEvent) -> bool {
        if self.apply_pr_line_navigation(event) {
            return true;
        }
        self.apply_pr_page_navigation(event)
    }

    /// Handle line-by-line Up/Down navigation.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 99-117
    fn apply_pr_line_navigation(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrNavigateUp => {
                match self.prs_state.pr_focus {
                    PrFocus::RepoList => self.navigate_repo_up_in_prs_mode(),
                    PrFocus::PrList => self.navigate_pr_list_up(),
                    PrFocus::PrDetail => {
                        self.apply_pr_scroll_event(ScrollDir::Up);
                    }
                }
                true
            }
            AppEvent::PrNavigateDown => {
                match self.prs_state.pr_focus {
                    PrFocus::RepoList => self.navigate_repo_down_in_prs_mode(),
                    PrFocus::PrList => self.navigate_pr_list_down(),
                    PrFocus::PrDetail => {
                        self.apply_pr_scroll_event(ScrollDir::Down);
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Handle page/home/end navigation.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-003
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 119-124
    fn apply_pr_page_navigation(&mut self, event: &AppEvent) -> bool {
        match event {
            AppEvent::PrNavigatePageUp => {
                match self.prs_state.pr_focus {
                    PrFocus::PrList => self.navigate_pr_list_page_up(),
                    PrFocus::PrDetail => {
                        self.apply_pr_scroll_event(ScrollDir::PageUp);
                    }
                    PrFocus::RepoList => {}
                }
                true
            }
            AppEvent::PrNavigatePageDown => {
                match self.prs_state.pr_focus {
                    PrFocus::PrList => self.navigate_pr_list_page_down(),
                    PrFocus::PrDetail => {
                        self.apply_pr_scroll_event(ScrollDir::PageDown);
                    }
                    PrFocus::RepoList => {}
                }
                true
            }
            AppEvent::PrNavigateHome => {
                match self.prs_state.pr_focus {
                    PrFocus::PrList => self.navigate_pr_list_home(),
                    PrFocus::PrDetail => {
                        self.prs_state.detail_scroll_offset = 0;
                    }
                    PrFocus::RepoList => {}
                }
                true
            }
            AppEvent::PrNavigateEnd => {
                match self.prs_state.pr_focus {
                    PrFocus::PrList => self.navigate_pr_list_end(),
                    PrFocus::PrDetail => {
                        self.prs_state.detail_scroll_offset = self.pr_detail_max_scroll_offset();
                    }
                    PrFocus::RepoList => {}
                }
                true
            }
            _ => false,
        }
    }

    /// Handle scroll-detail events (explicit scroll key bindings).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    fn apply_pr_scroll_detail_event(&mut self, event: &AppEvent) -> bool {
        let dir = match event {
            AppEvent::PrScrollDetailUp => ScrollDir::Up,
            AppEvent::PrScrollDetailDown => ScrollDir::Down,
            AppEvent::PrScrollDetailPageUp => ScrollDir::PageUp,
            AppEvent::PrScrollDetailPageDown => ScrollDir::PageDown,
            _ => return false,
        };
        self.apply_pr_scroll_event(dir);
        true
    }
}

/// Count all review threads across all reviews in a PR detail.
///
/// Threads are stored under each `PrReview.review_threads`; this flattens and
/// counts them to support the flat `PrDetailSubfocus::ReviewThread(usize)`
/// index used for navigation and rendering.
///
/// @plan PLAN-20260624-PR-MODE.P05
/// @requirement REQ-PR-009
pub(super) fn count_review_threads(detail: &crate::domain::PullRequestDetail) -> usize {
    detail
        .reviews
        .iter()
        .flat_map(|r| &r.review_threads)
        .count()
}
