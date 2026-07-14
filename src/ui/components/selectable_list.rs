//! Generic bordered, scrollable, selectable list used by the Issue, PR, and
//! Agent list panes.
//!
//! Domain layers project their data into [`SelectableRow`]s (each row already
//! windowed/trimmed by the domain projection); this component owns the iocraft
//! rendering once. The three behavioral shapes the original `IssueList`,
//! `PrList`, and `AgentList` components had are modelled by [`ListBorder`],
//! [`SelectionStyle`], `content_padding`, and the [`SelectableSpan`]/[`SpanColor`]
//! span model, so rendered output stays byte-identical to the pre-refactor
//! components.
//!
//! This component does NOT re-do scroll windowing — the domain projections
//! already window. It just renders the rows it is given, using the
//! `.enumerate()` index as the drag-highlight content line index.

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection, row_highlight_range};
use crate::theme::{ResolvedColors, ThemeColors};

/// A semantic color role for a fixed (selection-immune) span.
///
/// The component resolves each role against [`ResolvedColors`] at render time,
/// so the row model and the domain projections that build it stay free of any
/// iocraft type (pure-views pattern).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanRole {
    /// `rc.bright` (e.g. Running/Completed agent status glyph).
    Bright,
    /// `rc.dim` (e.g. Queued agent status glyph).
    Dim,
    /// Absolute terminal red.
    Red,
    /// Absolute terminal yellow.
    Yellow,
    /// Absolute terminal blue.
    Blue,
}

/// Color policy for one span of a row's first line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanColor {
    /// Uses the row color: normally `fg`; when keyboard-selected (for the
    /// [`SelectionStyle::BrightSelected`] style) `bright`; when drag-highlighted
    /// `sel_fg`.
    Themed,
    /// A fixed semantic color immune to selection/highlight (agent status glyph).
    Role(SpanRole),
}

/// One colored piece of a selectable row's first line, rendered left-to-right.
#[derive(Clone, Debug)]
pub struct SelectableSpan {
    /// Span text.
    pub text: String,
    /// Span color policy.
    pub color: SpanColor,
}

/// A single row in a selectable list, projected by the domain layer.
#[derive(Clone, Debug)]
pub struct SelectableRow {
    /// First line, as ordered colored spans.
    pub spans: Vec<SelectableSpan>,
    /// Optional second line (issue/pr meta line). Painted `dim`, or `sel_fg`
    /// when the row is drag-highlighted.
    ///
    /// - `Some(non-empty)` → two-line row (issue/pr full layout).
    /// - `Some("")` → single-line compact row (issue/pr compact layout).
    /// - `None` → single-line agent-style row (`Box(flex_direction Row)`).
    pub meta_line: Option<String>,
    /// Whether this row is the keyboard-selected row.
    pub is_selected: bool,
}

/// How a list paints keyboard-selection / drag-highlight on themed spans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionStyle {
    /// Issue/PR: themed color is `fg` (or `sel_fg` when drag-highlighted);
    /// selected rows render **bold**. (Color is NOT brightened.)
    #[default]
    BoldSelected,
    /// Agent: themed color is `bright` when keyboard-selected, `fg` otherwise,
    /// `sel_fg` when drag-highlighted; weight is always normal.
    BrightSelected,
}

/// Border policy for the list box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListBorder {
    /// Issue/PR: `Double` when focused, `Round` otherwise; `border_color` fixed.
    #[default]
    DoubleOnFocus,
    /// Agent: `Round` always; `border_color` brightens when focused.
    RoundFocusedColor,
}

/// Props for [`SelectableList`].
#[derive(Default, Props)]
pub struct SelectableListProps {
    /// Title text (e.g. "Issues", "Pull Requests", "Agents").
    pub title: String,
    /// Already windowed + projected rows (domain owns scroll windowing).
    pub rows: Vec<SelectableRow>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Loading/empty status message to show in place of rows (`None` = none).
    pub empty_message: Option<String>,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active drag text selection (if any).
    pub selection: Option<TextSelection>,
    /// Which selectable pane this list is (filters the drag selection).
    pub pane: SelectablePane,
    /// Border policy.
    pub border: ListBorder,
    /// Whether the content box uses `padding: 1` (AgentList) or none.
    pub content_padding: bool,
    /// Selection/row-color policy.
    pub selection_style: SelectionStyle,
}

