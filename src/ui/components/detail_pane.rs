//! Generic bordered, header + scrollable + optional-composer detail pane.
//!
//! Both the Issue and PR detail panes render the identical structure: a bordered
//! box → N fixed metadata header rows (rendered through the shared
//! [`header_row`] helper) → a [`ScrollableText`] viewport → an optional
//! [`TextBox`] composer. This module owns that iocraft structure once; the
//! per-domain projection modules (`issue_detail`, `pr_detail`) build a
//! [`DetailPaneProps`] carrying all layout math + semantic colors and delegate
//! rendering through [`detail_pane_element`].
//!
//! The header-row selection-highlight helpers ([`header_highlight`],
//! [`header_row`]) live here because both detail components share them; the
//! per-domain pure header projections (`issue_detail_header_view`,
//! `pr_detail_header_view`) stay iocraft-free in their own modules.

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection, row_highlight_range};
use crate::state::InlineState;
use crate::theme::{ResolvedColors, ThemeColors};

use super::scrollable_text::ScrollableText;
use super::text_box::TextBox;

/// Semantic color role for a single header row.
///
/// Resolved against [`ResolvedColors`] by the component. Keeping the role (not
/// a concrete `Color`) in the projection keeps the projection modules
/// iocraft-free (pure-views pattern).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailHeaderColor {
    /// `rc.fg`.
    #[default]
    Fg,
    /// `rc.bright` (e.g. OPEN state).
    Bright,
    /// `rc.dim` (e.g. CLOSED state, labels, url, separator).
    Dim,
}

/// One fixed metadata header row projected by the domain layer.
#[derive(Clone, Debug, Default)]
pub struct DetailHeaderRow {
    /// Row text.
    pub content: String,
    /// Semantic color role resolved by the component.
    pub color: DetailHeaderColor,
    /// Content-line index used for drag-selection whole-row highlighting.
    pub line: usize,
}

/// Composer props for the embedded `TextBox`, when present.
#[derive(Clone, Debug, Default)]
pub struct DetailComposerProps {
    /// Raw composer text.
    pub text: String,
    /// Byte cursor within `text`.
    pub byte_cursor: usize,
    /// Max display width (terminal cols) for prefix + row text.
    pub content_width: usize,
    /// Gutter prefix (e.g. `"  │ "`). Always a `&'static str` derived from
    /// [`composer_from_inline_state`], avoiding per-render allocation.
    pub prefix: &'static str,
}

/// Props for the generic [`DetailPane`] component.
///
/// The projection owns ALL layout math (viewport rows, composer rows,
/// content-line offset) and supplies already-final values; the component is a
/// pure renderer.
#[derive(Default, Props)]
pub struct DetailPaneProps {
    /// Fixed metadata header rows (rendered top-to-bottom).
    pub header_rows: Vec<DetailHeaderRow>,
    /// Scrollable viewport text.
    pub content: String,
    /// Cursor `(line, col)` within the content (for the caret), if any.
    pub content_cursor: Option<(usize, usize)>,
    /// Scroll offset (lines) for the viewport.
    pub scroll_offset: usize,
    /// Rows the `ScrollableText` viewport occupies (already-final layout).
    pub viewport_rows: usize,
    /// Header-row count; passed as `ScrollableText::content_line_offset` so
    /// selection content coordinates line up with the header rows above.
    pub content_line_offset: usize,
    /// `ScrollableText::max_line_width` (content width in cols).
    pub max_line_width: usize,
    /// Whether this pane is focused (drives double-vs-round border).
    pub focused: bool,
    /// Which selectable pane this is (filters the drag selection).
    pub pane: SelectablePane,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active drag text selection, if any.
    pub selection: Option<TextSelection>,
    /// Embedded composer props; when present a `TextBox` renders below the
    /// viewport.
    pub composer: Option<DetailComposerProps>,
    /// Rows the `TextBox` composer occupies (0 when no composer).
    pub composer_rows: usize,
}

/// Resolve a [`DetailHeaderColor`] role to a concrete terminal color.
fn resolve_header_color(role: DetailHeaderColor, rc: ResolvedColors) -> Color {
    match role {
        DetailHeaderColor::Fg => rc.fg,
        DetailHeaderColor::Bright => rc.bright,
        DetailHeaderColor::Dim => rc.dim,
    }
}

/// Whether content line `line` falls inside the active drag selection for
/// `pane`. Shared by both detail components for whole-row header highlighting.
///
/// Header rows use whole-row highlight (ignoring partial column ranges) for
/// visual simplicity on short metadata lines.
#[must_use]
pub fn header_highlight(
    line: usize,
    selection: Option<&TextSelection>,
    pane: SelectablePane,
) -> bool {
    selection
        .filter(|s| s.pane() == pane)
        .and_then(|s| row_highlight_range(s, line))
        .is_some()
}

/// Render a single header row, applying whole-row inverse-video when it falls
/// inside the active drag selection. Shared by both detail components.
#[must_use]
pub fn header_row(
    content: String,
    default_fg: Color,
    line: usize,
    selection: Option<&TextSelection>,
    pane: SelectablePane,
    rc: &ResolvedColors,
) -> AnyElement<'static> {
    if header_highlight(line, selection, pane) {
        element! {
            Box(height: 1u32, background_color: rc.sel_bg) {
                Text(content: content, color: rc.sel_fg)
            }
        }
        .into_any()
    } else {
        element! {
            Box(height: 1u32) {
                Text(content: content, color: default_fg)
            }
        }
        .into_any()
    }
}

