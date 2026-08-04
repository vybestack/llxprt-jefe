//! Pure action-registry projections shared by dispatch-adjacent display surfaces.
//!
//! This module is iocraft-free and side-effect-free. It projects one immutable
//! registry snapshot into Help, footer, current/future menu, and future Keys
//! rows without provider, persistence, runtime, or UI I/O.
//!
//! Section ordering, action IDs, and human descriptions come from the canonical
//! inventory display table in [`display`]. Every action-backed chord label is
//! formatted from the snapshot's effective bindings, so settings overrides are
//! reflected without a second chord authority.

use std::fmt::Write as _;

use crate::domain::action_registry::{ActionRegistrySnapshot, Availability};
use crate::domain::default_action_inventory::display::{
    ACTIONS_FOCUS_GROUPS, ActionsFocusKind, FooterDisplayHint, FooterMode, HELP_DISPLAY_LINES,
    HELP_SECTIONS, SHELL_OVERLAY_HINTS, TERMINAL_FOCUSED_HINTS,
};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::{Chord, Key, Modifier, ModifierSet};
use crate::state::{ActionsFocus, ScreenId};

const UNAVAILABLE_PREFIX: &str = "Unavailable: ";

// ── ProjectedAction (menu/keys rows) ──────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedAction {
    id: String,
    contexts: Vec<ContextId>,
    chords: Vec<String>,
    availability: Availability,
}

impl ProjectedAction {
    #[cfg(test)]
    #[must_use]
    fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match &self.availability {
            Availability::Available => None,
            Availability::Unavailable { reason } => Some(reason),
        }
    }

    #[must_use]
    pub fn status(&self) -> String {
        self.reason().map_or_else(
            || "Available".to_owned(),
            |reason| format!("{UNAVAILABLE_PREFIX}{reason}"),
        )
    }

    #[cfg(test)]
    fn applies_to(&self, context: &ContextId) -> bool {
        self.contexts.contains(context)
    }
}

// ── FooterProjectionInput ──────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FooterProjectionInput {
    pub screen: ScreenId,
    pub terminal_focused: bool,
    pub shell_overlay_active: bool,
    pub shell_resume_available: bool,
    pub actions_focus: Option<ActionsFocus>,
    pub mode_override: Option<FooterMode>,
}

// ── Availability lookup from snapshot ──────────────────────────────────────

/// Look up the availability reason for a single action ID in the snapshot.
/// Returns `None` if the action is available or not found.
fn unavailable_reason_for(snapshot: &ActionRegistrySnapshot, action_id: &str) -> Option<String> {
    snapshot
        .availability_entries()
        .iter()
        .find(|entry| entry.action().as_str() == action_id)
        .and_then(|entry| match entry.availability() {
            Availability::Available => None,
            Availability::Unavailable { reason } => Some(reason.clone()),
        })
}

/// Look up the shared "Unavailable: ..." status strings for a group of
/// action IDs. Returns one status per distinct unavailable action reason.
fn unavailable_statuses_for_group_dedup(
    snapshot: &ActionRegistrySnapshot,
    action_ids: &[&str],
) -> Vec<String> {
    let mut statuses = Vec::new();
    for id in action_ids {
        if let Some(reason) = unavailable_reason_for(snapshot, id) {
            let status = format!("{UNAVAILABLE_PREFIX}{reason}");
            if !statuses.contains(&status) {
                statuses.push(status);
            }
        }
    }
    statuses
}

#[derive(Clone, Copy)]
enum ChordSurface {
    Help,
    Footer,
}

fn effective_chords_for(snapshot: &ActionRegistrySnapshot, action_ids: &[&str]) -> Vec<Chord> {
    let mut chords = Vec::new();
    for action_id in action_ids {
        for binding in snapshot
            .effective_bindings()
            .iter()
            .filter(|binding| binding.action.as_str() == *action_id)
        {
            for chord in &binding.chords {
                if !chords.contains(chord) {
                    chords.push(*chord);
                }
            }
        }
    }
    chords
}

fn format_chord(chord: Chord, surface: ChordSurface) -> String {
    let none = ModifierSet::empty();
    let shift = ModifierSet::from_modifier(Modifier::Shift);
    let ctrl = ModifierSet::from_modifier(Modifier::Ctrl);
    let alt = ModifierSet::from_modifier(Modifier::Alt);
    match (chord.modifiers, chord.key, surface) {
        (modifiers, Key::Char(' '), _) if modifiers == none => "Space".to_owned(),
        (modifiers, Key::Char(character), _) if modifiers == shift => {
            character.to_uppercase().collect()
        }
        (modifiers, Key::Char(character), _) if modifiers == ctrl => {
            format!("Ctrl-{}", character.to_ascii_lowercase())
        }
        (modifiers, Key::Char(character), _) if modifiers == alt => format!("⌥{character}"),
        (modifiers, Key::Char(character), _) if modifiers == none => character.to_string(),
        (modifiers, Key::Up, ChordSurface::Footer) if modifiers == none => "^".to_owned(),
        (modifiers, Key::Down, ChordSurface::Footer) if modifiers == none => "v".to_owned(),
        (modifiers, Key::Left, ChordSurface::Footer) if modifiers == none => "<".to_owned(),
        (modifiers, Key::Right, ChordSurface::Footer) if modifiers == none => ">".to_owned(),
        (modifiers, Key::PageUp, _) if modifiers == none => "PgUp".to_owned(),
        (modifiers, Key::PageDown, _) if modifiers == none => "PgDn".to_owned(),
        _ => chord.to_string(),
    }
}