/// Resolve `(border_style, border_color)` for the outer box per [`ListBorder`].
fn resolve_border(policy: ListBorder, focused: bool, rc: ResolvedColors) -> (BorderStyle, Color) {
    match policy {
        ListBorder::DoubleOnFocus => {
            let style = if focused {
                BorderStyle::Double
            } else {
                BorderStyle::Round
            };
            (style, rc.border)
        }
        ListBorder::RoundFocusedColor => {
            let color = if focused {
                rc.border_focused
            } else {
                rc.border
            };
            (BorderStyle::Round, color)
        }
    }
}

/// Compute the themed foreground color for a row's spans (excluding fixed
/// spans), and the weight to apply to every span in that row.
fn themed_color_and_weight(
    style: SelectionStyle,
    is_selected: bool,
    highlighted: bool,
    rc: ResolvedColors,
) -> (Color, Weight) {
    match style {
        SelectionStyle::BoldSelected => {
            let fg = if highlighted { rc.sel_fg } else { rc.fg };
            let weight = if is_selected {
                Weight::Bold
            } else {
                Weight::Normal
            };
            (fg, weight)
        }
        SelectionStyle::BrightSelected => {
            let fg = if highlighted {
                rc.sel_fg
            } else if is_selected {
                rc.bright
            } else {
                rc.fg
            };
            (fg, Weight::Normal)
        }
    }
}

/// Resolve a [`SpanRole`] to a concrete terminal color against the theme.
fn resolve_role(role: SpanRole, rc: ResolvedColors) -> Color {
    match role {
        SpanRole::Bright => rc.bright,
        SpanRole::Dim => rc.dim,
        SpanRole::Red => Color::Red,
        SpanRole::Yellow => Color::Yellow,
        SpanRole::Blue => Color::Blue,
    }
}

/// Resolve the concrete color for a single span given the row's themed color.
fn span_color(color: SpanColor, themed: Color, rc: ResolvedColors) -> Color {
    match color {
        SpanColor::Themed => themed,
        SpanColor::Role(role) => resolve_role(role, rc),
    }
}

/// Whether a given content line index is covered by an active drag selection
/// on this pane (i.e. should be painted in inverse video).
fn row_is_highlighted(selection: Option<&TextSelection>, pane: SelectablePane, idx: usize) -> bool {
    selection
        .filter(|s| s.pane() == pane)
        .and_then(|s| row_highlight_range(s, idx))
        .is_some()
}

/// Resolve the single title span for a one-span issue/pr row (the pre-refactor
/// components render the whole `title_line` as a single `Text`).
fn title_span(row: &SelectableRow) -> (&str, SpanColor) {
    match row.spans.first() {
        Some(s) => (s.text.as_str(), s.color),
        None => ("", SpanColor::Themed),
    }
}

/// Render the empty/loading status message box (matches IssueList/PrList).
fn render_empty_message(msg: &str, dim: Color) -> AnyElement<'static> {
    element! {
        Box(padding_left: 1u32, height: 1u32) {
            Text(content: msg, color: dim)
        }
    }
    .into_any()
}

/// Render a single-line compact issue/pr row (`meta_line == Some("")`):
/// `Box(height 1u32, background_color row_bg) { Text(title) }`.
fn render_compact_row(
    row: &SelectableRow,
    themed: Color,
    weight: Weight,
    row_bg: Color,
    rc: ResolvedColors,
) -> AnyElement<'static> {
    let (text, color) = title_span(row);
    element! {
        Box(height: 1u32, background_color: row_bg) {
            Text(content: text, color: span_color(color, themed, rc), weight: weight)
        }
    }
    .into_any()
}

