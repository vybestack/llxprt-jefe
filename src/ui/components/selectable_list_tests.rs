//! Tests for [`SelectableList`] and the domain projection wrappers.
//!
//! These lock two layers of the reusable-list contract:
//! 1. The Issue and PR projection wrappers match their pure visible-row
//!    projections.
//! 2. Rendered ANSI preserves shared border, weight, color, empty-state, and
//!    drag-selection behavior.

use super::*;
use crate::domain::{Issue, IssueState, PrCheckStatus, PrState, PullRequest};
use crate::theme::ThemeColors;
use crate::ui::components::issue_list::{IssueListLayout, IssueListWindow, issue_list_props};
use crate::ui::components::pr_list::{PrListLayout, PrListWindow, pr_list_props};

/// Render a `SelectableList` element into an ANSI string at a fixed size.
fn render_ansi(props: SelectableListProps, cols: u16, rows: u16) -> String {
    let mut elem = element! {
        Box(width: u32::from(cols), height: u32::from(rows)) {
            #(vec![selectable_list_element(props)])
        }
    };
    let canvas = elem.render(Some(usize::from(cols)));
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    String::from_utf8_lossy(&buf).into_owned()
}

fn issue(n: u64) -> Issue {
    Issue {
        number: n,
        node_id: String::new(),
        title: format!("Issue {n}"),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2026-06-30".to_string(),
        updated_at: "2026-06-30".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
        priority: None,
        linked_pr_numbers: Vec::new(),
    }
}

