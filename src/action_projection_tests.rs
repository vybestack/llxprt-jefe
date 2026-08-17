use std::collections::BTreeMap;

use super::*;
use crate::domain::Id;
use crate::domain::action_registry::{
    ActionAvailability, Availability, AvailabilityGeneration, RegistryCandidate, Resolution,
};
use crate::domain::default_action_inventory::compiled_inventory;
use crate::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
use crate::domain::input_context::{ContextId, ContextStack};
use crate::domain::keymap::Chord;
use crate::state::{ActionsFocus, ScreenId};

const REASON: &str = "This section is read-only";
const STATUS: &str = "Unavailable: This section is read-only";

fn fixture(unavailable_id: Option<&str>) -> crate::domain::action_registry::ActionRegistrySnapshot {
    let result = compiled_inventory();
    let Ok(inventory) = result else {
        panic!("compiled inventory must build: {result:?}");
    };
    let owner = Id::parse("core.keymap");
    let Ok(owner) = owner else {
        panic!("builtin owner must parse: {owner:?}");
    };
    let entries = inventory
        .actions
        .iter()
        .map(|action| {
            let availability = if unavailable_id == Some(action.id.as_str()) {
                Availability::Unavailable {
                    reason: REASON.to_owned(),
                }
            } else {
                Availability::Available
            };
            ActionAvailability::new(action.id.clone(), availability)
        })
        .collect();
    let generation = AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(91),
            owner,
            screen_generation: 7,
            activation_generation: 11,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "action-availability"),
        },
        entries,
    );
    let composed = RegistryCandidate::new(
        inventory.actions,
        inventory.bindings,
        Vec::new(),
        inventory.context_stacks,
        generation,
    )
    .compose();
    let Ok(snapshot) = composed else {
        panic!("projection fixture must compose: {composed:?}");
    };
    snapshot
}

fn runtime_generation(
    snapshot: &ActionRegistrySnapshot,
    unavailable_id: Option<&str>,
    reason: &str,
) -> AvailabilityGeneration {
    let owner = Id::parse("core.keymap").unwrap_or_else(|error| {
        panic!("builtin owner must parse: {error}");
    });
    let entries = snapshot
        .actions()
        .iter()
        .map(|action| {
            let availability = if unavailable_id == Some(action.id.as_str()) {
                Availability::Unavailable {
                    reason: reason.to_owned(),
                }
            } else {
                Availability::Available
            };
            ActionAvailability::new(action.id.clone(), availability)
        })
        .collect();
    AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(92),
            owner,
            screen_generation: 8,
            activation_generation: 12,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "runtime-availability"),
        },
        entries,
    )
}

fn pr_footer_input() -> FooterProjectionInput {
    FooterProjectionInput {
        screen: ScreenId::PullRequests,
        terminal_focused: false,
        shell_overlay_active: false,
        shell_resume_available: false,
        actions_focus: None,
        mode_override: None,
    }
}

fn snapshot_with_dashboard_terminal_override() -> ActionRegistrySnapshot {
    let mut settings = crate::persistence::settings_document::PublishedSettings::default();
    settings.keymap.insert(
        "dashboard".to_owned(),
        BTreeMap::from([("dashboard.toggle-terminal".to_owned(), vec!["z".to_owned()])]),
    );
    let composed = crate::persistence::keymap_edit::compose_published(&settings, "settings");
    let Ok(composed) = composed else {
        panic!("override fixture must compose: {composed:?}");
    };
    composed.snapshot().clone()
}