/// Split a label on its last character, not its last byte: a chord label may
/// end in a multi-byte scalar, where byte slicing would panic on the boundary.
fn split_last_char(label: &str) -> Option<(&str, char)> {
    let last = label.chars().next_back()?;
    let boundary = label.len().checked_sub(last.len_utf8())?;
    Some((label.get(..boundary)?, last))
}

fn compact_digit_run(labels: &[String]) -> Option<String> {
    if labels.len() < 3 {
        return None;
    }
    let (prefix, _) = split_last_char(labels.first()?)?;
    let mut digits = Vec::with_capacity(labels.len());
    for label in labels {
        let (candidate_prefix, digit) = split_last_char(label)?;
        let value = u8::try_from(digit.to_digit(10)?).ok()?;
        if candidate_prefix != prefix {
            return None;
        }
        digits.push(value);
    }
    if !digits.windows(2).all(|pair| pair[1] == pair[0] + 1) {
        return None;
    }
    Some(format!(
        "{prefix}{}-{}",
        digits[0],
        digits[digits.len() - 1]
    ))
}

fn format_action_chords(
    snapshot: &ActionRegistrySnapshot,
    action_ids: &[&str],
    surface: ChordSurface,
) -> String {
    let mut labels = Vec::new();
    for chord in effective_chords_for(snapshot, action_ids) {
        let label = format_chord(chord, surface);
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    compact_digit_run(&labels).unwrap_or_else(|| labels.join("/"))
}

// ── Project action/keys/menu rows (from snapshot directly) ─────────────────

#[must_use]
pub fn project_action_rows(snapshot: &ActionRegistrySnapshot) -> Vec<ProjectedAction> {
    snapshot
        .actions()
        .iter()
        .filter_map(|action| {
            let availability = snapshot
                .availability_entries()
                .iter()
                .find(|entry| entry.action() == &action.id)?
                .availability()
                .clone();
            let chords = snapshot
                .effective_bindings()
                .iter()
                .filter(|binding| binding.action == action.id)
                .flat_map(|binding| binding.chords.iter().map(ToString::to_string))
                .collect();
            Some(ProjectedAction {
                id: action.id.as_str().to_owned(),
                contexts: action.contexts.clone(),
                chords,
                availability,
            })
        })
        .collect()
}

/// Project keys rows for the future Keys editor and menu (S6+). Currently
/// exercised by the five-consumer availability test.
#[cfg(test)]
#[must_use]
fn project_keys_rows(snapshot: &ActionRegistrySnapshot) -> Vec<ProjectedAction> {
    project_action_rows(snapshot)
}

/// Project menu rows for one context, filtering to actions that apply.
#[cfg(test)]
#[must_use]
fn project_menu_rows(
    snapshot: &ActionRegistrySnapshot,
    context: &ContextId,
) -> Vec<ProjectedAction> {
    project_keys_rows(snapshot)
        .into_iter()
        .filter(|row| row.applies_to(context))
        .collect()
}

// ── Help projection (from display table + snapshot availability) ────────────

#[must_use]
pub fn project_help_lines(snapshot: &ActionRegistrySnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_section: Option<u8> = None;
    for line in sorted_help_lines() {
        if current_section != Some(line.section) {
            let heading = HELP_SECTIONS
                .iter()
                .find(|(_, idx)| *idx == line.section)
                .map(|(heading, _)| *heading);
            if let Some(heading) = heading {
                lines.push(heading.to_owned());
            }
            current_section = Some(line.section);
        }
        lines.push(render_help_line(snapshot, &line));
        if !line.actions.is_empty() {
            for status in unavailable_statuses_for_group_dedup(snapshot, line.actions) {
                if !lines.contains(&status) {
                    lines.push(status);
                }
            }
        }
    }
    lines
}

fn render_help_line(
    snapshot: &ActionRegistrySnapshot,
    line: &crate::domain::default_action_inventory::display::HelpDisplayLine,
) -> String {
    if line.actions.is_empty() {
        return line.description.to_owned();
    }
    let chords = format_action_chords(snapshot, line.actions, ChordSurface::Help);
    let prefix = if chords.is_empty() {
        "Unbound"
    } else {
        &chords
    };
    format!("  {prefix:<12}{}", line.description)
}

// Sort helper for footer hints by their declared order.
fn sorted_hints(hints: &[FooterDisplayHint]) -> Vec<FooterDisplayHint> {
    let mut sorted: Vec<FooterDisplayHint> = hints.to_vec();
    sorted.sort_by_key(|hint| hint.order);
    sorted
}

// Sort helper for help lines by section then declared order.
fn sorted_help_lines() -> Vec<crate::domain::default_action_inventory::display::HelpDisplayLine> {
    use crate::domain::default_action_inventory::display::HelpDisplayLine;
    let mut sorted: Vec<HelpDisplayLine> = HELP_DISPLAY_LINES.to_vec();
    sorted.sort_by_key(|line| (line.section, line.order));
    sorted
}

// ── Footer projection (from display table + snapshot availability) ──────────

#[must_use]
pub fn project_footer(snapshot: &ActionRegistrySnapshot, input: FooterProjectionInput) -> String {
    let mode = input
        .mode_override
        .unwrap_or_else(|| footer_mode(input.screen));
    let hints = if input.shell_overlay_active {
        sorted_hints(SHELL_OVERLAY_HINTS)
    } else if input.terminal_focused {
        sorted_hints(TERMINAL_FOCUSED_HINTS)
    } else {
        footer_hints(mode, input.actions_focus)
    };
    let active_contexts = if input.shell_overlay_active {
        &["shell-overlay"][..]
    } else if input.terminal_focused {
        &["terminal", "global"][..]
    } else {
        footer_contexts(mode, input.actions_focus)
    };
    let mut parts = annotate_hints_with_status(
        snapshot,
        &hints,
        input.shell_resume_available && !input.shell_overlay_active,
        active_contexts,
    );
    if !input.shell_overlay_active && !input.terminal_focused {
        append_unlisted_unavailable_statuses(snapshot, mode, input.actions_focus, &mut parts);
    }
    parts.join(" | ")
}

fn footer_mode(screen: ScreenId) -> FooterMode {
    match screen {
        ScreenId::Dashboard => FooterMode::Dashboard,
        ScreenId::Repositories => FooterMode::Split,
        ScreenId::Issues => FooterMode::Issues,
        ScreenId::PullRequests => FooterMode::PullRequests,
        ScreenId::Actions => FooterMode::Actions,
        ScreenId::Errors => FooterMode::Errors,
        ScreenId::Terminals => FooterMode::Terminals,
        ScreenId::Settings => FooterMode::Settings,
    }
}

fn footer_hints(mode: FooterMode, actions_focus: Option<ActionsFocus>) -> Vec<FooterDisplayHint> {
    if mode == FooterMode::Actions {
        let focus = match actions_focus {
            Some(ActionsFocus::RepoList) => ActionsFocusKind::RepoList,
            Some(ActionsFocus::RunList) | None => ActionsFocusKind::RunList,
            Some(ActionsFocus::Detail) => ActionsFocusKind::Detail,
        };
        ACTIONS_FOCUS_GROUPS
            .iter()
            .find(|group| group.focus == focus)
            .map_or_else(Vec::new, |group| sorted_hints(group.hints))
    } else {
        sorted_hints(footer_hints_for_mode(footer_display_mode(mode)))
    }
}

fn footer_display_mode(mode: FooterMode) -> FooterMode {
    match mode {
        FooterMode::IssuesRepoList | FooterMode::IssuesList | FooterMode::IssuesDetail => {
            FooterMode::Issues
        }
        FooterMode::PullRequestsRepoList
        | FooterMode::PullRequestsList
        | FooterMode::PullRequestsDetail
        | FooterMode::PullRequestsChanges => FooterMode::PullRequests,
        other => other,
    }
}

fn annotate_hints_with_status(
    snapshot: &ActionRegistrySnapshot,
    hints: &[FooterDisplayHint],
    use_resume_description: bool,
    active_contexts: &[&str],
) -> Vec<String> {
    hints
        .iter()
        .filter_map(|hint| {
            render_footer_hint(snapshot, hint, use_resume_description, active_contexts)
        })
        .collect()
}

fn render_footer_hint(
    snapshot: &ActionRegistrySnapshot,
    hint: &FooterDisplayHint,
    use_resume_description: bool,
    active_contexts: &[&str],
) -> Option<String> {
    let description = if use_resume_description {
        hint.resume_description.unwrap_or(hint.description)
    } else {
        hint.description
    };
    let actions = actions_for_contexts(snapshot, hint.actions, active_contexts);
    if !hint.actions.is_empty() && actions.is_empty() {
        return None;
    }
    let mut rendered = if hint.actions.is_empty() {
        description.to_owned()
    } else {
        let chords = format_action_chords(snapshot, &actions, ChordSurface::Footer);
        let prefix = if chords.is_empty() {
            "Unbound"
        } else {
            &chords
        };
        format!("{prefix} {description}")
    };
    let statuses = unavailable_statuses_for_group_dedup(snapshot, &actions);
    if !statuses.is_empty() {
        let _ = write!(rendered, " [{}]", statuses.join(" / "));
    }
    Some(rendered)
}

fn actions_for_contexts<'a>(
    snapshot: &ActionRegistrySnapshot,
    action_ids: &'a [&'a str],
    active_contexts: &[&str],
) -> Vec<&'a str> {
    action_ids
        .iter()
        .copied()
        .filter(|action_id| {
            snapshot
                .actions()
                .iter()
                .find(|action| action.id.as_str() == *action_id)
                .is_some_and(|action| {
                    action
                        .contexts
                        .iter()
                        .any(|context| active_contexts.contains(&context.as_str()))
                })
        })
        .collect()
}

