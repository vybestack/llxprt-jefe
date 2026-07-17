//! Issue detail pane projection.
//!
//! The pure header projection ([`IssueDetailHeaderView`] /
//! [`issue_detail_header_view`]) is shared with the selection content provider
//! so copied text matches the rendered rows. The full-pane projection
//! ([`issue_detail_props`]) computes all layout math + semantic colors and
//! builds a [`DetailPaneProps`] that the generic [`DetailPane`] renders via
//! [`detail_pane_element`]. This module stays iocraft-free (pure-views
//! pattern): it never touches `Color` — only semantic [`DetailHeaderColor`]
//! roles resolved by the component.

use crate::domain::{IssueDetail, IssueState};
use crate::issue_detail_content::{DetailContent, build_detail_content, build_new_issue_content};
use crate::layout::DETAIL_HEADER_ROWS as HEADER_ROWS;
use crate::selection::{SelectablePane, TextSelection};
use crate::state::{ComposerTarget, DetailSubfocus, InlineState};
use crate::theme::ThemeColors;

use super::detail_pane::{
    DetailComposerProps, DetailHeaderColor, DetailHeaderRow, DetailPaneProps,
    composer_from_inline_state,
};

/// Projected issue detail header exactly as the component renders it (the four
/// fixed metadata rows). The component delegates to this so the selection
/// content provider (`src/selection/content.rs`) and the renderer share the
/// same single source of truth, preventing drift.
pub struct IssueDetailHeaderView {
    /// "#number title" row.
    pub title: String,
    /// State tag + author/created/updated row.
    pub state: String,
    /// "labels: ...  assignees: ...  milestone: ..." row.
    pub labels: String,
    /// Display-only external URL row.
    pub url: String,
}

/// Pure projection of the issue detail's four fixed header rows exactly as the
/// component renders them.
///
/// Issue #155 redesign (shared with the PR detail header): human dates replace
/// raw ISO timestamps, empty fields show `—` instead of `-`.
#[must_use]
pub fn issue_detail_header_view(detail: &IssueDetail) -> IssueDetailHeaderView {
    let state_tag = match detail.state {
        IssueState::Open => "OPEN".to_string(),
        IssueState::Closed => match detail.state_reason {
            Some(reason) => format!("CLOSED ({})", reason.label()),
            None => "CLOSED".to_string(),
        },
    };
    let created = crate::ui::util::format_iso_date(&detail.created_at);
    let updated = crate::ui::util::format_iso_date(&detail.updated_at);
    let labels_str = crate::ui::util::field_list(&detail.labels);
    let assignees_str = crate::ui::util::field_list(&detail.assignees);
    let milestone_str = crate::ui::util::field_opt(detail.milestone.as_deref());
    IssueDetailHeaderView {
        title: format!("#{} {}", detail.number, detail.title),
        state: format!(
            "{state_tag}  by @{}  opened: {created}  updated: {updated}",
            detail.author_login
        ),
        labels: format!(
            "labels: {labels_str}  assignees: {assignees_str}  milestone: {milestone_str}"
        ),
        url: detail.external_url.clone(),
    }
}

/// Inputs the Issues screen passes to [`issue_detail_props`], bundled to stay
/// under the clippy::too_many-arguments threshold (max 6).
pub struct IssueDetailProjectionInputs<'a> {
    /// Full issue detail (metadata, body, comments).
    pub issue_detail: Option<&'a IssueDetail>,
    /// Which sub-element is focused within the detail view.
    pub detail_subfocus: DetailSubfocus,
    /// Active inline editor/composer state.
    pub inline_state: &'a InlineState,
    /// Whether comments are loading.
    pub comments_loading: bool,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Scroll offset for the content viewport.
    pub scroll_offset: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Actual available height (in terminal rows) for the detail pane.
    pub available_height: Option<u16>,
    /// Actual available width (in terminal columns) for detail/composer text.
    pub available_width: Option<u16>,
    /// Active text selection, if any (and if it targets this pane).
    pub selection: Option<TextSelection>,
}

/// Semantic state-row color for an issue: `Bright` when Open, `Dim` when Closed.
/// Matches the pre-refactor component (`rc.bright` / `rc.dim`).
fn issue_state_color(state: IssueState) -> DetailHeaderColor {
    match state {
        IssueState::Open => DetailHeaderColor::Bright,
        IssueState::Closed => DetailHeaderColor::Dim,
    }
}

/// Compute the detail viewport rows (scrollable area) from the available pane
/// height. Mirrors the pre-refactor component's height-derived viewport math.
fn detail_viewport_rows(available_height: Option<u16>) -> usize {
    if let Some(height) = available_height {
        crate::layout::detail_body_viewport_rows(usize::from(height))
    } else {
        let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
        crate::layout::detail_viewport_rows(term_rows)
    }
}

