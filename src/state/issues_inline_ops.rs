//! Issues-mode inline composer/editor state operations.

use crate::issue_detail_content::{ISSUE_REPLY_ANCHOR, build_detail_content};

use super::{
    AppEvent, AppState, ComposerTarget, DetailSubfocus, EditorTarget, InlineState, IssueFocus,
    inline_cursor_vertical,
};

impl AppState {
    /// Scroll the issue detail viewport so the last content line is visible.
    pub(crate) fn scroll_detail_to_bottom(&mut self) {
        self.issues_state.detail_scroll_offset = self.issues_state.max_detail_scroll_offset();
    }

    fn scroll_issue_detail_to_reply_anchor(&mut self) {
        let Some(detail) = self.issues_state.issue_detail.as_ref() else {
            return;
        };
        let content = build_detail_content(
            detail,
            self.issues_state.detail_subfocus,
            &self.issues_state.inline_state,
            self.issues_state.loading.comments,
        );
        let Some(anchor_line) = content
            .text
            .lines()
            .position(|line| line == ISSUE_REPLY_ANCHOR)
        else {
            return;
        };
        let viewport_rows = if self.issues_state.detail_viewport_rows == 0 {
            crate::layout::detail_viewport_rows(40)
        } else {
            self.issues_state.detail_viewport_rows
        };
        let document_rows = crate::layout::issue_detail_document_viewport_rows(viewport_rows, true);
        let desired = anchor_line.saturating_add(1).saturating_sub(document_rows);
        self.issues_state.detail_scroll_offset =
            desired.min(self.issues_state.max_detail_scroll_offset());
    }

    fn apply_inline_event_while_pending(&mut self, event: AppEvent) -> bool {
        if matches!(event, AppEvent::InlineCancelOrEsc) {
            self.issues_state.inline_state = InlineState::None;
            self.issues_state.mutation_pending = None;
            return true;
        }
        matches!(
            event,
            AppEvent::InlineChar(_)
                | AppEvent::InlineNewline
                | AppEvent::InlineBackspace
                | AppEvent::InlineDelete
                | AppEvent::InlineCursorLeft
                | AppEvent::InlineCursorRight
                | AppEvent::InlineCursorUp
                | AppEvent::InlineCursorDown
        )
    }

    fn active_inline_text(inline_state: &mut InlineState) -> Option<(&mut String, &mut usize)> {
        match inline_state {
            InlineState::Composer { text, cursor, .. }
            | InlineState::Editor { text, cursor, .. } => Some((text, cursor)),
            InlineState::None => None,
        }
    }