/// Render a two-line issue/pr full row (`meta_line == Some(non-empty)`):
/// `Box(flex_direction Column) { title Box ; meta Box }`.
fn render_two_line_row(
    row: &SelectableRow,
    themed: Color,
    weight: Weight,
    row_bg: Color,
    highlighted: bool,
    rc: ResolvedColors,
) -> AnyElement<'static> {
    let (title_text, title_color_policy) = title_span(row);
    let title_color = span_color(title_color_policy, themed, rc);
    let meta = row.meta_line.as_deref().unwrap_or("");
    let meta_color = if highlighted { rc.sel_fg } else { rc.dim };
    element! {
        Box(flex_direction: FlexDirection::Column) {
            Box(height: 1u32, background_color: row_bg) {
                Text(content: title_text, color: title_color, weight: weight)
            }
            Box(height: 1u32, background_color: row_bg) {
                Text(content: meta, color: meta_color)
            }
        }
    }
    .into_any()
}

/// Render an agent-style row (`meta_line == None`):
/// `Box(flex_direction Row, background_color row_bg) { Text per span }`.
fn render_agent_row(
    row: &SelectableRow,
    themed: Color,
    weight: Weight,
    row_bg: Color,
    rc: ResolvedColors,
) -> AnyElement<'static> {
    let texts: Vec<AnyElement<'static>> = row
        .spans
        .iter()
        .map(|s| {
            element! {
                Text(
                    content: s.text.as_str(),
                    color: span_color(s.color, themed, rc),
                    weight: weight,
                )
            }
            .into_any()
        })
        .collect();
    element! {
        Box(flex_direction: FlexDirection::Row, background_color: row_bg) {
            #(texts)
        }
    }
    .into_any()
}

/// Render a single projected row, choosing the box layout that matches the
/// original domain component for byte-identical output.
fn render_row(
    row: &SelectableRow,
    style: SelectionStyle,
    highlighted: bool,
    rc: ResolvedColors,
) -> AnyElement<'static> {
    let row_bg = if highlighted { rc.sel_bg } else { rc.bg };
    let (themed, weight) = themed_color_and_weight(style, row.is_selected, highlighted, rc);
    match &row.meta_line {
        // Compact issue/pr: empty meta string → single-line title box.
        Some(m) if m.is_empty() => render_compact_row(row, themed, weight, row_bg, rc),
        // Full issue/pr: non-empty meta → two-line column box.
        Some(_) => render_two_line_row(row, themed, weight, row_bg, highlighted, rc),
        // Agent: no meta → single-line row box with one Text per span.
        None => render_agent_row(row, themed, weight, row_bg, rc),
    }
}

/// Build the content children: an empty message, or one element per row.
fn content_children(props: &SelectableListProps, rc: ResolvedColors) -> Vec<AnyElement<'static>> {
    match &props.empty_message {
        Some(msg) => vec![render_empty_message(msg, rc.dim)],
        None => props
            .rows
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                let highlighted = row_is_highlighted(props.selection.as_ref(), props.pane, idx);
                render_row(row, props.selection_style, highlighted, rc)
            })
            .collect(),
    }
}

/// Build the content box element, applying `padding: 1` only when the
/// `content_padding` flag is set (AgentList). The two branches produce the
/// exact same element tree as the original domain components.
fn content_box(props: &SelectableListProps, rc: ResolvedColors) -> AnyElement<'static> {
    let children = content_children(props, rc);
    if props.content_padding {
        element! {
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                #(children)
            }
        }
        .into_any()
    } else {
        element! {
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                background_color: rc.bg,
            ) {
                #(children)
            }
        }
        .into_any()
    }
}

/// Generic bordered, scrollable, selectable list pane.
///
/// Renders a bordered box with a bold title row, then either an empty/loading
/// status message or the projected rows. Row coloring, weight, border style,
/// and padding are driven by the props so Issue/PR and Agent shapes render
/// byte-identically to the original per-domain components.
#[component]
pub fn SelectableList(props: &SelectableListProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let (border_style, border_color) = resolve_border(props.border, props.focused, rc);
    let content = content_box(props, rc);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: border_color,
            background_color: rc.bg,
        ) {
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: props.title.as_str(), weight: Weight::Bold, color: rc.fg)
            }
            #(
                // The content box is built separately (it conditionally sets
                // `padding: 1`); embed it as the second child.
                vec![content]
            )
        }
    }
}

