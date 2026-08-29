//! Pure action-registry projections shared by dispatch-adjacent display surfaces.
//!
//! This module is iocraft-free and side-effect-free. It projects one immutable
//! registry snapshot plus an optional validated runtime availability generation
//! into Help, footer, menu, action, and Keys rows without provider, persistence,
//! runtime, or UI I/O.
//!
//! Section ordering, action IDs, and human descriptions come from the inventory
//! display table in [`display`]. Every action-backed chord label is formatted
//! from the committed snapshot's effective bindings, while runtime health may
//! only make a committed-available action unavailable.

use std::fmt::Write as _;

use crate::domain::action_registry::{
    ActionId, ActionRegistrySnapshot, Availability, AvailabilityGeneration,
};
use crate::domain::default_action_inventory::display::{
    ACTIONS_FOCUS_GROUPS, ActionsFocusKind, FooterDisplayHint, FooterMode, HELP_DISPLAY_LINES,
    HELP_SECTIONS, SHELL_OVERLAY_HINTS, TERMINAL_FOCUSED_HINTS,
};
use crate::domain::input_context::ContextId;
use crate::domain::keymap::{Chord, Key, Modifier, ModifierSet};
use crate::state::{ActionsFocus, ScreenId};
use crate::workbench::ScreenIdentity;

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
    pub screen: ScreenIdentity,
    pub terminal_focused: bool,
    pub shell_overlay_active: bool,
    pub shell_resume_available: bool,
    pub actions_focus: Option<ActionsFocus>,
    pub mode_override: Option<FooterMode>,
}

// ── Availability lookup from snapshot ──────────────────────────────────────

pub fn effective_action_availability<'a>(
    snapshot: &'a ActionRegistrySnapshot,
    runtime: Option<&'a AvailabilityGeneration>,
    action: &ActionId,
) -> Option<&'a Availability> {
    snapshot.effective_availability_of(runtime, action)
}

/// Look up the effective availability reason for a single action ID.
/// Returns `None` if the action is available or not found.
fn unavailable_reason_for(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
    action_id: &str,
) -> Option<String> {
    snapshot
        .actions()
        .iter()
        .find(|action| action.id.as_str() == action_id)
        .and_then(|action| effective_action_availability(snapshot, runtime, &action.id))
        .and_then(|availability| match availability {
            Availability::Available => None,
            Availability::Unavailable { reason } => Some(reason.clone()),
        })
}