/// Build the five fixed header rows (title, state, labels, url, separator) with
/// their semantic colors and selection-line indices.
fn build_header_rows(
    h_title: String,
    h_state: String,
    h_labels: String,
    h_url: String,
    state_color: DetailHeaderColor,
) -> Vec<DetailHeaderRow> {
    vec![
        DetailHeaderRow {
            content: h_title,
            color: DetailHeaderColor::Fg,
            line: 0,
        },
        DetailHeaderRow {
            content: h_state,
            color: state_color,
            line: 1,
        },
        DetailHeaderRow {
            content: h_labels,
            color: DetailHeaderColor::Dim,
            line: 2,
        },
        DetailHeaderRow {
            content: h_url,
            color: DetailHeaderColor::Dim,
            line: 3,
        },
        DetailHeaderRow {
            content: super::SEPARATOR_LINE.to_string(),
            color: DetailHeaderColor::Dim,
            line: 4,
        },
    ]
}

/// Resolve the content + header for the "new issue composer" branch (title
/// "New Issue", state "Draft", bright state color).
fn new_issue_composer_content(inline_state: &InlineState) -> (Vec<DetailHeaderRow>, DetailContent) {
    let rows = build_header_rows(
        "New Issue".to_string(),
        "Draft".to_string(),
        String::new(),
        String::new(),
        DetailHeaderColor::Bright,
    );
    (rows, build_new_issue_content(inline_state))
}

/// Resolve the content + header for a loaded issue detail.
fn loaded_issue_content(
    detail: &IssueDetail,
    detail_subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> (Vec<DetailHeaderRow>, DetailContent) {
    let header = issue_detail_header_view(detail);
    let rows = build_header_rows(
        header.title,
        header.state,
        header.labels,
        header.url,
        issue_state_color(detail.state),
    );
    (
        rows,
        build_detail_content(detail, detail_subfocus, inline_state, comments_loading),
    )
}

/// Resolve the content + header for the "no issue selected" placeholder branch.
fn empty_issue_content() -> (Vec<DetailHeaderRow>, DetailContent) {
    let rows = build_header_rows(
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        DetailHeaderColor::Dim,
    );
    (
        rows,
        DetailContent {
            text: "No issue selected".to_string(),
            cursor: None,
        },
    )
}

/// Resolve the issue detail content width (terminal cols) from the supplied
/// width, falling back to the terminal size when the parent did not supply one.
fn detail_content_width(available_width: Option<u16>) -> usize {
    usize::from(available_width.unwrap_or_else(|| {
        crate::layout::issues_detail_content_width(
            crossterm::terminal::size().map_or(120, |(w, _)| w),
        )
    }))
}

/// Compute the `(scroll_rows, composer_rows)` split for the issue detail pane
/// given the total detail viewport rows, the reserved document rows, the
/// document line count, and whether a composer is active.
fn issue_scroll_composer_rows(
    detail_viewport_rows: usize,
    reserved_document_rows: usize,
    document_line_count: usize,
    composer_active: bool,
) -> (usize, usize) {
    let scroll_rows = if composer_active {
        reserved_document_rows.min(document_line_count)
    } else {
        reserved_document_rows
    };
    let composer_rows = if composer_active {
        crate::layout::DETAIL_COMPOSER_VIEWPORT_ROWS
            .min(detail_viewport_rows.saturating_sub(scroll_rows))
    } else {
        0
    };
    (scroll_rows, composer_rows)
}

/// Pure projection of the issue detail pane into a [`DetailPaneProps`].
///
/// Encapsulates ALL the logic the pre-refactor `IssueDetailView` component body
/// owned: the new-issue-composer / loaded-detail / empty branching, the
/// viewport/composer row math, the semantic state color, and the composer
/// extraction. The result is rendered byte-identically by the generic
/// [`DetailPane`] via [`detail_pane_element`].
///
/// This function is iocraft-free: it emits semantic [`DetailHeaderColor`] roles
/// (never concrete `Color`), keeping the pure-views invariant.
#[must_use]
pub fn issue_detail_props(inputs: IssueDetailProjectionInputs<'_>) -> DetailPaneProps {
    let detail_vp_rows = detail_viewport_rows(inputs.available_height);
    let composer = composer_from_inline_state(inputs.inline_state);
    let composer_active = composer.is_some();
    let reserved_document_rows =
        crate::layout::issue_detail_document_viewport_rows(detail_vp_rows, composer_active);
    let content_width = detail_content_width(inputs.available_width);

    let showing_new_issue_composer = matches!(
        inputs.inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewIssue,
            ..
        }
    );

    let (header_rows, detail_content) = if showing_new_issue_composer {
        new_issue_composer_content(inputs.inline_state)
    } else if let Some(detail) = inputs.issue_detail {
        loaded_issue_content(
            detail,
            inputs.detail_subfocus,
            inputs.inline_state,
            inputs.comments_loading,
        )
    } else {
        empty_issue_content()
    };

    let document_line_count = detail_content.text.lines().count().max(1);
    let (scroll_rows, composer_rows) = issue_scroll_composer_rows(
        detail_vp_rows,
        reserved_document_rows,
        document_line_count,
        composer_active,
    );
    let composer_props = composer.map(|(text, byte_cursor, prefix)| DetailComposerProps {
        text,
        byte_cursor,
        content_width,
        prefix,
    });

    DetailPaneProps {
        header_rows,
        content: detail_content.text,
        content_cursor: detail_content.cursor,
        scroll_offset: inputs.scroll_offset,
        viewport_rows: scroll_rows,
        content_line_offset: HEADER_ROWS,
        max_line_width: content_width,
        focused: inputs.focused,
        pane: SelectablePane::IssueDetail,
        colors: inputs.colors,
        selection: inputs.selection,
        composer: composer_props,
        composer_rows,
    }
}