fn append_unlisted_unavailable_statuses(
    snapshot: &ActionRegistrySnapshot,
    mode: FooterMode,
    actions_focus: Option<ActionsFocus>,
    parts: &mut Vec<String>,
) {
    let mode_contexts = footer_contexts(mode, actions_focus);
    let rows = project_action_rows(snapshot);
    for row in rows.iter().filter(|row| {
        row.reason().is_some()
            && row
                .contexts
                .iter()
                .any(|context| mode_contexts.contains(&context.as_str()))
    }) {
        let status = row.status();
        if !parts.contains(&status) {
            parts.push(status);
        }
    }
}

/// Get the footer hints for a non-Actions mode.
fn footer_hints_for_mode(mode: FooterMode) -> &'static [FooterDisplayHint] {
    crate::domain::default_action_inventory::display::FOOTER_MODE_GROUPS
        .iter()
        .find(|group| group.mode == mode)
        .map_or(&[], |group| group.hints)
}

fn footer_contexts(
    mode: FooterMode,
    actions_focus: Option<ActionsFocus>,
) -> &'static [&'static str] {
    match mode {
        FooterMode::Dashboard => &["dashboard", "global"],
        FooterMode::Split => &["split", "global"],
        FooterMode::Issues => &["issues.list", "issues.detail", "issues", "global"],
        FooterMode::IssuesRepoList => &["issues.repo-list", "issues", "global"],
        FooterMode::IssuesList => &["issues.list", "issues", "global"],
        FooterMode::IssuesDetail => &["issues.detail", "issues", "global"],
        FooterMode::IssuesNewComposer => &["issues.new-form", "global"],
        FooterMode::IssuesInlineComposer => &["issues.inline", "global"],
        FooterMode::PullRequests => &["prs.repo-list", "prs.list", "prs.detail", "prs", "global"],
        FooterMode::PullRequestsRepoList => &["prs.repo-list", "prs", "global"],
        FooterMode::PullRequestsList => &["prs.list", "prs", "global"],
        FooterMode::PullRequestsDetail => &["prs.detail", "prs", "global"],
        FooterMode::PullRequestsChanges => &["prs.changes", "prs", "global"],
        FooterMode::PullRequestsInlineComposer => &["prs.inline", "global"],
        FooterMode::PullRequestsNewComposer => &["prs.new-form", "global"],
        FooterMode::Actions => match actions_focus {
            Some(ActionsFocus::RepoList) => &["actions.repo-list", "actions", "global"],
            Some(ActionsFocus::Detail) => &["actions.detail", "actions", "global"],
            Some(ActionsFocus::RunList) | None => &["actions.run-list", "actions", "global"],
        },
        FooterMode::Errors => &["errors", "global"],
        FooterMode::Terminals => &["terminal-manager", "global"],
        FooterMode::Settings => &["settings", "global"],
    }
}

// ── Test helpers ───────────────────────────────────────────────────────────

#[cfg(test)]
pub fn test_snapshot() -> ActionRegistrySnapshot {
    let result = crate::persistence::keymap_edit::compose_published(
        &crate::persistence::settings_document::PublishedSettings::default(),
        "test",
    );
    let Ok(composed) = result else {
        panic!("test action snapshot must compose: {result:?}");
    };
    composed.snapshot().clone()
}

#[cfg(test)]
#[path = "action_projection_composer_tests.rs"]
mod composer_tests;

#[cfg(test)]
mod tests {
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

    fn fixture(
        unavailable_id: Option<&str>,
    ) -> crate::domain::action_registry::ActionRegistrySnapshot {
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
            lines.iter().any(|line| line.starts_with("  R")
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
        // Scan only the production portion of this file (before #[cfg(test)]).
        let source = include_str!("action_projection.rs");
        let prod = source.split("#[cfg(test)]").next().unwrap_or(source);
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
}
