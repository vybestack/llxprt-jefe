//! Pull Requests mode inline composer state operations.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-010
//! @pseudocode component-001 lines 292-330

use super::{
    AppEvent, AppState, ComposerTarget, InlineState, PrDetailSubfocus, PrFocus, ReadOnlyHintKind,
};
use crate::messages::PrInlineMsg;

/// Convert the stored `detail_content_width` into the `Option<usize>` the
/// builder expects: 0 (the default before the dispatch boundary sets it) means
/// "no wrapping" so reducers stay deterministic and crossterm-free.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
pub(super) fn wrap_width_from_state(detail_content_width: usize) -> Option<usize> {
    (detail_content_width > 0).then_some(detail_content_width)
}

impl AppState {
    /// Scroll the PR detail viewport so the last content line is visible.
    ///
    /// Shared by the inline composer-open path and the post-comment-create path
    /// so both land on the SAME offset the scroll clamp uses (the real rendered
    /// bottom), keeping the new content on-screen and preventing a later
    /// page-down jump (#56).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 169-176
    pub(super) fn scroll_pr_detail_to_bottom(&mut self) {
        self.prs_state.detail_scroll_offset = self.pr_max_detail_scroll_offset();
    }

    /// Compute the maximum detail scroll offset from the REAL rendered content
    /// length (the exact text `build_pr_detail_content` emits for the current
    /// subfocus and inline composer state) minus the viewport prop.
    ///
    /// Using the shared `pr_detail_content_line_count` parity function — rather
    /// than a heuristic — guarantees that scrolling to the bottom on composer
    /// open lands on the same offset the scroll clamp uses, so the composer
    /// stays on-screen and a later page-down does not jump (#56).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    fn pr_max_detail_scroll_offset(&self) -> usize {
        let Some(detail) = &self.prs_state.pr_detail else {
            return 0;
        };
        crate::pr_detail_content::pr_detail_content_line_count(
            detail,
            self.prs_state.detail_subfocus,
            &self.prs_state.inline_state,
            self.prs_state.loading.detail,
            self.prs_state.loading.comments,
            wrap_width_from_state(self.prs_state.detail_content_width),
        )
        .saturating_sub(self.prs_state.detail_viewport_rows)
    }

    /// Borrow the active (text, cursor) pair from the inline composer/editor.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 308-330
    fn pr_active_inline_text(inline_state: &mut InlineState) -> Option<(&mut String, &mut usize)> {
        match inline_state {
            InlineState::Composer { text, cursor, .. }
            | InlineState::Editor { text, cursor, .. } => Some((text, cursor)),
            InlineState::None => None,
        }
    }

    /// Insert a character at the cursor position and advance the cursor.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 44-50
    fn pr_insert_inline_char(inline_state: &mut InlineState, c: char) {
        if let Some((text, cursor)) = Self::pr_active_inline_text(inline_state) {
            text.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
    }

    /// Delete the character before the cursor (backspace) and retreat the cursor.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 44-50
    fn pr_delete_inline_previous_char(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::pr_active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            text.drain((*cursor - prev)..*cursor);
            *cursor -= prev;
        }
    }

    /// Move the cursor one character to the left.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 44-50
    fn pr_move_inline_cursor_left(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::pr_active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            *cursor -= prev;
        }
    }

    /// Move the cursor one character to the right.
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 44-50
    fn pr_move_inline_cursor_right(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::pr_active_inline_text(inline_state)
            && *cursor < text.len()
        {
            let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
            *cursor += next;
        }
    }

    /// Open the new-comment composer (sets subfocus to NewComment + follow viewport).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 292-298
    fn pr_open_new_comment_composer(&mut self) {
        if self.prs_state.pr_focus != PrFocus::PrDetail {
            return;
        }
        if self.prs_state.inline_state != InlineState::None {
            return;
        }
        self.prs_state.inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: String::new(),
            cursor: 0,
        };
        self.prs_state.detail_subfocus = PrDetailSubfocus::NewComment;
        self.scroll_pr_detail_to_bottom();
    }

    /// Open the reply composer (prefill with @author).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 299-307
    fn pr_open_reply_composer(&mut self, comment_index: usize) {
        if self.prs_state.pr_focus != PrFocus::PrDetail {
            return;
        }
        if !matches!(self.prs_state.detail_subfocus, PrDetailSubfocus::Comment(_)) {
            self.apply_pr_show_notice(ReadOnlyHintKind::ReadOnlyReplyOnComment);
            return;
        }
        if self.prs_state.inline_state != InlineState::None {
            return;
        }
        let author = self
            .prs_state
            .pr_detail
            .as_ref()
            .and_then(|d| d.comments.get(comment_index))
            .map(|c| format!("@{} ", c.author_login))
            .unwrap_or_default();
        let cursor = author.len();
        self.prs_state.inline_state = InlineState::Composer {
            target: ComposerTarget::Reply {
                comment_index,
                author: author.clone(),
            },
            text: author,
            cursor,
        };
        self.scroll_pr_detail_to_bottom();
    }

    /// Apply inline-open events (OpenNewCommentComposer, OpenReplyComposer).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 292-307
    pub(super) fn apply_pr_inline_open_event(&mut self, event: AppEvent) -> bool {
        if self.prs_state.mutation_pending.is_some() {
            return matches!(
                event,
                AppEvent::PrOpenNewCommentComposer | AppEvent::PrOpenReplyComposer { .. }
            );
        }
        match event {
            AppEvent::PrOpenNewCommentComposer => self.pr_open_new_comment_composer(),
            AppEvent::PrOpenReplyComposer { comment_index } => {
                self.pr_open_reply_composer(comment_index);
            }
            _ => return false,
        }
        true
    }

    /// Apply inline editor events (char/newline/backspace/cursor/cancel).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @pseudocode component-001 lines 308-330
    pub(super) fn apply_pr_inline_event(&mut self, msg: PrInlineMsg) -> bool {
        if self.prs_state.mutation_pending.is_some() {
            // While a mutation is in flight, text-edit keys are consumed but
            // ignored so the in-flight draft is not mutated. CancelOrEsc,
            // however, MUST still work: it closes the composer AND clears the
            // pending mutation (cancel the intent at the state level). The
            // dispatch layer tolerates a dropped mutation result: the
            // completion handlers (apply_pr_comment_created /
            // apply_pr_comment_create_failed) guard on mutation_pending
            // matching (scope + mutation_id) and no-op when it is None.
            if msg == PrInlineMsg::CancelOrEsc {
                self.prs_state.inline_state = InlineState::None;
                self.prs_state.mutation_pending = None;
                return true;
            }
            return matches!(
                msg,
                PrInlineMsg::Char(_)
                    | PrInlineMsg::Newline
                    | PrInlineMsg::Backspace
                    | PrInlineMsg::Delete
                    | PrInlineMsg::CursorLeft
                    | PrInlineMsg::CursorRight
                    | PrInlineMsg::CursorUp
                    | PrInlineMsg::CursorDown
            );
        }
        match msg {
            PrInlineMsg::Char(c) => {
                Self::pr_insert_inline_char(&mut self.prs_state.inline_state, c);
            }
            PrInlineMsg::Newline => {
                Self::pr_insert_inline_char(&mut self.prs_state.inline_state, char::from(0x0Au8));
            }
            PrInlineMsg::Backspace => {
                Self::pr_delete_inline_previous_char(&mut self.prs_state.inline_state);
            }
            PrInlineMsg::Delete | PrInlineMsg::CursorUp | PrInlineMsg::CursorDown => {}
            PrInlineMsg::CursorLeft => {
                Self::pr_move_inline_cursor_left(&mut self.prs_state.inline_state);
            }
            PrInlineMsg::CursorRight => {
                Self::pr_move_inline_cursor_right(&mut self.prs_state.inline_state);
            }
            PrInlineMsg::Submit => {
                self.pr_inline_submit();
            }
            PrInlineMsg::CancelOrEsc => {
                self.prs_state.inline_state = InlineState::None;
            }
        }
        true
    }

    /// Apply inline submit (blank text cancels; non-blank sets mutation pending).
    ///
    /// @plan PLAN-20260624-PR-MODE.P05
    /// @requirement REQ-PR-010
    /// @requirement REQ-PR-013
    /// @pseudocode component-001 lines 308-315
    fn pr_inline_submit(&mut self) {
        let (text, target) = match &self.prs_state.inline_state {
            InlineState::Composer { text, target, .. } => (text.clone(), target.clone()),
            // An Editor is never reachable in PR mode (no PR path sets
            // InlineState::Editor — only issues_inline_ops does). Rather than
            // silently fabricate a NewComment mutation, reject it explicitly
            // with a visible error so the misroute cannot produce a bogus
            // comment create.
            InlineState::Editor { .. } => {
                self.prs_state.error =
                    Some("Editor submit is not available in PR mode".to_string());
                self.prs_state.inline_state = InlineState::None;
                return;
            }
            InlineState::None => return,
        };
        if text.trim().is_empty() {
            self.prs_state.inline_state = InlineState::None;
            return;
        }
        // Set mutation pending — the dispatch layer spawns the actual create.
        let scope_repo_id = self.selected_repository_id().cloned();
        if let Some(scope) = scope_repo_id {
            let mutation_id = self.prs_state.next_mutation_id.saturating_add(1);
            self.prs_state.next_mutation_id = mutation_id;
            self.prs_state.mutation_pending = Some(crate::state::types::PrMutationPending {
                scope_repo_id: scope,
                mutation_id,
                target,
            });
        } else {
            // No repository selected: surface a visible error and close the
            // composer rather than silently freezing it open.
            self.prs_state.error = Some("No repository selected".to_string());
            self.prs_state.inline_state = InlineState::None;
        }
    }
}
