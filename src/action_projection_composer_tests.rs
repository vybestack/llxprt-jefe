use std::collections::BTreeMap;

use super::*;
use crate::domain::action_registry::Provenance;
use crate::domain::default_action_inventory::display::FooterMode;
use crate::state::ScreenId;
use crate::state::keys_editor_project::{ChordText, project_keys};

struct ComposerFooterCase {
    context: &'static str,
    action: &'static str,
    mode: FooterMode,
    screen: ScreenId,
    chord: &'static str,
}

fn published_with_submit_override(
    context: &str,
    action: &str,
    chords: &[&str],
) -> crate::persistence::settings_document::PublishedSettings {
    let mut settings = crate::persistence::settings_document::PublishedSettings::default();
    settings.keymap.insert(
        context.to_owned(),
        BTreeMap::from([(
            action.to_owned(),
            chords.iter().map(|chord| (*chord).to_owned()).collect(),
        )]),
    );
    settings
}

fn snapshot_with_submit_override(
    context: &str,
    action: &str,
    chords: &[&str],
) -> ActionRegistrySnapshot {
    let settings = published_with_submit_override(context, action, chords);
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
            screen: case.screen.into(),
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

struct ListSendProjectionCase {
    context: &'static str,
    action: &'static str,
    mode: FooterMode,
    screen: ScreenId,
}

fn project_list_send_footer(
    snapshot: &ActionRegistrySnapshot,
    case: &ListSendProjectionCase,
) -> String {
    project_footer(
        snapshot,
        FooterProjectionInput {
            screen: case.screen.into(),
            terminal_focused: false,
            shell_overlay_active: false,
            shell_resume_available: false,
            actions_focus: None,
            mode_override: Some(case.mode),
        },
    )
}

#[test]
fn list_send_remaps_project_to_footer_and_help_from_one_snapshot() {
    for case in [
        ListSendProjectionCase {
            context: "issues.list",
            action: "issues.list-send-agent",
            mode: FooterMode::IssuesList,
            screen: ScreenId::Issues,
        },
        ListSendProjectionCase {
            context: "prs.list",
            action: "prs.list-send-agent",
            mode: FooterMode::PullRequestsList,
            screen: ScreenId::PullRequests,
        },
    ] {
        let snapshot = snapshot_with_submit_override(case.context, case.action, &["F8"]);
        let published = published_with_submit_override(case.context, case.action, &["F8"]);
        let footer = project_list_send_footer(&snapshot, &case);
        let help = project_help_lines(&snapshot).join("\n");
        let help_description = if case.context == "issues.list" {
            "Send selected issue to agent"
        } else {
            "Send selected pull request to agent"
        };
        let Some(help_line) = help.lines().find(|line| line.contains(help_description)) else {
            panic!("help did not project list send-to-agent: {help}");
        };

        assert!(footer.contains("F8 send to agent"), "footer: {footer}");
        assert!(!footer.contains("/S send to agent"), "footer: {footer}");
        assert!(help_line.contains("F8"), "help: {help}");
        let keys = project_keys(&snapshot, &published);
        let Some(keys_row) = keys.iter().find(|row| row.action.as_str() == case.action) else {
            panic!("Keys did not project list send-to-agent: {keys:?}");
        };
        assert_eq!(
            keys_row.chords,
            vec![ChordText::Chord(
                Chord::parse("F8").unwrap_or_else(|error| panic!("test chord: {error}"))
            )]
        );
        assert!(
            !footer.contains("Ctrl-s/S send to agent"),
            "footer: {footer}"
        );
    }
}

#[test]
fn unbound_list_send_is_discoverable_in_footer_and_help() {
    for case in [
        ListSendProjectionCase {
            context: "issues.list",
            action: "issues.list-send-agent",
            mode: FooterMode::IssuesList,
            screen: ScreenId::Issues,
        },
        ListSendProjectionCase {
            context: "prs.list",
            action: "prs.list-send-agent",
            mode: FooterMode::PullRequestsList,
            screen: ScreenId::PullRequests,
        },
    ] {
        let snapshot = snapshot_with_submit_override(case.context, case.action, &[]);
        let published = published_with_submit_override(case.context, case.action, &[]);
        let footer = project_list_send_footer(&snapshot, &case);
        let help = project_help_lines(&snapshot).join("\n");
        let help_description = if case.context == "issues.list" {
            "Send selected issue to agent"
        } else {
            "Send selected pull request to agent"
        };
        let Some(help_line) = help.lines().find(|line| line.contains(help_description)) else {
            panic!("help did not project list send-to-agent: {help}");
        };

        assert!(footer.contains("Unbound send to agent"), "footer: {footer}");
        assert!(help_line.contains("Unbound"), "help: {help}");
        let keys = project_keys(&snapshot, &published);
        let Some(keys_row) = keys.iter().find(|row| row.action.as_str() == case.action) else {
            panic!("Keys did not project unbound list send-to-agent: {keys:?}");
        };
        assert!(keys_row.chords.is_empty());
        assert!(
            matches!(keys_row.provenance, Provenance::Settings { .. }),
            "an explicit empty settings binding must remain identified as an override"
        );
        assert!(
            !footer.contains("Ctrl-s/S send to agent"),
            "footer: {footer}"
        );
    }
}

#[test]
fn list_and_detail_footers_project_only_their_active_send_action() {
    for (context, action, screen, list_mode, detail_mode) in [
        (
            "issues.list",
            "issues.list-send-agent",
            ScreenId::Issues,
            FooterMode::IssuesList,
            FooterMode::IssuesDetail,
        ),
        (
            "prs.list",
            "prs.list-send-agent",
            ScreenId::PullRequests,
            FooterMode::PullRequestsList,
            FooterMode::PullRequestsDetail,
        ),
    ] {
        let snapshot = snapshot_with_submit_override(context, action, &["F8"]);
        let list_footer = project_footer(
            &snapshot,
            FooterProjectionInput {
                screen: screen.into(),
                terminal_focused: false,
                shell_overlay_active: false,
                shell_resume_available: false,
                actions_focus: None,
                mode_override: Some(list_mode),
            },
        );
        let detail_footer = project_footer(
            &snapshot,
            FooterProjectionInput {
                screen: screen.into(),
                terminal_focused: false,
                shell_overlay_active: false,
                shell_resume_available: false,
                actions_focus: None,
                mode_override: Some(detail_mode),
            },
        );

        assert!(list_footer.contains("F8 send to agent"), "{list_footer}");
        assert!(!list_footer.contains("/S send to agent"), "{list_footer}");
        assert!(detail_footer.contains("S send to agent"), "{detail_footer}");
        assert!(
            !detail_footer.contains("F8 send to agent"),
            "{detail_footer}"
        );
    }
}

#[test]
fn non_item_focus_footers_do_not_advertise_list_send() {
    let snapshot = test_snapshot();
    for (screen, mode) in [
        (ScreenId::Issues, FooterMode::IssuesRepoList),
        (ScreenId::PullRequests, FooterMode::PullRequestsRepoList),
        (ScreenId::PullRequests, FooterMode::PullRequestsChanges),
    ] {
        let footer = project_footer(
            &snapshot,
            FooterProjectionInput {
                screen: screen.into(),
                terminal_focused: false,
                shell_overlay_active: false,
                shell_resume_available: false,
                actions_focus: None,
                mode_override: Some(mode),
            },
        );

        assert!(!footer.contains("send to agent"), "{mode:?}: {footer}");
    }
}