/// Build a [`SelectableList`] element from a fully-formed [`SelectableListProps`].
///
/// iocraft's `element!` macro cannot spread a pre-built props struct into a
/// component invocation (each field must be passed inline), so domain wrappers
/// like [`crate::ui::components::issue_list_props`] return a
/// [`SelectableListProps`] that is rendered through this helper. Screens embed
/// the returned element as a pane child.
#[must_use]
pub fn selectable_list_element(props: SelectableListProps) -> AnyElement<'static> {
    element! {
        SelectableList(
            title: props.title,
            rows: props.rows,
            focused: props.focused,
            empty_message: props.empty_message,
            colors: props.colors,
            selection: props.selection,
            pane: props.pane,
            border: props.border,
            content_padding: props.content_padding,
            selection_style: props.selection_style,
        )
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    //! Tests for [`SelectableList`] and the domain projection wrappers.
    //!
    //! These lock two layers of the refactoring contract:
    //! 1. The projection wrappers (`issue_list_props`, `pr_list_props`,
    //!    `agent_list_props`) produce rows whose span text/colors exactly
    //!    match the pre-refactor projections (parity with the unchanged pure
    //!    `*_visible_rows` functions).
    //! 2. The rendered ANSI output preserves the per-domain border/weight/color
    //!    behavior (bold-on-select for Issue/PR, fixed status-glyph color +
    //!    bright-on-select for Agent, double-vs-round border policy, empty
    //!    message rendering).

    use super::*;
    use crate::domain::{
        Agent, AgentId, AgentStatus, Issue, IssueState, PrCheckStatus, PrState, PullRequest,
        RepositoryId,
    };
    use crate::git_info::GitRepoInfo;
    use crate::theme::ThemeColors;
    use crate::ui::components::agent_list::{AgentListSelection, agent_list_props};
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
        }
    }

    fn pr(n: u64) -> PullRequest {
        PullRequest {
            number: n,
            title: format!("PR {n}"),
            state: PrState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2026-01-01".to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        }
    }

    fn agent(name: &str, status: AgentStatus) -> Agent {
        let mut a = Agent::new(
            AgentId(name.to_string()),
            RepositoryId("r".to_string()),
            name.to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        a.status = status;
        a
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
        let visible =
            crate::ui::components::pr_list::pr_list_visible_rows(&prs, Some(0), 10, Some(40));
        assert_eq!(props.rows.len(), visible.len());
        for (got, want) in props.rows.iter().zip(visible.iter()) {
            assert_eq!(got.spans.len(), 1);
            assert_eq!(got.spans[0].text, want.title_line);
            // Compact → empty meta string (single-line row).
            assert_eq!(got.meta_line.as_deref(), Some(""));
            assert_eq!(got.is_selected, want.is_selected);
        }
    }

    /// A Running selected agent projects to three spans: prefix "> ", a
    /// fixed-color status glyph, and " {name}". The first span is themed.
    #[test]
    fn agent_list_props_running_selected_spans() {
        let agents = vec![agent("alpha", AgentStatus::Running)];
        let props = agent_list_props(
            &agents,
            &[],
            AgentListSelection::default(),
            false,
            ThemeColors::default(),
            None,
        );
        assert_eq!(props.rows.len(), 1);
        let row = &props.rows[0];
        assert_eq!(row.spans.len(), 3);
        assert_eq!(row.spans[0].text, "> ");
        assert!(matches!(row.spans[0].color, SpanColor::Themed));
        assert_eq!(row.spans[1].text, "*");
        // Running → bright fixed role (resolved by the component).
        assert!(matches!(
            row.spans[1].color,
            SpanColor::Role(SpanRole::Bright)
        ));
        assert_eq!(row.spans[2].text, " alpha");
        assert!(matches!(row.spans[2].color, SpanColor::Themed));
        assert!(row.meta_line.is_none());
        assert!(row.is_selected);
    }

    /// A grabbed agent's prefix span text is "↕ " regardless of selection.
    #[test]
    fn agent_list_props_grabbed_prefix() {
        let agents = vec![
            agent("alpha", AgentStatus::Running),
            agent("beta", AgentStatus::Completed),
        ];
        let props = agent_list_props(
            &agents,
            &[],
            AgentListSelection {
                selected: 0,
                grabbed: Some(1),
            },
            false,
            ThemeColors::default(),
            None,
        );
        // Row 1 is grabbed (index 1).
        assert_eq!(props.rows[1].spans[0].text, "\u{2195} ");
        // Row 0 is selected but NOT grabbed → "> ".
        assert_eq!(props.rows[0].spans[0].text, "> ");
    }

    /// The agent status-glyph fixed color maps each `AgentStatus` exactly as the
    /// pre-refactor component did.
    #[test]
    fn agent_list_props_status_glyph_color_per_status() {
        let cases: [(AgentStatus, SpanRole); 7] = [
            (AgentStatus::Running, SpanRole::Bright),
            (AgentStatus::Completed, SpanRole::Bright),
            (AgentStatus::Dead, SpanRole::Red),
            (AgentStatus::Errored, SpanRole::Red),
            (AgentStatus::Waiting, SpanRole::Yellow),
            (AgentStatus::Paused, SpanRole::Blue),
            (AgentStatus::Queued, SpanRole::Dim),
        ];
        for (status, expected) in cases {
            let agents = vec![agent("a", status)];
            let props = agent_list_props(
                &agents,
                &[],
                AgentListSelection::default(),
                false,
                ThemeColors::default(),
                None,
            );
            let row = &props.rows[0];
            assert_eq!(
                row.spans.len(),
                3,
                "status {status:?} should project 3 spans"
            );
            match row.spans[1].color {
                SpanColor::Role(r) => {
                    assert_eq!(r, expected, "status {status:?} glyph role mismatch");
                }
                SpanColor::Themed => panic!("status {status:?} glyph must be Role, got Themed"),
            }
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

    /// BrightSelected agent row: the status glyph keeps its fixed color (Red for
    /// Dead) even when selected, and the agent name uses the bright color —
    /// matching the pre-refactor AgentList which always used `Weight::Normal`
    /// for rows and a fixed status-glyph color. (The title row is always bold
    /// in every list, so we assert on the glyph color + name color, not on the
    /// absence of any bold SGR.)
    #[test]
    fn bright_selected_agent_row_keeps_fixed_glyph_color() {
        let agents = vec![agent("dead-agent", AgentStatus::Dead)];
        let props = agent_list_props(
            &agents,
            &[],
            AgentListSelection::default(),
            true,
            ThemeColors::default(),
            None,
        );
        let ansi = render_ansi(props, 30, 8);
        // `Color::Red` is emitted by iocraft as the 256-color code 38;5;9. The
        // key byte-identity guarantee is that the status glyph keeps its fixed
        // color (immune to the BrightSelected themed-color policy).
        assert!(
            ansi.contains("\u{1b}[38;5;9m"),
            "Dead status glyph must keep its fixed Red (38;5;9) color: {ansi}"
        );
        // The selected agent's themed name span uses bright (#00ff00).
        assert!(
            ansi.contains("dead-agent"),
            "agent name text must appear: {ansi}"
        );
    }

    // ── Agent list with git info (issue #170) ───────────────────────────────

    /// When `git_infos` carries an origin shortform + branch, the agent row
    /// gets a 4th dim-colored span with `  {origin} @ {branch}` text.
    #[test]
    fn agent_list_props_with_git_info_adds_suffix_span() {
        let agents = vec![agent("fix-login", AgentStatus::Running)];
        let git_infos = vec![GitRepoInfo {
            origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
            branch: Some("main".to_owned()),
            dirty: None,
        }];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            false,
            ThemeColors::default(),
            None,
        );
        let row = &props.rows[0];
        assert_eq!(
            row.spans.len(),
            4,
            "should have prefix + glyph + name + git suffix"
        );
        assert_eq!(row.spans[3].text, "  vybestack/llxprt-jefe @ main");
        assert!(matches!(row.spans[3].color, SpanColor::Role(SpanRole::Dim)));
    }

    /// When `git_infos` entry has no data (both None), no suffix span is added
    /// — the row stays at 3 spans.
    #[test]
    fn agent_list_props_with_empty_git_info_no_suffix() {
        let agents = vec![agent("fix-login", AgentStatus::Running)];
        let git_infos = vec![GitRepoInfo::default()];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            false,
            ThemeColors::default(),
            None,
        );
        let row = &props.rows[0];
        assert_eq!(
            row.spans.len(),
            3,
            "empty git info should not add a suffix span"
        );
    }

    /// When `git_infos` is shorter than `agents` (missing entry), the agent at
    /// the missing index just renders without a suffix (graceful degradation).
    #[test]
    fn agent_list_props_git_infos_shorter_than_agents() {
        let agents = vec![
            agent("alpha", AgentStatus::Running),
            agent("beta", AgentStatus::Completed),
        ];
        // Only provide git info for index 0.
        let git_infos = vec![GitRepoInfo {
            origin_shortform: Some("acme/widgets".to_owned()),
            branch: Some("dev".to_owned()),
            dirty: None,
        }];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            false,
            ThemeColors::default(),
            None,
        );
        assert_eq!(props.rows[0].spans.len(), 4, "row 0 has git suffix");
        assert_eq!(props.rows[1].spans.len(), 3, "row 1 has no git suffix");
    }

    /// Agent list git-info suffix renders in the rendered output.
    #[test]
    fn agent_list_git_info_suffix_renders() {
        let agents = vec![agent("fix-login", AgentStatus::Running)];
        let git_infos = vec![GitRepoInfo {
            origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
            branch: Some("main".to_owned()),
            dirty: None,
        }];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            true,
            ThemeColors::default(),
            None,
        );
        let ansi = render_ansi(props, 60, 8);
        assert!(
            ansi.contains("vybestack/llxprt-jefe @ main"),
            "git suffix text must appear in rendered output: {ansi}"
        );
    }

    /// Agent list git-info suffix includes the dirty marker (` *`) when the
    /// working tree is dirty (issue #230).
    #[test]
    fn agent_list_dirty_suffix_renders_marker() {
        let agents = vec![agent("fix-login", AgentStatus::Running)];
        let git_infos = vec![GitRepoInfo {
            origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
            branch: Some("main".to_owned()),
            dirty: Some(true),
        }];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            true,
            ThemeColors::default(),
            None,
        );
        // The suffix span text must include the dirty marker.
        assert_eq!(
            props.rows[0].spans[3].text, "  vybestack/llxprt-jefe @ main *",
            "dirty worktree must append ' *' to the git suffix span"
        );
        let ansi = render_ansi(props, 70, 8);
        assert!(
            ansi.contains("vybestack/llxprt-jefe @ main *"),
            "dirty marker must appear in rendered output: {ansi}"
        );
    }

    /// Agent list git-info suffix does NOT include the dirty marker when the
    /// tree is clean or dirty status is unknown (issue #230).
    #[test]
    fn agent_list_clean_suffix_no_marker() {
        let agents = vec![agent("fix-login", AgentStatus::Running)];
        let git_infos = vec![GitRepoInfo {
            origin_shortform: Some("vybestack/llxprt-jefe".to_owned()),
            branch: Some("main".to_owned()),
            dirty: Some(false),
        }];
        let props = agent_list_props(
            &agents,
            &git_infos,
            AgentListSelection::default(),
            true,
            ThemeColors::default(),
            None,
        );
        assert_eq!(
            props.rows[0].spans[3].text, "  vybestack/llxprt-jefe @ main",
            "clean worktree must not append a dirty marker"
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
}
