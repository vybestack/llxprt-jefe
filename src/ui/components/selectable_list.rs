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

use crate::list_viewport::fit_text_to_width;
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
    /// Absolute logical-list index represented by this visible row.
    pub source_index: usize,
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
    /// Fixed terminal-cell width of every physical content row.
    pub content_width: usize,
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

/// Fit every span in one logical row as a single display-width budget while
/// preserving each retained span's color policy.
fn fit_row(row: &SelectableRow, content_width: usize) -> SelectableRow {
    SelectableRow {
        source_index: row.source_index,
        spans: fit_spans_to_width(&row.spans, content_width),
        meta_line: row
            .meta_line
            .as_deref()
            .map(|line| fit_text_to_width(line, content_width)),
        is_selected: row.is_selected,
    }
}

fn fit_spans_to_width(spans: &[SelectableSpan], width: usize) -> Vec<SelectableSpan> {
    let full_width: usize = spans
        .iter()
        .map(|span| unicode_width::UnicodeWidthStr::width(span.text.as_str()))
        .sum();
    if full_width <= width {
        return spans.to_vec();
    }
    if width == 0 {
        return Vec::new();
    }

    let content_width = width - 1;
    let mut used: usize = 0;
    let mut fitted: Vec<SelectableSpan> = Vec::new();
    let mut ellipsis_color = SpanColor::Themed;
    'spans: for span in spans {
        for character in span.text.chars() {
            let character_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if used.saturating_add(character_width) > content_width {
                ellipsis_color = span.color;
                break 'spans;
            }
            push_styled_character(&mut fitted, character, span.color);
            used = used.saturating_add(character_width);
        }
    }
    push_styled_character(&mut fitted, '…', ellipsis_color);
    fitted
}

fn push_styled_character(spans: &mut Vec<SelectableSpan>, character: char, color: SpanColor) {
    if let Some(last) = spans.last_mut()
        && last.color == color
    {
        last.text.push(character);
        return;
    }
    spans.push(SelectableSpan {
        text: character.to_string(),
        color,
    });
}

/// One fitted copyable line and the logical list item that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedContentLine {
    pub source_index: usize,
    pub text: String,
}