fn pr(n: u64) -> PullRequest {
    PullRequest {
        number: n,
        title: format!("PR {n}"),
        state: PrState::Open,
        author_login: "octocat".to_string(),
        created_at: "2026-01-01".to_string(),
        updated_at: "2026-01-01".to_string(),
        head_ref: "feature".to_string(),
        head_sha: String::new(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        mergeable: None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

// ── Projection parity ───────────────────────────────────────────────────

/// `issue_list_props` rows' first span text must equal the unchanged
/// `issue_list_visible_rows` `title_line` (parity with the pure projection).
#[test]
fn issue_list_props_first_span_matches_visible_rows_title_line() {
    let issues: Vec<Issue> = (1..=3).map(issue).collect();
    let window = IssueListWindow {
        selected_index: Some(1),
        list_pane_rows: 10,
        layout: IssueListLayout::Compact,
        available_width: Some(40),
    };
    let props = issue_list_props(&issues, window, true, None, ThemeColors::default(), None);
    let visible = crate::ui::components::issue_list::issue_list_visible_rows(
        &issues,
        Some(1),
        10,
        IssueListLayout::Compact,
        Some(40),
    );
    assert_eq!(props.rows.len(), visible.len());
    for (got, want) in props.rows.iter().zip(visible.iter()) {
        assert_eq!(got.spans.len(), 1);
        assert_eq!(got.spans[0].text, want.title_line);
        assert!(got.meta_line.as_deref() == Some("") || got.meta_line.is_some());
        assert_eq!(got.is_selected, want.is_selected);
    }
}

/// `pr_list_props` rows' first span text must equal the unchanged
/// `pr_list_visible_rows` `title_line`, and compact rows carry an empty
/// meta line (signaling single-line rendering).
#[test]
fn pr_list_props_first_span_matches_visible_rows_title_line() {
    let prs: Vec<PullRequest> = (1..=3).map(pr).collect();
    let window = PrListWindow {
        selected_index: Some(0),
        list_pane_rows: 10,
        available_width: Some(40),
        layout: PrListLayout::Compact,
    };
    let props = pr_list_props(&prs, window, true, None, ThemeColors::default(), None);
    let visible = crate::ui::components::pr_list::pr_list_visible_rows(&prs, Some(0), 10, Some(40));
    assert_eq!(props.rows.len(), visible.len());
    for (got, want) in props.rows.iter().zip(visible.iter()) {
        assert_eq!(got.spans.len(), 1);
        assert_eq!(got.spans[0].text, want.title_line);
        // Compact → empty meta string (single-line row).
        assert_eq!(got.meta_line.as_deref(), Some(""));
        assert_eq!(got.is_selected, want.is_selected);
    }
}

// ── Render-canvas (ANSI) identity ───────────────────────────────────────

/// BoldSelected + compact issue row: the selected row's title renders bold
/// (`\e[1m`), and the title text appears in the output.
#[test]
fn bold_selected_compact_issue_row_renders_bold_when_selected() {
    let issues: Vec<Issue> = (1..=2).map(issue).collect();
    let window = IssueListWindow {
        selected_index: Some(0),
        list_pane_rows: 8,
        layout: IssueListLayout::Compact,
        available_width: Some(40),
    };
    let props = issue_list_props(&issues, window, true, None, ThemeColors::default(), None);
    let ansi = render_ansi(props, 40, 8);
    assert!(ansi.contains("Issue 1"), "title text must appear: {ansi}");
    assert!(
        ansi.contains("\u{1b}[1m"),
        "selected row must render bold (SGR 1): {ansi}"
    );
}

/// An empty-message list renders the message text in the dim color and no
/// row text appears.
#[test]
fn empty_message_renders_in_dim() {
    let props = SelectableListProps {
        title: "Issues".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("No issues found".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::IssueList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 20,
    };
    let ansi = render_ansi(props, 40, 8);
    assert!(
        ansi.contains("No issues found"),
        "empty message must render"
    );
    // Green-screen dim = #6a9955 → RGB 106,153,85 (accent_secondary).
    assert!(
        ansi.contains("\u{1b}[38;2;106;153;85m"),
        "empty message must render in dim color: {ansi}"
    );
}

/// DoubleOnFocus border: a focused list renders a double border (`╔`), an
/// unfocused one renders a round border (`╭`).
#[test]
fn double_on_focus_border_switches_on_focus() {
    let base = || SelectableListProps {
        title: "Issues".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("x".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::IssueList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 20,
    };
    let unfocused = render_ansi(base(), 20, 6);
    let mut focused = base();
    focused.focused = true;
    let focused = render_ansi(focused, 20, 6);
    assert!(
        unfocused.contains('╭'),
        "unfocused DoubleOnFocus must use round border: {unfocused}"
    );
    assert!(
        focused.contains('╔'),
        "focused DoubleOnFocus must use double border: {focused}"
    );
}

/// RoundFocusedColor border (Agent): always round (`╭`), focused or not.
#[test]
fn round_focused_color_border_always_round() {
    let base = || SelectableListProps {
        title: "Agents".to_string(),
        rows: Vec::new(),
        focused: false,
        empty_message: Some("x".to_string()),
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::AgentList,
        border: ListBorder::RoundFocusedColor,
        content_padding: true,
        selection_style: SelectionStyle::BrightSelected,
        content_width: 16,
    };
    let unfocused = render_ansi(base(), 20, 6);
    let mut focused = base();
    focused.focused = true;
    let focused = render_ansi(focused, 20, 6);
    assert!(
        unfocused.contains('╭') && focused.contains('╭'),
        "RoundFocusedColor must always use round border"
    );
    // Focused border color brightens to border_focused (#00ff00 → 0;255;0).
    assert!(
        focused.contains("\u{1b}[38;2;0;255;0m"),
        "focused RoundFocusedColor border must use border_focused color: {focused}"
    );
}

fn strip_ansi(ansi: &str) -> String {
    let mut plain = String::with_capacity(ansi.len());
    let mut chars = ansi.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for sequence_character in chars.by_ref() {
                if sequence_character.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

fn right_border_columns(ansi: &str, border: char) -> Vec<usize> {
    strip_ansi(ansi)
        .lines()
        .filter_map(|line| {
            let byte_index = line.rfind(border)?;
            Some(unicode_width::UnicodeWidthStr::width(&line[..byte_index]))
        })
        .collect()
}

#[test]
fn long_unicode_rows_keep_border_column_stable_for_nonzero_windows() {
    let rows = vec![
        SelectableRow {
            source_index: 5,
            spans: vec![
                SelectableSpan {
                    text: String::from("> １２very-long-selected-title"),
                    color: SpanColor::Themed,
                },
                SelectableSpan {
                    text: String::from(" suffix-that-must-fit"),
                    color: SpanColor::Role(SpanRole::Dim),
                },
            ],
            meta_line: None,
            is_selected: true,
        },
        SelectableRow {
            source_index: 6,
            spans: vec![SelectableSpan {
                text: String::from("  unselected-１２-long-title"),
                color: SpanColor::Themed,
            }],
            meta_line: None,
            is_selected: false,
        },
    ];
    let props = SelectableListProps {
        title: String::from("Unicode window"),
        rows,
        focused: false,
        empty_message: None,
        colors: ThemeColors::default(),
        selection: None,
        pane: SelectablePane::AgentList,
        border: ListBorder::RoundFocusedColor,
        content_padding: true,
        selection_style: SelectionStyle::BrightSelected,
        content_width: 12,
    };
    let ansi = render_ansi(props, 16, 8);

    let border_columns = right_border_columns(&ansi, '│');
    assert_eq!(border_columns, vec![15; 6], "{ansi}");
    assert!(
        ansi.contains('…'),
        "long Unicode rows must be truncated: {ansi}"
    );
}

#[test]
fn fitting_adds_ellipsis_when_an_earlier_span_fills_the_width() {
    let row = SelectableRow {
        source_index: 0,
        spans: vec![
            SelectableSpan {
                text: String::from("> *"),
                color: SpanColor::Themed,
            },
            SelectableSpan {
                text: String::from(" trailing"),
                color: SpanColor::Role(SpanRole::Dim),
            },
        ],
        meta_line: None,
        is_selected: true,
    };

    let fitted = fit_row(&row, 3);
    let text: String = fitted.spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(text, "> …");
    assert!(
        fitted
            .spans
            .last()
            .is_some_and(|span| span.color == SpanColor::Themed)
    );
}

#[test]
fn title_only_drag_selection_does_not_highlight_metadata_background() {
    let pane = SelectablePane::IssueList;
    let selection = TextSelection {
        anchor: crate::selection::SelectionPoint::new(pane, 0, 0),
        focus: crate::selection::SelectionPoint::new(pane, 0, 8),
    };
    let row = SelectableRow {
        source_index: 0,
        spans: vec![SelectableSpan {
            text: String::from("title-row"),
            color: SpanColor::Themed,
        }],
        meta_line: Some(String::from("meta-row")),
        is_selected: false,
    };
    let colors = ThemeColors {
        selection_bg: String::from("#112233"),
        ..ThemeColors::default()
    };
    let props = SelectableListProps {
        title: String::from("Rows"),
        rows: vec![row],
        focused: false,
        empty_message: None,
        colors,
        selection: Some(selection),
        pane,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 12,
    };

    let ansi = render_ansi(props, 14, 6);
    assert_eq!(ansi.matches("48;2;17;34;51m").count(), 1, "{ansi}");
}

#[test]
fn metadata_only_drag_selection_does_not_highlight_title_background() {
    let pane = SelectablePane::IssueList;
    let selection = TextSelection {
        anchor: crate::selection::SelectionPoint::new(pane, 1, 0),
        focus: crate::selection::SelectionPoint::new(pane, 1, 7),
    };
    let row = SelectableRow {
        source_index: 0,
        spans: vec![SelectableSpan {
            text: String::from("title-row"),
            color: SpanColor::Themed,
        }],
        meta_line: Some(String::from("meta-row")),
        is_selected: false,
    };
    let colors = ThemeColors {
        selection_bg: String::from("#112233"),
        ..ThemeColors::default()
    };
    let props = SelectableListProps {
        title: String::from("Rows"),
        rows: vec![row],
        focused: false,
        empty_message: None,
        colors,
        selection: Some(selection),
        pane,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: 12,
    };

    let ansi = render_ansi(props, 14, 6);
    assert_eq!(ansi.matches("48;2;17;34;51m").count(), 1, "{ansi}");
}

#[test]
fn drag_highlight_uses_window_local_rows_not_source_indices() {
    let pane = SelectablePane::Sidebar;
    let first = TextSelection {
        anchor: crate::selection::SelectionPoint::new(pane, 0, 0),
        focus: crate::selection::SelectionPoint::new(pane, 0, 4),
    };
    let last = TextSelection {
        anchor: crate::selection::SelectionPoint::new(pane, 19, 0),
        focus: crate::selection::SelectionPoint::new(pane, 19, 4),
    };

    assert!(row_is_highlighted(Some(&first), pane, 0));
    assert!(!row_is_highlighted(Some(&first), pane, 5));
    assert!(row_is_highlighted(Some(&last), pane, 19));
}