/// Look up the shared "Unavailable: ..." status strings for a group of
/// action IDs. Returns one status per distinct unavailable action reason.
fn unavailable_statuses_for_group_dedup(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
    action_ids: &[&str],
) -> Vec<String> {
    let mut statuses = Vec::new();
    for id in action_ids {
        if let Some(reason) = unavailable_reason_for(snapshot, runtime, id) {
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

#[cfg(test)]
#[must_use]
fn project_action_rows(snapshot: &ActionRegistrySnapshot) -> Vec<ProjectedAction> {
    project_action_rows_effective(snapshot, None)
}

/// Project action rows using the committed graph plus a validated runtime-only
/// availability generation.
#[must_use]
pub fn project_action_rows_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
) -> Vec<ProjectedAction> {
    snapshot
        .actions()
        .iter()
        .filter_map(|action| {
            let availability =
                effective_action_availability(snapshot, runtime, &action.id)?.clone();
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

#[cfg(test)]
#[must_use]
pub fn project_help_lines(snapshot: &ActionRegistrySnapshot) -> Vec<String> {
    project_help_lines_effective(snapshot, None)
}

/// Project Help from committed declarations and one validated runtime-only
/// availability generation.
#[must_use]
pub fn project_help_lines_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
) -> Vec<String> {
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
            for status in unavailable_statuses_for_group_dedup(snapshot, runtime, line.actions) {
                if !lines.contains(&status) {
                    lines.push(status);
                }
            }
        }
    }
    lines
}

/// The Help heading under which package-contributed actions are listed.
const PACKAGE_SECTION: &str = "Packages:";

/// Project the package-contributed actions a trusted package published
/// (issue #390 CW-10, row CW10-13).
///
/// Compiled actions are described by a compiled display table; a package action
/// is not in that table because it did not exist when this binary was built.
/// Its rows therefore come from the snapshot itself — the same authority a
/// keybind resolves against — so the reason an action cannot run is one string
/// with one owner rather than two that can drift.
///
/// An empty result is correct and deliberate: with no packages there is no
/// section, rather than a heading over nothing.
#[cfg(test)]
#[must_use]
pub fn project_provider_help_lines(snapshot: &ActionRegistrySnapshot) -> Vec<String> {
    project_provider_help_lines_effective(snapshot, None)
}

/// Project package Help rows from committed declarations and one validated
/// runtime-only availability generation.
#[must_use]
pub fn project_provider_help_lines_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
) -> Vec<String> {
    let mut rows: Vec<(&str, String)> = snapshot
        .provider_actions()
        .map(|action| {
            let chords = format_action_chords(
                snapshot,
                std::slice::from_ref(&action.id.as_str()),
                ChordSurface::Help,
            );
            let prefix = if chords.is_empty() {
                "Unbound".to_owned()
            } else {
                chords
            };
            let reason = effective_action_availability(snapshot, runtime, &action.id).and_then(
                |availability| match availability {
                    Availability::Available => None,
                    Availability::Unavailable { reason } => Some(reason.clone()),
                },
            );
            let suffix = reason.map_or_else(String::new, |reason| {
                format!("  ({UNAVAILABLE_PREFIX}{reason})")
            });
            (
                action.id.as_str(),
                format!("  {prefix:<12}{}{suffix}", action.label),
            )
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    rows.sort_by(|left, right| left.0.cmp(right.0));
    let mut lines = vec![PACKAGE_SECTION.to_owned()];
    lines.extend(rows.into_iter().map(|(_, line)| line));
    lines
}

/// Project the complete ordered Help content from committed declarations plus
/// one validated runtime-only availability generation.
#[must_use]
pub fn project_help_content_lines_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
) -> Vec<String> {
    let mut lines = project_help_lines_effective(snapshot, runtime);
    lines.extend(project_provider_help_lines_effective(snapshot, runtime));
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

#[cfg(test)]
#[must_use]
pub fn project_footer(snapshot: &ActionRegistrySnapshot, input: FooterProjectionInput) -> String {
    project_footer_effective(snapshot, None, input)
}

/// Project the footer from committed declarations and one validated runtime-only
/// availability generation.
#[must_use]
pub fn project_footer_effective(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
    input: FooterProjectionInput,
) -> String {
    let Some(mode) = input.mode_override.or_else(|| footer_mode(input.screen)) else {
        return String::new();
    };
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
        runtime,
        &hints,
        input.shell_resume_available && !input.shell_overlay_active,
        active_contexts,
    );
    if !input.shell_overlay_active && !input.terminal_focused {
        append_unlisted_unavailable_statuses(
            snapshot,
            runtime,
            mode,
            input.actions_focus,
            &mut parts,
        );
    }
    parts.join(" | ")
}

fn footer_mode(screen: ScreenIdentity) -> Option<FooterMode> {
    Some(match screen.compiled()? {
        ScreenId::Repositories => FooterMode::Split,
        ScreenId::Issues => FooterMode::Issues,
        ScreenId::PullRequests => FooterMode::PullRequests,
        ScreenId::Actions => FooterMode::Actions,
        ScreenId::Errors => FooterMode::Errors,
        ScreenId::Terminals => FooterMode::Terminals,
        ScreenId::Settings => FooterMode::Settings,
    })
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
    runtime: Option<&AvailabilityGeneration>,
    hints: &[FooterDisplayHint],
    use_resume_description: bool,
    active_contexts: &[&str],
) -> Vec<String> {
    hints
        .iter()
        .filter_map(|hint| {
            render_footer_hint(
                snapshot,
                runtime,
                hint,
                use_resume_description,
                active_contexts,
            )
        })
        .collect()
}

fn render_footer_hint(
    snapshot: &ActionRegistrySnapshot,
    runtime: Option<&AvailabilityGeneration>,
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
    let statuses = unavailable_statuses_for_group_dedup(snapshot, runtime, &actions);
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
    runtime: Option<&AvailabilityGeneration>,
    mode: FooterMode,
    actions_focus: Option<ActionsFocus>,
    parts: &mut Vec<String>,
) {
    let mode_contexts = footer_contexts(mode, actions_focus);
    let rows = project_action_rows_effective(snapshot, runtime);
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
#[path = "action_projection_provider_tests.rs"]
mod provider_tests;

#[cfg(test)]
#[path = "action_projection_tests.rs"]
mod tests;