#[test]
fn availability_projection_is_byte_identical_across_five_consumers() {
    let snapshot = fixture(Some("prs.edit"));
    let context = ContextId::parse("prs.detail");
    let Ok(context) = context else {
        panic!("context must parse: {context:?}");
    };
    let stack = ContextStack::from_ordered(["prs.detail"], false);
    let Ok(stack) = stack else {
        panic!("context stack must build: {stack:?}");
    };
    let chord = Chord::parse("e");
    let Ok(chord) = chord else {
        panic!("chord must parse: {chord:?}");
    };
    let Resolution::Unavailable { reason, .. } = snapshot.resolve(&chord, &stack) else {
        panic!("fixture action must resolve unavailable");
    };

    let help = project_help_lines(&snapshot);
    let footer = project_footer(&snapshot, pr_footer_input());
    let menu = project_menu_rows(&snapshot, &context);
    let keys = project_keys_rows(&snapshot);
    let projected = project_action_rows(&snapshot);
    let row = projected.iter().find(|row| row.id() == "prs.edit");
    let Some(row) = row else {
        panic!("projected action row must remain visible");
    };

    assert_eq!(reason, REASON);
    assert_eq!(row.reason(), Some(reason.as_str()));
    assert_eq!(row.status(), STATUS);
    assert!(help.iter().any(|line| line == STATUS));
    assert!(footer.contains(STATUS));
    assert!(menu.iter().any(|row| row.status() == STATUS));
    assert!(keys.iter().any(|row| row.status() == STATUS));
}
#[test]
fn runtime_unavailability_is_identical_in_help_footer_action_and_keys_rows() {
    const RUNTIME_REASON: &str = "Provider generation is unhealthy";
    let snapshot = fixture(None);
    let runtime = runtime_generation(&snapshot, Some("prs.edit"), RUNTIME_REASON);
    let status = format!("Unavailable: {RUNTIME_REASON}");

    let help = project_help_lines_effective(&snapshot, Some(&runtime));
    let footer = project_footer_effective(&snapshot, Some(&runtime), pr_footer_input());
    let projected = project_action_rows_effective(&snapshot, Some(&runtime));
    let keys = crate::state::keys_editor_project::project_keys_effective(
        &snapshot,
        Some(&runtime),
        &crate::persistence::settings_document::PublishedSettings::default(),
    );
    let row = projected.iter().find(|row| row.id() == "prs.edit");
    let Some(row) = row else {
        panic!("projected action row must remain visible");
    };
    let key = keys.iter().find(|row| row.action.as_str() == "prs.edit");
    let Some(key) = key else {
        panic!("Keys row must remain visible");
    };

    assert_eq!(row.reason(), Some(RUNTIME_REASON));
    assert_eq!(row.status(), status);
    assert!(help.iter().any(|line| line == &status));
    assert!(footer.contains(&status));
    assert_eq!(
        key.availability,
        Availability::Unavailable {
            reason: RUNTIME_REASON.to_owned(),
        }
    );
}

#[test]
fn runtime_availability_does_not_override_committed_unavailability() {
    let snapshot = fixture(Some("prs.edit"));
    let runtime = runtime_generation(&snapshot, None, "unused");

    let help = project_help_lines_effective(&snapshot, Some(&runtime));
    let footer = project_footer_effective(&snapshot, Some(&runtime), pr_footer_input());
    let projected = project_action_rows_effective(&snapshot, Some(&runtime));
    let keys = crate::state::keys_editor_project::project_keys_effective(
        &snapshot,
        Some(&runtime),
        &crate::persistence::settings_document::PublishedSettings::default(),
    );
    let row = projected.iter().find(|row| row.id() == "prs.edit");
    let Some(row) = row else {
        panic!("projected action row must remain visible");
    };
    let key = keys.iter().find(|row| row.action.as_str() == "prs.edit");
    let Some(key) = key else {
        panic!("Keys row must remain visible");
    };

    assert_eq!(row.reason(), Some(REASON));
    assert_eq!(row.status(), STATUS);
    assert!(help.iter().any(|line| line == STATUS));
    assert!(footer.contains(STATUS));
    assert_eq!(
        key.availability,
        Availability::Unavailable {
            reason: REASON.to_owned(),
        }
    );
}

#[test]
fn actions_footer_appends_unavailable_status_only_for_the_active_focus() {
    let snapshot = fixture(Some("actions.run-up"));
    let footer = project_footer(
        &snapshot,
        FooterProjectionInput {
            screen: ScreenId::Actions,
            terminal_focused: false,
            shell_overlay_active: false,
            shell_resume_available: false,
            actions_focus: Some(ActionsFocus::Detail),
            mode_override: None,
        },
    );

    assert!(!footer.contains(STATUS));
}