#[cfg(test)]
mod tests {
    use super::{DetailContent, build_new_issue_content, issue_detail_header_view};
    use crate::domain::{IssueDetail, IssueState, IssueStateReason};
    use crate::state::{ComposerTarget, InlineState};

    fn detail_with_state(state: IssueState, state_reason: Option<IssueStateReason>) -> IssueDetail {
        IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 42,
            node_id: String::new(),
            title: "Test".to_string(),
            state,
            author_login: "alice".to_string(),
            created_at: "2026-01-01".to_string(),
            updated_at: "2026-01-02".to_string(),
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            body: String::new(),
            external_url: "https://github.com/owner/repo/issues/42".to_string(),
            comments: crate::domain::PaginatedList::default(),
            issue_type_name: None,
            state_reason,
        }
    }

    #[test]
    fn header_shows_closed_with_reason_in_state_row() {
        let detail = detail_with_state(IssueState::Closed, Some(IssueStateReason::NotPlanned));
        let view = issue_detail_header_view(&detail);
        assert!(
            view.state.contains("CLOSED (not planned)"),
            "closed-with-reason detail should show reason: {}",
            view.state
        );
    }

    #[test]
    fn header_shows_closed_duplicate_reason() {
        let detail = detail_with_state(IssueState::Closed, Some(IssueStateReason::Duplicate));
        let view = issue_detail_header_view(&detail);
        assert!(
            view.state.contains("CLOSED (duplicate)"),
            "duplicate close detail should show reason: {}",
            view.state
        );
    }

    #[test]
    fn header_shows_plain_closed_without_reason() {
        let detail = detail_with_state(IssueState::Closed, None);
        let view = issue_detail_header_view(&detail);
        assert!(
            view.state.contains("CLOSED"),
            "closed-without-reason detail should show CLOSED: {}",
            view.state
        );
        assert!(!view.state.contains("CLOSED ("));
    }

    #[test]
    fn header_shows_closed_completed_reason() {
        let detail = detail_with_state(IssueState::Closed, Some(IssueStateReason::Completed));
        let view = issue_detail_header_view(&detail);
        assert!(
            view.state.contains("CLOSED (completed)"),
            "completed close should show CLOSED (completed): {}",
            view.state
        );
    }

    #[test]
    fn header_shows_open_without_reason() {
        let detail = detail_with_state(IssueState::Open, None);
        let view = issue_detail_header_view(&detail);
        assert!(view.state.contains("OPEN"));
    }

    #[test]
    fn build_new_issue_content_renders_static_prompt_only() {
        let inline = InlineState::Composer {
            target: ComposerTarget::NewIssue,
            text: "Issue title\nIssue body".to_string(),
            cursor: "Issue title\nIssue body".len(),
        };

        let DetailContent { text, cursor } = build_new_issue_content(&inline);

        assert!(text.contains("New Issue"));
        assert!(text.contains("Title: first line | Body: remaining lines"));
        assert!(text.contains("Alt+Enter submit | Esc cancel"));
        // The editor text is rendered by the embedded wrapping TextBox, so it
        // must NOT be flattened into the read-only document (issue #212).
        assert!(
            !text.contains("Issue title"),
            "editor text must not be flattened into the document: {text}"
        );
        assert!(
            cursor.is_none(),
            "the TextBox owns the caret; the document must carry no cursor"
        );
    }
}