    fn insert_inline_char(inline_state: &mut InlineState, c: char) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state) {
            text.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
    }

    fn delete_inline_previous_char(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            text.drain((*cursor - prev)..*cursor);
            *cursor -= prev;
        }
    }

    fn delete_inline_next_char(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor < text.len()
        {
            let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
            text.drain(*cursor..(*cursor + next));
        }
    }

    fn move_inline_cursor_left(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor > 0
        {
            let prev = text[..*cursor].chars().last().map_or(0, char::len_utf8);
            *cursor -= prev;
        }
    }

    fn move_inline_cursor_right(inline_state: &mut InlineState) {
        if let Some((text, cursor)) = Self::active_inline_text(inline_state)
            && *cursor < text.len()
        {
            let next = text[*cursor..].chars().next().map_or(0, char::len_utf8);
            *cursor += next;
        }
    }

    pub(super) fn apply_inline_event(&mut self, event: AppEvent) -> bool {
        if self.issues_state.mutation_pending.is_some() {
            return self.apply_inline_event_while_pending(event);
        }
        match event {
            AppEvent::InlineChar(c) => {
                Self::insert_inline_char(&mut self.issues_state.inline_state, c);
            }
            AppEvent::InlineNewline => {
                Self::insert_inline_char(&mut self.issues_state.inline_state, char::from(0x0Au8));
            }
            AppEvent::InlineBackspace => {
                Self::delete_inline_previous_char(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineDelete => {
                Self::delete_inline_next_char(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorLeft => {
                Self::move_inline_cursor_left(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorRight => {
                Self::move_inline_cursor_right(&mut self.issues_state.inline_state);
            }
            AppEvent::InlineCursorUp => {
                if let Some((text, cursor)) =
                    Self::active_inline_text(&mut self.issues_state.inline_state)
                {
                    inline_cursor_vertical(text, cursor, -1);
                }
            }
            AppEvent::InlineCursorDown => {
                if let Some((text, cursor)) =
                    Self::active_inline_text(&mut self.issues_state.inline_state)
                {
                    inline_cursor_vertical(text, cursor, 1);
                }
            }
            AppEvent::InlineCancelOrEsc => self.issues_state.inline_state = InlineState::None,
            _ => return false,
        }
        true
    }

    fn open_reply_composer(&mut self, comment_index: usize) -> bool {
        if self.issues_state.inline_state != InlineState::None {
            return false;
        }
        let author = self
            .issues_state
            .issue_detail
            .as_ref()
            .and_then(|d| d.comments.get(comment_index))
            .map(|c| format!("@{} ", c.author_login))
            .unwrap_or_default();
        let cursor = author.len();
        self.issues_state.inline_state = InlineState::Composer {
            target: ComposerTarget::Reply {
                comment_index,
                author: author.clone(),
            },
            text: author,
            cursor,
        };
        true
    }

    fn open_inline_editor(&mut self, target: EditorTarget) {
        if self.issues_state.inline_state == InlineState::None {
            let text = match &target {
                EditorTarget::IssueBody => self
                    .issues_state
                    .issue_detail
                    .as_ref()
                    .map(|detail| format!("{}\n{}", detail.title, detail.body))
                    .unwrap_or_default(),
                EditorTarget::Comment { comment_index } => self
                    .issues_state
                    .issue_detail
                    .as_ref()
                    .and_then(|d| d.comments.get(*comment_index))
                    .map(|c| c.body.clone())
                    .unwrap_or_default(),
            };
            let cursor = text.len();
            self.issues_state.inline_state = InlineState::Editor {
                target,
                text,
                cursor,
            };
        }
    }

    pub(super) fn apply_inline_open_event(&mut self, event: AppEvent) -> bool {
        if self.issues_state.mutation_pending.is_some() {
            return matches!(
                event,
                AppEvent::OpenNewIssueComposer
                    | AppEvent::OpenNewCommentComposer
                    | AppEvent::OpenReplyComposer { .. }
                    | AppEvent::OpenInlineEditor { .. }
            );
        }
        // The match returns (handled, opened): `handled` means the event was
        // recognized and consumed; `opened` means a composer/editor actually
        // transitioned into an active state.
        let (handled, opened) = match event {
            AppEvent::OpenNewIssueComposer => {
                let opened = self.issues_state.inline_state == InlineState::None;
                if opened {
                    self.issues_state.issue_focus = IssueFocus::IssueList;
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewIssue,
                        text: String::new(),
                        cursor: 0,
                    };
                }
                (true, opened)
            }
            AppEvent::OpenNewCommentComposer => {
                let opened = self.issues_state.inline_state == InlineState::None;
                if opened {
                    self.issues_state.inline_state = InlineState::Composer {
                        target: ComposerTarget::NewComment,
                        text: String::new(),
                        cursor: 0,
                    };
                    self.issues_state.detail_subfocus = DetailSubfocus::NewComment;
                    self.scroll_detail_to_bottom();
                }
                (true, opened)
            }
            AppEvent::OpenReplyComposer { comment_index } => {
                let opened = self.open_reply_composer(comment_index);
                if opened {
                    self.scroll_issue_detail_to_reply_anchor();
                }
                (true, opened)
            }
            AppEvent::OpenInlineEditor { target } => {
                let opened = self.issues_state.inline_state == InlineState::None;
                if opened {
                    self.open_inline_editor(target);
                }
                (true, opened)
            }
            _ => (false, false),
        };
        // Issue #265: clear a stale non-blocking notice (e.g. "No agents
        // available") ONLY when a composer/editor actually opens. A blocked
        // attempt (already-active inline state) must preserve the notice.
        // This does NOT touch the real `error`.
        if opened {
            self.issues_state.draft_notice = None;
        }
        handled
    }
}