#[test]
fn available_projection_preserves_existing_help_and_footer_bytes() {
    let snapshot = fixture(None);
    assert_eq!(
        project_footer(
            &snapshot,
            FooterProjectionInput {
                screen: ScreenId::Repositories,
                terminal_focused: false,
                shell_overlay_active: false,
                shell_resume_available: false,
                actions_focus: Some(ActionsFocus::RunList),
                mode_override: None,
            },
        ),
        "^/k/v/j select | g/G grab | m move | Esc back | ?/h/H/F1 help | Ctrl-q quit | qqq quit"
    );
    let lines = project_help_lines(&snapshot);
    assert_eq!(lines.first().map(String::as_str), Some("Navigation:"));
    assert!(lines.iter().any(|line| line == "  e           Edit"));
    assert!(lines.iter().all(|line| !line.contains("Unavailable:")));
    assert!(lines.iter().any(|line| {
        line.starts_with("  Left/Right/Tab/BackTab") && line.ends_with("Switch pane")
    }));
    assert!(lines.iter().any(|line| {
        line.starts_with("  Tab/j/BackTab/k")
            && line.ends_with("Focus next / previous detail section")
    }));
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("  R")
                && line.ends_with("Resolve / unresolve review thread"))
    );
}

#[test]
fn settings_override_replaces_compiled_chord_in_help_and_footer() {
    let snapshot = snapshot_with_dashboard_terminal_override();
    let help = project_help_lines(&snapshot);
    let Some(help_line) = help
        .iter()
        .find(|line| line.contains("Toggle terminal focus"))
    else {
        panic!("Help must retain the terminal-focus action row");
    };
    let footer = project_footer(
        &snapshot,
        FooterProjectionInput {
            screen: ScreenId::Dashboard,
            terminal_focused: false,
            shell_overlay_active: false,
            shell_resume_available: false,
            actions_focus: None,
            mode_override: None,
        },
    );

    assert_eq!(help_line, "  z           Toggle terminal focus");
    let terminal_hint = footer
        .split(" | ")
        .find(|hint| hint.ends_with("terminal focus"));
    assert_eq!(terminal_hint, Some("t/T/z terminal focus"));
    assert!(
        !help_line.contains("F12"),
        "Help retained compiled chord: {help_line}"
    );
    assert!(
        !footer.contains("F12"),
        "footer retained compiled chord: {footer}"
    );
}

// ── Structural tests rejecting hardcoded chord-action maps ──────────────

/// Reject any hardcoded chord→action mapping. The projection module must
/// not contain a static map from chord strings to action IDs; all such
/// authority lives in the immutable snapshot.
#[test]
fn projection_has_no_hardcoded_chord_action_map() {
    // Scan the complete module: production helpers may legitimately carry
    // item-level `#[cfg(test)]`, so splitting at the first such attribute can
    // discard nearly all production code and make this guard vacuous.
    let prod = include_str!("action_projection.rs");
    // Must not declare a static help-lines const binding.
    assert!(
        !prod.contains("const HELP_LINES"),
        "projection production code must not declare a static help-lines const"
    );
    // Must not declare a static footer_base function.
    assert!(
        !prod.contains("fn footer_base"),
        "projection production code must not declare a hardcoded footer_base fn"
    );
    // Must not declare a HelpLine struct used as static authority.
    assert!(
        !prod.contains("struct HelpLine"),
        "projection must not declare a HelpLine struct"
    );
    // Must derive display from the canonical inventory display table.
    assert!(
        prod.contains("HELP_DISPLAY_LINES"),
        "projection must use the canonical HELP_DISPLAY_LINES table"
    );
    assert!(
        prod.contains("FOOTER_MODE_GROUPS") || prod.contains("ACTIONS_FOCUS_GROUPS"),
        "projection must use canonical footer display groups"
    );
}

