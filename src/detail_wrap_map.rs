//! Wrap-aware screen→content coordinate mapping for detail panes.
//!
//! Extracted from [`crate::mouse_routing`] to keep that file under the
//! 1000-line source-size limit. The detail panes (Issue/PR detail) render a
//! word-wrapping `ScrollableText` body below a fixed header. Mouse selection
//! must reverse-map a screen row back to a content line + column; because one
//! content line may span several wrapped display rows, the naive 1:1 map
//! (`point_to_content_coords`) is wrong for body rows. This module threads the
//! SAME pure wrap projection the renderer uses ([`jefe::ui::components::doc_wrap`])
//! through the reverse map so selection coordinates stay exact.
//!
//! The selection model and scroll offset both live in CONTENT-LINE space and
//! are unchanged by wrapping; only this screen→content step is wrap-aware.

use jefe::selection::PaneGeometry;
use jefe::selection::{SelectablePane, point_to_content_coords};
use jefe::state::AppState;

/// Bundled screen coordinate + geometry context for [`content_coords_for_pane`],
/// keeping its argument count under the clippy `too_many_arguments` threshold.
pub struct ScreenCoord<'a> {
    /// Screen column.
    pub col: u16,
    /// Screen row.
    pub row: u16,
    /// Pane scroll offset (content-line units) for the scrolled region.
    pub scroll_offset: usize,
    /// Pane geometry (content origin + size).
    pub geometry: &'a PaneGeometry,
}

/// Resolve a screen coordinate to content `(line, col)` for `pane`, taking
/// the word-wrap projection into account for detail panes whose `ScrollableText`
/// wraps long lines.
///
/// Detail panes render a fixed header block followed by a wrapping scrollable
/// body. Header rows keep the naive 1:1 mapping; body rows are reverse-mapped
/// through the wrap projection the renderer uses (`doc_wrap`), so a click on a
/// wrapped subrow lands on the correct content line + char column. Non-detail
/// panes (lists, sidebar, …) never wrap and keep the original
/// [`point_to_content_coords`] mapping.
pub fn content_coords_for_pane(
    app_state: &AppState,
    pane: SelectablePane,
    terminal_cols: u16,
    terminal_rows: u16,
    coord: &ScreenCoord<'_>,
) -> (usize, usize) {
    let geometry = coord.geometry;
    let (render_cols, _) = jefe::layout::effective_render_size(terminal_cols, terminal_rows);
    // Only detail-style panes wrap; finite-row panes use their exact projected
    // physical line to translate terminal cells into character columns.
    let Some((body_content, header_rows, wrap_width)) =
        detail_wrap_projection(app_state, pane, render_cols)
    else {
        let (line, cell_col) =
            point_to_content_coords(coord.col, coord.row, coord.scroll_offset, geometry);
        let content = jefe::pane_content_projection::projected_pane_content(
            pane,
            app_state,
            None,
            &[],
            terminal_cols,
            terminal_rows,
        );
        let Some(text) = content.lines.get(line) else {
            return content.lines.last().map_or((0, 0), |last| {
                (content.lines.len() - 1, last.chars().count())
            });
        };
        let char_col = jefe::ui::components::doc_wrap::display_cell_to_char_offset(text, cell_col);
        return (line, char_col);
    };

    let vp_row_abs = usize::from(coord.row.saturating_sub(geometry.content_origin_row));
    // Header rows are not wrapped and not scrolled: keep the naive mapping so
    // header selection coordinates stay stable.
    if vp_row_abs < header_rows {
        return point_to_content_coords(coord.col, coord.row, 0, geometry);
    }

    // Body row: reverse-map through the wrap projection.
    let body_vp_row = vp_row_abs - header_rows;
    let body_rows = jefe::ui::components::doc_wrap::wrap_document(&body_content, wrap_width);
    let first_visible =
        jefe::ui::components::doc_wrap::line_first_row(&body_rows, coord.scroll_offset);
    let last_visible_row = body_rows
        .len()
        .saturating_sub(first_visible)
        .saturating_sub(1);
    let past_end = body_vp_row > last_visible_row;
    let col_rel = usize::from(coord.col.saturating_sub(geometry.content_origin_col));
    let (body_line, content_col) = if past_end {
        jefe::ui::components::doc_wrap::viewport_row_to_content(
            &body_rows,
            first_visible,
            body_vp_row,
        )
        .unwrap_or((0, 0))
    } else {
        jefe::ui::components::doc_wrap::viewport_cell_to_content(
            &body_rows,
            first_visible,
            body_vp_row,
            col_rel,
        )
        .unwrap_or((0, 0))
    };
    (header_rows + body_line, content_col)
}

/// Build the `(body_content, header_rows, wrap_width)` projection for a
/// wrapping detail pane, or `None` for panes that do not wrap.
///
/// `body_content` is the scrollable body text (no header lines),
/// `header_rows` is the fixed header count, and `wrap_width` is the content
/// width the renderer wraps at. This mirrors exactly what the renderer feeds
/// `ScrollableText` so the reverse-map cannot drift from the painted rows.
pub fn detail_wrap_projection(
    app_state: &AppState,
    pane: SelectablePane,
    cols: u16,
) -> Option<(String, usize, usize)> {
    use jefe::issue_detail_content::build_detail_content;
    use jefe::layout::DETAIL_HEADER_ROWS;
    use jefe::pr_detail_content::build_pr_detail_content;
    match pane {
        SelectablePane::IssueDetail => {
            let detail = app_state.issues_state.issue_detail.as_ref()?;
            let content = build_detail_content(
                detail,
                app_state.issues_state.detail_subfocus,
                &app_state.issues_state.inline_state,
                app_state.issues_state.loading.comments,
            );
            let width = usize::from(jefe::layout::issues_detail_content_width(cols));
            Some((content.text, DETAIL_HEADER_ROWS, width))
        }
        SelectablePane::PrDetail => {
            let detail = app_state.prs_state.pr_detail.as_ref()?;
            let content = build_pr_detail_content(
                detail,
                app_state.prs_state.detail_subfocus,
                &app_state.prs_state.inline_state,
                app_state.prs_state.loading.detail,
                app_state.prs_state.loading.comments,
            );
            let width = usize::from(jefe::layout::prs_detail_content_width(cols));
            Some((content.text, DETAIL_HEADER_ROWS, width))
        }
        SelectablePane::HelpModal => {
            // The help modal content is static and already line-broken short;
            // it does not wrap differently, so keep the naive map.
            None
        }
        _ => None,
    }
}
