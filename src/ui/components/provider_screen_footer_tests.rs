//! Footer-band composition tests for the shared provider runtime chrome.
//!
//! The band carries two segments — the context-sensitive hint run and the
//! process identity `pid:<pid> <commit>` — exactly as the pre-cutover
//! `KeybindBar` `SpaceBetween` row did (`keybind_bar.rs:66-86`, mounted at
//! `dashboard.rs:331-346`). These tests pin the budget that row computed: an
//! overflowing band shrinks both segments in proportion to their own length,
//! so neither the hint literals nor the `pid:` prefix can be crowded out by
//! the other (issue #734).

use super::{clip_to_width, footer_line};

/// The dashboard hint run, verbatim, as the corpus observes it on a band wide
/// enough to hold all of it (`tui-scenarios-macos-1`,
/// `issue621/prs-list-send-agent`, 400 columns). `active-only` occupies
/// columns 103-113 of it, which is why a 120-column band that reserves the
/// identity's full width loses the literal.
const DASHBOARD_HINTS: &str = "^/k/v/j navigate | </> pane | t/T/F12 terminal focus | F7 shells | F10 shell | F8 external term | v/V active-only (repos+agents) | / search | ⌥1-9 jump agent | n new-agent | N new-repo | Ctrl-d delete | Ctrl-k kill | Ctrl-r restart | l/L relaunch/recover | s/S split | , settings | ?/h/H/F1 help | Ctrl-q quit | qqq quit";

/// A process identity of the shape `process_identity_label` produces.
const IDENTITY: &str = "pid:4732 441751f";

/// `issue722/dashboard-arrow-navigation` runs at 120 columns and waits for the
/// `active-only` hint; `pid-commit-corner` waits for `pid:`. Both must render
/// on the same band.
#[test]
fn the_dashboard_band_keeps_the_hint_run_and_the_identity_at_the_failing_width() {
    let line = footer_line(DASHBOARD_HINTS, IDENTITY, 120);

    assert_eq!(
        unicode_width::UnicodeWidthStr::width(line.as_str()),
        120,
        "the band fills its resolved rectangle: {line}"
    );
    assert!(
        line.contains("active-only"),
        "the hint literal the corpus waits for must survive: {line}"
    );
    assert!(
        line.contains("pid:"),
        "the identity label must keep its prefix: {line}"
    );
    assert!(
        line.starts_with("^/k/v/j navigate"),
        "hints keep the left edge: {line}"
    );
}

/// `pid-commit-corner.json` runs at 100 columns and asserts `pid:` there.
#[test]
fn the_identity_prefix_survives_at_the_pid_scenario_width() {
    let line = footer_line(DASHBOARD_HINTS, IDENTITY, 100);

    assert_eq!(unicode_width::UnicodeWidthStr::width(line.as_str()), 100);
    assert!(line.contains("pid:"), "{line}");
}

/// A band with room for both lays them out as the flex row did: hints on the
/// left edge, identity flush right.
#[test]
fn a_band_with_room_for_both_right_aligns_the_identity() {
    let line = footer_line("q quit", IDENTITY, 40);

    assert_eq!(line.len(), 40, "the band fills its resolved rectangle");
    assert!(
        line.starts_with("q quit"),
        "hints keep the left edge: {line}"
    );
    assert!(
        line.ends_with(IDENTITY),
        "the identity keeps the right edge: {line}"
    );
}

/// An overflowing band shrinks both segments in proportion to their own
/// length, which is what the pre-cutover flex row did; neither segment is
/// dropped. Hints of 28 cells and an identity of 16 share a 20-cell band as
/// 13 and 7 (`16 * 20 / 44`, rounded).
#[test]
fn an_overflowing_band_shrinks_both_segments_in_proportion() {
    let line = footer_line("q quit | ? help | , settings", IDENTITY, 20);

    assert_eq!(line, "q quit | ? h…pid:473");
}

/// Clipping keeps whole cells and never spends one on a truncation marker.
#[test]
fn the_identity_is_clipped_rather_than_ellipsised() {
    assert_eq!(clip_to_width(IDENTITY, 4), "pid:");
    assert_eq!(clip_to_width(IDENTITY, 0), "");
    assert_eq!(clip_to_width(IDENTITY, 99), IDENTITY);
    assert_eq!(clip_to_width("⌥1-9", 2), "⌥1");
}