/// Every action with a binding that appears in the help display must be
/// accounted for: the action ID referenced in the display table must
/// exist in the compiled inventory.
#[test]
fn displayed_help_action_ids_are_complete() {
    let result = compiled_inventory();
    let Ok(inventory) = result else {
        panic!("inventory must compile: {result:?}");
    };
    let inventory_ids: std::collections::HashSet<&str> = inventory
        .actions
        .iter()
        .map(|action| action.id.as_str())
        .collect();
    for line in HELP_DISPLAY_LINES {
        for action_id in line.actions {
            assert!(
                inventory_ids.contains(action_id),
                "help display references unknown action '{action_id}'"
            );
        }
    }
}

#[test]
fn action_backed_display_metadata_contains_no_known_chord_literals() {
    const CHORD_LITERALS: &[&str] = &[
        "F1",
        "F7",
        "F8",
        "F9",
        "F10",
        "F12",
        "Up/Down",
        "Left/Right",
        "Enter",
        "Esc",
        "Tab",
        "BackTab",
        "PgUp",
        "PgDn",
        "PageUp",
        "PageDown",
        "Ctrl-",
        "ctrl-",
        "⌥",
        "^/",
        "</>",
    ];
    for line in HELP_DISPLAY_LINES
        .iter()
        .filter(|line| !line.actions.is_empty())
    {
        assert_no_chord_literals(line.description, line.actions, CHORD_LITERALS);
    }
    for hint in all_footer_display_hints().filter(|hint| !hint.actions.is_empty()) {
        assert_no_chord_literals(hint.description, hint.actions, CHORD_LITERALS);
        if let Some(description) = hint.resume_description {
            assert_no_chord_literals(description, hint.actions, CHORD_LITERALS);
        }
    }
}

fn assert_no_chord_literals(description: &str, actions: &[&str], literals: &[&str]) {
    for literal in literals {
        assert!(
            !description.contains(literal),
            "action-backed metadata {actions:?} embeds chord literal '{literal}': {description}"
        );
    }
}

fn all_footer_display_hints() -> impl Iterator<Item = &'static FooterDisplayHint> {
    use crate::domain::default_action_inventory::display::FOOTER_MODE_GROUPS;

    FOOTER_MODE_GROUPS
        .iter()
        .flat_map(|group| group.hints.iter())
        .chain(
            ACTIONS_FOCUS_GROUPS
                .iter()
                .flat_map(|group| group.hints.iter()),
        )
        .chain(SHELL_OVERLAY_HINTS.iter())
        .chain(TERMINAL_FOCUSED_HINTS.iter())
}

/// Every action with a binding that appears in the footer display must be
/// accounted for: the action ID referenced in the display table must
/// exist in the compiled inventory.
#[test]
fn displayed_footer_action_ids_are_complete() {
    let result = compiled_inventory();
    let Ok(inventory) = result else {
        panic!("inventory must compile: {result:?}");
    };
    let inventory_ids: std::collections::HashSet<&str> = inventory
        .actions
        .iter()
        .map(|action| action.id.as_str())
        .collect();
    for group in crate::domain::default_action_inventory::display::FOOTER_MODE_GROUPS {
        for hint in group.hints {
            for action_id in hint.actions {
                assert!(
                    inventory_ids.contains(action_id),
                    "footer display references unknown action '{action_id}' in mode {:?}",
                    group.mode
                );
            }
        }
    }
    for focus_group in ACTIONS_FOCUS_GROUPS {
        for hint in focus_group.hints {
            for action_id in hint.actions {
                assert!(
                    inventory_ids.contains(action_id),
                    "actions footer display references unknown action '{action_id}' in focus {:?}",
                    focus_group.focus
                );
            }
        }
    }
    for hint in SHELL_OVERLAY_HINTS.iter().chain(TERMINAL_FOCUSED_HINTS) {
        for action_id in hint.actions {
            assert!(
                inventory_ids.contains(action_id),
                "special footer display references unknown action '{action_id}'"
            );
        }
    }
}
