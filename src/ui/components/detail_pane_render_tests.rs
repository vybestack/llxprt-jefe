//! Render-path tests for the generic `DetailPane` component.
//!
//! These lock the shared structure that `IssueDetailView` and `PrDetailView`
//! delegate to after the refactoring: a bordered box → N header rows →
//! `ScrollableText` viewport → optional `TextBox` composer. The assertions are
//! structural (border style on focus, header-row rendering, composer presence)
//! so they validate the generic renderer independently of the per-domain
//! projections (which keep their own regression-guard render tests).

use crate::selection::SelectablePane;
use crate::theme::ThemeColors;
use crate::ui::components::{
    DetailComposerProps, DetailHeaderColor, DetailHeaderRow, DetailPaneProps, detail_pane_element,
};

use iocraft::prelude::*;

/// Render a `DetailPane` element into an ANSI string at a fixed size.
fn render_ansi(props: DetailPaneProps, cols: u16, rows: u16) -> String {
    let mut elem = element! {
        Box(width: u32::from(cols), height: u32::from(rows)) {
            #(vec![detail_pane_element(props)])
        }
    };
    let canvas = elem.render(Some(usize::from(cols)));
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    String::from_utf8_lossy(&buf).into_owned()
}

/// Helper: a 5-row header mirroring the real detail components.
fn sample_header_rows() -> Vec<DetailHeaderRow> {
    vec![
        DetailHeaderRow {
            content: "#1 Title".to_string(),
            color: DetailHeaderColor::Fg,
            line: 0,
        },
        DetailHeaderRow {
            content: "OPEN  by @x".to_string(),
            color: DetailHeaderColor::Bright,
            line: 1,
        },
        DetailHeaderRow {
            content: "labels: -".to_string(),
            color: DetailHeaderColor::Dim,
            line: 2,
        },
        DetailHeaderRow {
            content: "https://example.com".to_string(),
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

/// Base props shared by every test: the full default field set so individual
/// tests only override the values they care about (avoids error-prone manual
/// updates whenever a new field is added).
fn base_props() -> DetailPaneProps {
    DetailPaneProps {
        header_rows: sample_header_rows(),
        content: "body".to_string(),
        content_cursor: None,
        scroll_offset: 0,
        viewport_rows: 5,
        content_line_offset: 5,
        max_line_width: 40,
        focused: false,
        pane: SelectablePane::IssueDetail,
        colors: ThemeColors::default(),
        selection: None,
        composer: None,
        composer_rows: 0,
    }
}

/// A focused `DetailPane` must render a double border (`╔`); unfocused renders
/// a round border (`╭`). This mirrors the original `DoubleOnFocus` behavior of
/// both `IssueDetailView` and `PrDetailView`.
#[test]
fn detail_pane_border_switches_on_focus() {
    let unfocused = {
        let mut p = base_props();
        p.focused = false;
        render_ansi(p, 40, 14)
    };
    let focused = {
        let mut p = base_props();
        p.focused = true;
        render_ansi(p, 40, 14)
    };
    assert!(
        unfocused.contains('╭'),
        "unfocused detail must use round border: {unfocused}"
    );
    assert!(
        focused.contains('╔'),
        "focused detail must use double border: {focused}"
    );
}

/// The header rows' text must all appear in the rendered output.
#[test]
fn detail_pane_renders_all_header_rows() {
    let mut props = base_props();
    props.focused = true;
    let ansi = render_ansi(props, 50, 16);
    assert!(ansi.contains("#1 Title"), "title row missing: {ansi}");
    assert!(ansi.contains("OPEN  by @x"), "state row missing: {ansi}");
    assert!(ansi.contains("labels: -"), "labels row missing: {ansi}");
    assert!(
        ansi.contains("https://example.com"),
        "url row missing: {ansi}"
    );
    assert!(
        ansi.contains("─────────────────────────────────────────"),
        "separator row missing: {ansi}"
    );
}

/// The scrollable content text must appear below the header rows.
#[test]
fn detail_pane_renders_scrollable_content() {
    let mut props = base_props();
    props.focused = true;
    props.content = "unique body marker".to_string();
    let ansi = render_ansi(props, 50, 16);
    assert!(
        ansi.contains("unique body marker"),
        "content text must render: {ansi}"
    );
}

/// When a composer is supplied, the TextBox draft text must render below the
/// viewport, and the composer prefix must appear.
#[test]
fn detail_pane_renders_composer_when_present() {
    let mut props = base_props();
    props.focused = true;
    props.viewport_rows = 3;
    props.composer = Some(DetailComposerProps {
        text: "draft composer text".to_string(),
        byte_cursor: "draft composer text".len(),
        content_width: 40,
        prefix: "  │ ",
    });
    props.composer_rows = 3;
    let ansi = render_ansi(props, 50, 16);
    assert!(
        ansi.contains("draft composer text"),
        "composer draft text must render: {ansi}"
    );
    assert!(
        ansi.contains("  │ "),
        "composer prefix must render when composer is present: {ansi}"
    );
}

/// The composer must NOT render when `composer` is `None` (the draft text must
/// be absent from the output).
#[test]
fn detail_pane_omits_composer_when_absent() {
    let mut props = base_props();
    props.focused = true;
    let ansi = render_ansi(props, 50, 16);
    // No TextBox gutter/prefix should appear when no composer is active.
    assert!(
        !ansi.contains("  │ "),
        "no composer prefix should render when composer is None: {ansi}"
    );
}
