//! Tests for issue close/delete keybind hints and delete-overlay text (issue #182).
//!
//! The overlay's visible text is produced by pure projection seams
//! (`delete_confirm_header`, `delete_confirm_hint`) extracted from the iocraft
//! component, mirroring how `pr_render_tests.rs` asserts via projection seams
//! rather than rendering the `element!` macro directly.

use crate::state::ScreenMode;
use crate::ui::components::issue_delete_confirm::{delete_confirm_header, delete_confirm_hint};
use crate::ui::components::keybind_bar::keybind_hints_for;

#[test]
fn issues_keybind_hint_includes_close_and_delete() {
    let hints = keybind_hints_for(ScreenMode::DashboardIssues, false, None);
    assert!(
        hints.contains("close"),
        "issues keybind hint should include close, got: {hints}"
    );
    assert!(
        hints.contains("delete"),
        "issues keybind hint should include delete, got: {hints}"
    );
}

#[test]
fn issues_keybind_hint_includes_capital_c_and_d() {
    let hints = keybind_hints_for(ScreenMode::DashboardIssues, false, None);
    assert!(
        hints.contains("C close"),
        "issues keybind hint should show 'C close', got: {hints}"
    );
    assert!(
        hints.contains("D delete"),
        "issues keybind hint should show 'D delete', got: {hints}"
    );
}

#[test]
fn delete_confirm_header_includes_issue_number() {
    assert_eq!(delete_confirm_header(42), "Delete Issue #42");
    assert_eq!(delete_confirm_header(1), "Delete Issue #1");
}

#[test]
fn delete_confirm_header_is_emoji_free() {
    let header = delete_confirm_header(99);
    assert!(
        header.chars().all(|c| !is_emoji(c)),
        "delete confirm header must be emoji-free, got: {header}"
    );
}

#[test]
fn delete_confirm_hint_unarmed_prompts_enter_to_confirm() {
    let hint = delete_confirm_hint(false);
    assert_eq!(
        hint, "Enter confirm, Esc cancel",
        "unarmed hint should use the exact unarmed wording, got: {hint}"
    );
}

#[test]
fn delete_confirm_hint_armed_prompts_press_enter_to_confirm() {
    let hint = delete_confirm_hint(true);
    assert_eq!(
        hint,
        "Press Enter to confirm delete, Esc to cancel",
        "armed hint should show the two-step confirm wording"
    );
}

/// Whether a char is an emoji/pictograph (rough Unicode-range check for the
/// emoji-free UI invariant).
fn is_emoji(c: char) -> bool {
    let cp = c as u32;
    matches!(cp,
        0x1F300..=0x1FAFF     // symbols & pictographs + extensions
        | 0x2600..=0x27BF     // misc symbols + dingbats
        | 0x2190..=0x21FF     // arrows (used as emoji)
    )
}
