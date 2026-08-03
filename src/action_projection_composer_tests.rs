use std::collections::BTreeMap;

use super::*;
use crate::domain::default_action_inventory::display::FooterMode;
use crate::state::ScreenId;

struct ComposerFooterCase {
    context: &'static str,
    action: &'static str,
    mode: FooterMode,
    screen: ScreenId,
    chord: &'static str,
}

fn snapshot_with_submit_override(
    context: &str,
    action: &str,
    chords: &[&str],
) -> ActionRegistrySnapshot {
    let mut settings = crate::persistence::settings_document::PublishedSettings::default();
    settings.keymap.insert(
        context.to_owned(),
        BTreeMap::from([(
            action.to_owned(),
            chords.iter().map(|chord| (*chord).to_owned()).collect(),
        )]),
    );
    let composed = crate::persistence::keymap_edit::compose_published(&settings, "settings");
    let Ok(composed) = composed else {
        panic!("composer override fixture must compose: {composed:?}");
    };
    composed.snapshot().clone()
}

fn project_composer_footer(case: &ComposerFooterCase, chords: &[&str]) -> String {
    let snapshot = snapshot_with_submit_override(case.context, case.action, chords);
    project_footer(
        &snapshot,
        FooterProjectionInput {
            screen: case.screen,
            terminal_focused: false,
            shell_overlay_active: false,
            shell_resume_available: false,
            actions_focus: None,
            mode_override: Some(case.mode),
        },
    )
}

fn assert_effective_submit(case: ComposerFooterCase) {
    let footer = project_composer_footer(&case, &[case.chord]);
    assert!(
        footer.contains(&format!("{} submit", case.chord)),
        "composer footer did not project effective submit binding: {footer}"
    );
    assert!(
        !footer.contains("Alt+Enter submit"),
        "composer footer retained compiled submit binding: {footer}"
    );
}

#[test]
fn new_issue_footer_projects_effective_submit_binding() {
    assert_effective_submit(ComposerFooterCase {
        context: "issues.new-form",
        action: "issues.new-submit",
        mode: FooterMode::IssuesNewComposer,
        screen: ScreenId::Issues,
        chord: "F8",
    });
}

#[test]
fn issue_inline_footer_projects_effective_submit_binding() {
    assert_effective_submit(ComposerFooterCase {
        context: "issues.inline",
        action: "issues.inline-submit",
        mode: FooterMode::IssuesInlineComposer,
        screen: ScreenId::Issues,
        chord: "F9",
    });
}

#[test]
fn pr_inline_footer_projects_effective_submit_binding() {
    assert_effective_submit(ComposerFooterCase {
        context: "prs.inline",
        action: "prs.inline-submit",
        mode: FooterMode::PullRequestsInlineComposer,
        screen: ScreenId::PullRequests,
        chord: "F10",
    });
}

#[test]
fn unbound_composer_submit_is_discoverable() {
    let case = ComposerFooterCase {
        context: "issues.inline",
        action: "issues.inline-submit",
        mode: FooterMode::IssuesInlineComposer,
        screen: ScreenId::Issues,
        chord: "F9",
    };
    let footer = project_composer_footer(&case, &[]);

    assert!(footer.contains("Unbound submit"), "footer: {footer}");
}