/// Extract the active detail composer `(text, byte_cursor, prefix)`.
///
/// Both Issues and PRs detail panes use the same match arms and the same
/// composer prefixes, so this single helper replaces the duplicated
/// `active_issue_composer` / `active_pr_composer`.
#[must_use]
pub fn composer_from_inline_state(
    inline_state: &InlineState,
) -> Option<(String, usize, &'static str)> {
    use crate::state::ComposerTarget;
    match inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            text,
            cursor,
        } => Some((
            text.clone(),
            *cursor,
            crate::layout::NEW_COMMENT_COMPOSER_PREFIX,
        )),
        InlineState::Composer {
            target: ComposerTarget::Reply { .. } | ComposerTarget::ReplyToReviewThread { .. },
            text,
            cursor,
        } => Some((text.clone(), *cursor, crate::layout::REPLY_COMPOSER_PREFIX)),
        InlineState::Composer {
            target: ComposerTarget::NewIssue,
            ..
        }
        | InlineState::Editor { .. }
        | InlineState::None => None,
    }
}

/// Build the header-column box children (one [`header_row`] per projected row).
fn header_children(props: &DetailPaneProps, rc: &ResolvedColors) -> Vec<AnyElement<'static>> {
    props
        .header_rows
        .iter()
        .map(|row| {
            header_row(
                row.content.clone(),
                resolve_header_color(row.color, *rc),
                row.line,
                props.selection.as_ref(),
                props.pane,
                rc,
            )
        })
        .collect()
}

/// Build the header-column box (always present, contains the projected rows).
fn header_box(props: &DetailPaneProps, rc: &ResolvedColors) -> AnyElement<'static> {
    let children = header_children(props, rc);
    element! {
        Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
            #(children)
        }
    }
    .into_any()
}

/// Build the scrollable viewport box (always present).
fn viewport_box(props: &DetailPaneProps, rc: &ResolvedColors) -> AnyElement<'static> {
    let (cursor_line, cursor_col) = (
        props.content_cursor.map(|(l, _)| l),
        props.content_cursor.map(|(_, c)| c),
    );
    element! {
        Box(width: 100pct, padding_left: 1u32) {
            ScrollableText(
                content: props.content.clone(),
                scroll_offset: props.scroll_offset,
                viewport_rows: props.viewport_rows,
                max_line_width: props.max_line_width,
                cursor_line: cursor_line,
                cursor_col: cursor_col,
                color: rc.fg,
                cursor_color: rc.bg,
                cursor_bg: rc.bright,
                track_color: rc.dim,
                thumb_color: rc.bright,
                selection: props
                    .selection
                    .filter(|s| s.pane() == props.pane),
                selection_bg: Some(rc.sel_bg),
                selection_fg: Some(rc.sel_fg),
                bg: Some(rc.bg),
                content_line_offset: props.content_line_offset,
            )
        }
    }
    .into_any()
}

/// Build the embedded composer `TextBox` box; `None` when no composer.
fn composer_box(
    composer: &DetailComposerProps,
    composer_rows: usize,
    rc: &ResolvedColors,
) -> AnyElement<'static> {
    element! {
        Box(width: 100pct, padding_left: 1u32) {
            TextBox(
                text: composer.text.clone(),
                byte_cursor: composer.byte_cursor,
                viewport_rows: composer_rows,
                content_width: composer.content_width,
                prefix: composer.prefix.to_string(),
                color: rc.fg,
                caret_color: rc.bg,
                caret_bg: rc.bright,
            )
        }
    }
    .into_any()
}

/// Generic bordered, header + scrollable + optional-composer detail pane.
///
/// Renders byte-identically to the pre-refactor `IssueDetailView` /
/// `PrDetailView`: a bordered box → header-column box → scrollable viewport →
/// optional composer box. All layout math + color roles are supplied by the
/// per-domain projection so this component stays a pure renderer.
#[component]
pub fn DetailPane(props: &DetailPaneProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };
    let header = header_box(props, &rc);
    let viewport = viewport_box(props, &rc);
    let composer: Option<AnyElement<'static>> = props
        .composer
        .as_ref()
        .map(|c| composer_box(c, props.composer_rows, &rc));

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            #(vec![header])
            #(vec![viewport])
            #(composer.map(|e| vec![e]).unwrap_or_default())
        }
    }
}

/// Build a [`DetailPane`] element from a fully-formed [`DetailPaneProps`].
///
/// iocraft's `element!` macro cannot spread a pre-built props struct into a
/// component invocation (each field must be passed inline), so domain wrappers
/// like [`crate::ui::components::issue_detail_props`] return a
/// [`DetailPaneProps`] that is rendered through this helper. Screens embed the
/// returned element as a pane child.
#[must_use]
pub fn detail_pane_element(props: DetailPaneProps) -> AnyElement<'static> {
    element! {
        DetailPane(
            header_rows: props.header_rows,
            content: props.content,
            content_cursor: props.content_cursor,
            scroll_offset: props.scroll_offset,
            viewport_rows: props.viewport_rows,
            content_line_offset: props.content_line_offset,
            max_line_width: props.max_line_width,
            focused: props.focused,
            pane: props.pane,
            colors: props.colors,
            selection: props.selection,
            composer: props.composer,
            composer_rows: props.composer_rows,
        )
    }
    .into_any()
}