/// Fit the exact visible row projection consumed by rendering into copy lines.
#[must_use]
pub fn projected_content_lines(props: &SelectableListProps) -> Vec<ProjectedContentLine> {
    if let Some(message) = &props.empty_message {
        return vec![ProjectedContentLine {
            source_index: 0,
            text: fit_text_to_width(message, props.content_width),
        }];
    }
    props
        .rows
        .iter()
        .flat_map(|row| {
            let fitted = fit_row(row, props.content_width);
            let title = ProjectedContentLine {
                source_index: fitted.source_index,
                text: fitted.spans.into_iter().map(|span| span.text).collect(),
            };
            let meta = fitted
                .meta_line
                .filter(|line| !line.is_empty())
                .map(|text| ProjectedContentLine {
                    source_index: fitted.source_index,
                    text,
                });
            std::iter::once(title).chain(meta)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct RowPresentation {
    themed: Color,
    weight: Weight,
    background: Color,
    colors: ResolvedColors,
}

/// Render the empty/loading status message box (matches IssueList/PrList).
fn render_empty_message(
    msg: &str,
    highlighted: bool,
    rc: ResolvedColors,
    content_width: usize,
) -> AnyElement<'static> {
    let width = u32::try_from(content_width).unwrap_or(u32::MAX);
    let fitted = fit_text_to_width(msg, content_width);
    let foreground = if highlighted { rc.sel_fg } else { rc.dim };
    let background = if highlighted { rc.sel_bg } else { rc.bg };
    element! {
        Box(height: 1u32, width: width, background_color: background) {
            Text(content: fitted, color: foreground, wrap: TextWrap::NoWrap)
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
    content_width: usize,
) -> AnyElement<'static> {
    let (text, color) = title_span(row);
    let width = u32::try_from(content_width).unwrap_or(u32::MAX);
    element! {
        Box(height: 1u32, width: width, background_color: row_bg) {
            Text(
                content: text,
                color: span_color(color, themed, rc),
                weight: weight,
                wrap: TextWrap::NoWrap,
            )
        }
    }
    .into_any()
}

/// Render a two-line issue/pr full row (`meta_line == Some(non-empty)`):
/// `Box(flex_direction Column) { title Box ; meta Box }`.
fn render_two_line_row(
    row: &SelectableRow,
    presentation: RowPresentation,
    title_highlighted: bool,
    meta_highlighted: bool,
    content_width: usize,
) -> AnyElement<'static> {
    let (title_text, title_color_policy) = title_span(row);
    let title_color = if title_highlighted {
        presentation.colors.sel_fg
    } else {
        span_color(title_color_policy, presentation.themed, presentation.colors)
    };
    let meta = row.meta_line.as_deref().unwrap_or("");
    let meta_color = if meta_highlighted {
        presentation.colors.sel_fg
    } else {
        presentation.colors.dim
    };
    let title_background = if title_highlighted {
        presentation.colors.sel_bg
    } else {
        presentation.background
    };
    let meta_background = if meta_highlighted {
        presentation.colors.sel_bg
    } else {
        presentation.background
    };
    let width = u32::try_from(content_width).unwrap_or(u32::MAX);
    element! {
        Box(flex_direction: FlexDirection::Column, width: width) {
            Box(height: 1u32, width: width, background_color: title_background) {
                Text(content: title_text, color: title_color, weight: presentation.weight, wrap: TextWrap::NoWrap)
            }
            Box(height: 1u32, width: width, background_color: meta_background) {
                Text(content: meta, color: meta_color, wrap: TextWrap::NoWrap)
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
    content_width: usize,
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
                    wrap: TextWrap::NoWrap,
                )
            }
            .into_any()
        })
        .collect();
    let width = u32::try_from(content_width).unwrap_or(u32::MAX);
    element! {
        Box(flex_direction: FlexDirection::Row, width: width, background_color: row_bg) {
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
    highlights: &[bool],
    rc: ResolvedColors,
    content_width: usize,
) -> AnyElement<'static> {
    let fitted = fit_row(row, content_width);
    let highlighted = highlights.iter().copied().any(std::convert::identity);
    let row_bg = if highlighted { rc.sel_bg } else { rc.bg };
    let (themed, weight) = themed_color_and_weight(style, fitted.is_selected, highlighted, rc);
    match &fitted.meta_line {
        Some(m) if m.is_empty() => {
            render_compact_row(&fitted, themed, weight, row_bg, rc, content_width)
        }
        Some(_) => {
            let (base_themed, base_weight) =
                themed_color_and_weight(style, fitted.is_selected, false, rc);
            render_two_line_row(
                &fitted,
                RowPresentation {
                    themed: base_themed,
                    weight: base_weight,
                    background: rc.bg,
                    colors: rc,
                },
                highlights.first().copied().unwrap_or(false),
                highlights.get(1).copied().unwrap_or(false),
                content_width,
            )
        }
        None => render_agent_row(&fitted, themed, weight, row_bg, rc, content_width),
    }
}

/// Build the content children: an empty message, or one element per row.
fn content_children(props: &SelectableListProps, rc: ResolvedColors) -> Vec<AnyElement<'static>> {
    if let Some(message) = &props.empty_message {
        return vec![render_empty_message(
            message,
            row_is_highlighted(props.selection.as_ref(), props.pane, 0),
            rc,
            props.content_width,
        )];
    }
    let mut physical_line = 0;
    props
        .rows
        .iter()
        .map(|row| {
            let line_count =
                usize::from(row.meta_line.as_ref().is_some_and(|line| !line.is_empty())) + 1;
            let highlights = (physical_line..physical_line + line_count)
                .map(|line| row_is_highlighted(props.selection.as_ref(), props.pane, line))
                .collect::<Vec<_>>();
            physical_line += line_count;
            render_row(
                row,
                props.selection_style,
                &highlights,
                rc,
                props.content_width,
            )
        })
        .collect()
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
    let content_width = u32::try_from(props.content_width).unwrap_or(u32::MAX);
    let title = fit_text_to_width(&props.title, props.content_width.saturating_sub(1));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: border_color,
            background_color: rc.bg,
        ) {
            Box(height: 1u32, width: content_width, padding_left: 1u32) {
                Text(
                    content: title,
                    weight: Weight::Bold,
                    color: rc.fg,
                    wrap: TextWrap::NoWrap,
                )
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
            content_width: props.content_width,
        )
    }
    .into_any()
}

#[cfg(test)]
#[path = "selectable_list_tests.rs"]
mod tests;
