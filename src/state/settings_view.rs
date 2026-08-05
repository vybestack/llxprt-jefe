//! Pure projection of the Settings screen into renderable rows.
//!
//! This is the iocraft-free view model. It is also the reducer's authority for
//! how many rows a section has, so the selection cannot point at a row the
//! renderer does not draw — one list, one answer, no drift.
//!
//! The Appearance theme rows are the theme picker's projection moved here: same
//! slug, name, selection and active marker, plus the availability the picker
//! could not express because it only ever listed installed themes.

use crate::domain::action_registry::ActionId;
use crate::domain::agent_definition::AgentTypeId;
use crate::domain::input_context::ContextId;
use crate::domain::{Id, ThemeId};
use crate::list_viewport::{ContentRows, ListViewport, RowsPerItem};
use crate::messages::settings::{RecoveryChoice, SettingsSection};
use crate::persistence::diagnostic::{CfgCode, Severity};
use crate::persistence::settings_document::PublishedSettings;
use crate::persistence::{SettingsEdit, SyntaxPath};
use crate::workbench::ScreenId;

use super::agent_types_editor::{AgentAvailability, AgentIntent, project_agent_types};
use super::keys_editor_project::{KeyIntent, project_keys};
use super::screens_editor::{CompositionStatus, ScreenIntent, project_screens};
use super::settings_types::{CaptureMode, DraftStatus, SettingsDraft, SettingsState};

/// What one detail row lets the user do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRowKind {
    /// Read-only information.
    Fact,
    /// A theme this row selects.
    Theme {
        /// The theme's identity, or `None` when the document names a slug that
        /// is not a valid theme identity at all. Such a row is still shown —
        /// a setting that vanished would be harder to correct than one that
        /// says it cannot be resolved — but it names no installed theme.
        id: Option<ThemeId>,
        /// Whether the session is wearing it.
        active: bool,
        /// Whether the manager can resolve it.
        available: bool,
    },
    /// A boolean this row toggles.
    Toggle {
        /// The leaf the toggle writes.
        path: SyntaxPath,
        /// The value the draft currently describes.
        value: bool,
    },
    /// A start screen this row selects.
    Screen {
        /// The screen's identity.
        id: ScreenId,
        /// Whether the draft currently names it.
        active: bool,
    },
    /// One diagnostic the draft carries.
    Diagnostic {
        /// The stable operator-facing code.
        code: CfgCode,
        /// How serious it is.
        severity: Severity,
    },
    /// One agent type this installation may offer.
    AgentType {
        /// The type's identity.
        type_id: AgentTypeId,
        /// Whether the candidate offers it.
        enabled: bool,
        /// What the probe found.
        availability: AgentAvailability,
    },
    /// One screen's membership, order, and layout.
    ScreenMember {
        /// The screen's identity as a configuration owner, when it spells one.
        screen_id: Option<Id>,
        /// Whether composition includes it.
        enabled: bool,
        /// Why membership is read-only, when it is.
        locked: Option<&'static str>,
        /// Whether the candidate descriptor still validates.
        composition: CompositionStatus,
    },
    /// One action's chords in one context.
    KeyBinding {
        /// The context the binding applies in.
        context: ContextId,
        /// The action the binding dispatches.
        action: ActionId,
        /// Why the binding is read-only, when it is.
        protected: Option<String>,
    },
}

/// What activating one row asks the reducer to do.
///
/// A row either writes one leaf directly or names an editor intent. Keeping
/// both in one closed answer is what lets the reducer treat "the user pressed
/// Enter on this row" as one question with one answer, whichever section the
/// row came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsActivation {
    /// Write one typed value into the draft.
    Edit(SettingsEdit),
    /// Change one agent type's enablement.
    Agent(AgentIntent),
    /// Change one screen's membership, order, or layout.
    Screen(Box<ScreenIntent>),
    /// Change one action's chords.
    Key(Box<KeyIntent>),
    /// Start capturing exactly one chord for this binding.
    CaptureChord {
        /// The context to bind in.
        context: ContextId,
        /// The action to bind.
        action: ActionId,
        /// What the captured chord does to the chords already bound.
        mode: CaptureMode,
    },
    /// Open the layout tree editor on this screen.
    OpenLayout {
        /// The screen whose layout is edited.
        screen_id: Id,
    },
}

/// One rendered row of the focused section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRow {
    /// The row's label.
    pub label: String,
    /// The row's value or detail.
    pub value: String,
    /// What the row lets the user do.
    pub kind: SettingsRowKind,
}

impl SettingsRow {
    /// What activating this row asks for, when it asks for anything.
    #[must_use]
    pub fn activation(&self) -> Option<SettingsActivation> {
        match &self.kind {
            SettingsRowKind::Theme {
                id: Some(id),
                available: true,
                ..
            } => Some(SettingsActivation::Edit(SettingsEdit::Theme(id.clone()))),
            SettingsRowKind::Toggle { path, value } => toggle_activation(path, *value),
            SettingsRowKind::Screen { id, .. } => crate::domain::Id::parse(id.as_str())
                .ok()
                .map(|id| SettingsActivation::Edit(SettingsEdit::InitialScreen(id))),
            SettingsRowKind::AgentType {
                type_id, enabled, ..
            } => Some(SettingsActivation::Agent(AgentIntent::SetEnabled {
                type_id: type_id.clone(),
                enabled: !enabled,
            })),
            // Enter on a screen opens its layout; Space is what toggles its
            // membership, so the two never compete for one keystroke.
            // A screen whose identity is not a configuration owner has no
            // syntax to write, so it offers nothing to do rather than an
            // action that would refuse itself later.
            SettingsRowKind::ScreenMember {
                screen_id: Some(screen_id),
                ..
            } => Some(SettingsActivation::OpenLayout {
                screen_id: screen_id.clone(),
            }),
            // A protected control is read-only, so Enter on it asks for
            // nothing rather than starting a capture that would be refused.
            SettingsRowKind::KeyBinding {
                context,
                action,
                protected: None,
            } => Some(SettingsActivation::CaptureChord {
                context: context.clone(),
                action: action.clone(),
                mode: CaptureMode::Replace,
            }),
            SettingsRowKind::ScreenMember { .. }
            | SettingsRowKind::KeyBinding { .. }
            | SettingsRowKind::Theme { .. }
            | SettingsRowKind::Fact
            | SettingsRowKind::Diagnostic { .. } => None,
        }
    }

    /// What toggling this row asks for, when it can be toggled.
    ///
    /// Space toggles; Enter activates. They are the same question only for a
    /// row whose whole content is a boolean.
    #[must_use]
    pub fn toggle(&self) -> Option<SettingsActivation> {
        match &self.kind {
            SettingsRowKind::Toggle { path, value } => toggle_activation(path, *value),
            SettingsRowKind::AgentType {
                type_id, enabled, ..
            } => Some(SettingsActivation::Agent(AgentIntent::SetEnabled {
                type_id: type_id.clone(),
                enabled: !enabled,
            })),
            SettingsRowKind::ScreenMember {
                screen_id: Some(screen_id),
                enabled,
                ..
            } => Some(SettingsActivation::Screen(Box::new(
                ScreenIntent::SetEnabled {
                    screen_id: screen_id.clone(),
                    enabled: !enabled,
                },
            ))),
            SettingsRowKind::ScreenMember { .. }
            | SettingsRowKind::Theme { .. }
            | SettingsRowKind::Screen { .. }
            | SettingsRowKind::KeyBinding { .. }
            | SettingsRowKind::Fact
            | SettingsRowKind::Diagnostic { .. } => None,
        }
    }

    /// What resetting this row asks for, when it can be reset.
    #[must_use]
    pub fn reset(&self) -> Option<SettingsActivation> {
        match &self.kind {
            SettingsRowKind::Theme { .. } => Some(SettingsActivation::Edit(SettingsEdit::Reset(
                SyntaxPath::Theme,
            ))),
            SettingsRowKind::Toggle { path, .. } => {
                Some(SettingsActivation::Edit(SettingsEdit::Reset(path.clone())))
            }
            SettingsRowKind::Screen { .. } => Some(SettingsActivation::Edit(SettingsEdit::Reset(
                SyntaxPath::InitialScreen,
            ))),
            SettingsRowKind::AgentType { type_id, .. } => {
                Some(SettingsActivation::Agent(AgentIntent::Reset {
                    type_id: type_id.clone(),
                }))
            }
            SettingsRowKind::ScreenMember {
                screen_id: Some(screen_id),
                ..
            } => Some(SettingsActivation::Screen(Box::new(
                ScreenIntent::ResetLayout {
                    screen_id: screen_id.clone(),
                },
            ))),
            SettingsRowKind::KeyBinding {
                context,
                action,
                protected: None,
            } => Some(SettingsActivation::Key(Box::new(KeyIntent::Reset {
                context: context.clone(),
                action: action.clone(),
            }))),
            SettingsRowKind::ScreenMember { .. }
            | SettingsRowKind::KeyBinding { .. }
            | SettingsRowKind::Fact
            | SettingsRowKind::Diagnostic { .. } => None,
        }
    }

    /// What adding one more chord to this row asks for, when it binds anything.
    ///
    /// An action may carry several chords. Capturing one more is how a binding
    /// gets its second, so the editor can express every binding the registry
    /// accepts rather than only the single-chord ones.
    #[must_use]
    pub fn add_chord(&self) -> Option<SettingsActivation> {
        match &self.kind {
            SettingsRowKind::KeyBinding {
                context,
                action,
                protected: None,
            } => Some(SettingsActivation::CaptureChord {
                context: context.clone(),
                action: action.clone(),
                mode: CaptureMode::Add,
            }),
            _ => None,
        }
    }

    /// What unbinding this row asks for, when it binds anything.
    #[must_use]
    pub fn unbind(&self) -> Option<SettingsActivation> {
        match &self.kind {
            SettingsRowKind::KeyBinding {
                context,
                action,
                protected: None,
            } => Some(SettingsActivation::Key(Box::new(KeyIntent::Unbind {
                context: context.clone(),
                action: action.clone(),
            }))),
            _ => None,
        }
    }

    /// The screen this row reorders, when it names one.
    #[must_use]
    pub const fn reorderable_screen(&self) -> Option<&Id> {
        match &self.kind {
            SettingsRowKind::ScreenMember {
                screen_id: Some(screen_id),
                ..
            } => Some(screen_id),
            _ => None,
        }
    }
}

fn toggle_activation(path: &SyntaxPath, value: bool) -> Option<SettingsActivation> {
    match path {
        SyntaxPath::OverrideAgentTheme => Some(SettingsActivation::Edit(
            SettingsEdit::OverrideAgentTheme(!value),
        )),
        SyntaxPath::AgentEnabled(agent) => {
            Some(SettingsActivation::Edit(SettingsEdit::AgentEnabled {
                agent: agent.clone(),
                enabled: !value,
            }))
        }
        // Every remaining leaf holds something other than a boolean, so a
        // toggle row can never name one.
        SyntaxPath::Theme
        | SyntaxPath::InitialScreen
        | SyntaxPath::EnabledScreens
        | SyntaxPath::ScreenOrder
        | SyntaxPath::LayoutOverride(_)
        | SyntaxPath::Keymap { .. } => None,
    }
}

/// One row of the section list, with the count its title carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionRow {
    /// The section this row names.
    pub section: SettingsSection,
    /// The section's title.
    pub title: &'static str,
    /// The count shown beside the title, when the section reports one.
    pub count: Option<usize>,
}

/// Project the section list.
#[must_use]
pub fn section_rows(state: &SettingsState) -> Vec<SectionRow> {
    let diagnostics = diagnostic_count(state);
    SettingsSection::ALL
        .into_iter()
        .map(|section| SectionRow {
            section,
            title: section.title(),
            count: section_count(state, section, diagnostics),
        })
        .collect()
}

/// The count one section's title carries, when it carries one.
///
/// A count is shown where it is the fact the user is looking for: how many
/// problems there are, and how many of each kind of thing there is to choose
/// from. General and Appearance are short fixed lists and say nothing.
fn section_count(
    state: &SettingsState,
    section: SettingsSection,
    diagnostics: usize,
) -> Option<usize> {
    match section {
        SettingsSection::Diagnostics if diagnostics > 0 => Some(diagnostics),
        SettingsSection::AgentTypes => Some(state.agent_types.len()),
        SettingsSection::Screens | SettingsSection::Keys => {
            let mut showing = state.clone();
            showing.section = section;
            Some(detail_rows(&showing).len())
        }
        SettingsSection::General | SettingsSection::Appearance | SettingsSection::Diagnostics => {
            None
        }
    }
}

/// Project the focused section's rows.
#[must_use]
pub fn detail_rows(state: &SettingsState) -> Vec<SettingsRow> {
    match state.section {
        SettingsSection::General => general_rows(state),
        SettingsSection::Appearance => appearance_rows(state),
        SettingsSection::AgentTypes => agent_type_rows(state),
        SettingsSection::Screens => screen_rows(state),
        SettingsSection::Keys => key_rows(state),
        SettingsSection::Diagnostics => diagnostic_rows(state),
    }
}

/// The slice of a section's rows that fits, with the selection kept in view.
///
/// The Keys section lists every action in every context — hundreds of rows —
/// so a pane that drew them all would put most of the list off the bottom of
/// the terminal where `j` could reach it but nothing could show it. The window
/// follows the selection, which is what makes "move down" and "see where I am"
/// the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailWindow {
    /// The rows to draw, each with its index in the whole section.
    pub rows: Vec<(usize, SettingsRow)>,
    /// How many rows sit above the window.
    pub above: usize,
    /// How many rows sit below it.
    pub below: usize,
}

/// Window one section's rows around the selection.
#[must_use]
pub fn detail_window(state: &SettingsState, content_rows: usize) -> DetailWindow {
    let rows = detail_rows(state);
    let viewport = ListViewport::uniform(
        rows.len(),
        Some(state.selected_row),
        ContentRows::new(content_rows),
        RowsPerItem::new(1),
    );
    let range = viewport.visible_range();
    let above = range.start;
    let below = rows.len().saturating_sub(range.end);
    DetailWindow {
        rows: rows
            .into_iter()
            .enumerate()
            .filter(|(index, _)| range.contains(index))
            .collect(),
        above,
        below,
    }
}

/// The typed settings the rows project from.
///
/// A draft that cannot compose a candidate still shows the document's own
/// values, so the user can see what is there while correcting whatever is
/// wrong with it.
fn published(state: &SettingsState) -> PublishedSettings {
    state
        .draft
        .as_ref()
        .map_or_else(PublishedSettings::default, |draft| {
            draft.published().clone()
        })
}

fn agent_type_rows(state: &SettingsState) -> Vec<SettingsRow> {
    project_agent_types(&state.agent_types, &published(state))
        .into_iter()
        .map(|row| SettingsRow {
            label: row.display_name,
            value: format!(
                "{} {}",
                if row.enabled { "[x]" } else { "[ ]" },
                agent_status(&row.availability)
            ),
            kind: SettingsRowKind::AgentType {
                type_id: row.type_id,
                enabled: row.enabled,
                availability: row.availability,
            },
        })
        .collect()
}

/// The one-line status an agent row shows.
///
/// The probe's own reason is shown in full rather than summarised: "not found"
/// and "missing capability: prompt" call for different actions, and shortening
/// the second to "unavailable" would hide which one this is.
fn agent_status(availability: &AgentAvailability) -> String {
    match availability {
        AgentAvailability::Compatible => "Compatible".to_owned(),
        AgentAvailability::Incompatible { reason } => format!("Incompatible: {reason}"),
        AgentAvailability::NotFound => "Not found".to_owned(),
        AgentAvailability::ProbeError { code, reason } => format!("{code}: {reason}"),
    }
}

fn screen_rows(state: &SettingsState) -> Vec<SettingsRow> {
    let Ok(registry) = crate::workbench::screen_registry() else {
        return Vec::new();
    };
    let published = published(state);
    project_screens(registry, &published)
        .into_iter()
        .map(|row| {
            let screen_id = row.owner.clone();
            SettingsRow {
                label: row.title,
                value: format!(
                    "{} {}",
                    if row.enabled { "[x]" } else { "[ ]" },
                    composition_status(&row.composition)
                ),
                kind: SettingsRowKind::ScreenMember {
                    screen_id,
                    enabled: row.enabled,
                    locked: row.enablement_locked,
                    composition: row.composition,
                },
            }
        })
        .collect()
}

fn composition_status(composition: &CompositionStatus) -> String {
    match composition {
        CompositionStatus::Valid => String::new(),
        CompositionStatus::Invalid { code, reason } => {
            format!("{code}: invalid override retained — {reason}")
        }
    }
}

fn key_rows(state: &SettingsState) -> Vec<SettingsRow> {
    let Some(snapshot) = state.actions.as_ref() else {
        return Vec::new();
    };
    let published = published(state);
    project_keys(snapshot, &published)
        .into_iter()
        .map(|row| SettingsRow {
            label: format!("{} {}", row.context.as_str(), row.label),
            value: key_value(&row),
            kind: SettingsRowKind::KeyBinding {
                context: row.context,
                action: row.action,
                protected: row.protected,
            },
        })
        .collect()
}

fn key_value(row: &crate::state::keys_editor_project::KeyEditorRow) -> String {
    let chords = if row.chords.is_empty() {
        "unbound".to_owned()
    } else {
        row.chords
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    };
    match row.protected.as_deref() {
        Some(reason) => format!("{chords} — protected: {reason}"),
        None => chords,
    }
}

fn general_rows(state: &SettingsState) -> Vec<SettingsRow> {
    let mut rows = Vec::new();
    if let Some(environment) = state.environment.as_ref() {
        rows.push(fact(
            "Settings",
            environment.settings_path.to_string_lossy(),
        ));
        rows.push(fact("State", environment.state_path.to_string_lossy()));
        rows.push(fact("Platform", environment.platform));
        rows.push(fact(
            "Isolated",
            if environment.isolated { "yes" } else { "no" },
        ));
    }
    let selected = state
        .draft
        .as_ref()
        .and_then(|draft| draft.published().workbench.initial_screen.clone());
    rows.push(fact(
        "Start screen",
        selected
            .as_ref()
            .map_or_else(|| ScreenId::default().as_str().to_owned(), Id::to_string),
    ));
    for screen in ScreenId::ALL {
        let active = selected
            .as_ref()
            .is_some_and(|id| id.as_str() == screen.as_str());
        rows.push(SettingsRow {
            label: screen.as_str().to_owned(),
            value: if active { "start screen" } else { "" }.to_owned(),
            kind: SettingsRowKind::Screen { id: screen, active },
        });
    }
    rows
}

fn appearance_rows(state: &SettingsState) -> Vec<SettingsRow> {
    let mut rows = Vec::new();
    let drafted = state
        .draft
        .as_ref()
        .and_then(|draft| draft.published().appearance.theme.clone());
    // The active marker means "the theme this session is wearing", which is what
    // the retired picker marked. A document naming a theme nobody can resolve
    // does not make the session wear it, so that row is listed as unavailable
    // rather than marked active.
    let worn = state.desired_theme();
    for choice in &state.themes {
        let active = worn == Some(&choice.id);
        rows.push(SettingsRow {
            label: choice.name.clone(),
            value: choice.id.to_string(),
            kind: SettingsRowKind::Theme {
                id: Some(choice.id.clone()),
                active,
                available: true,
            },
        });
    }
    if let Some(missing) = missing_theme(state, drafted.as_ref()) {
        // The slug is shown as written, even when it is not a valid theme
        // identity: a setting that vanished from the list would be harder to
        // correct than one that says it cannot be resolved. It carries no
        // identity, because it names no theme that exists.
        rows.push(SettingsRow {
            label: missing.to_owned(),
            value: "unavailable: not installed".to_owned(),
            kind: SettingsRowKind::Theme {
                id: ThemeId::parse(missing).ok(),
                active: false,
                available: false,
            },
        });
    }
    let override_agent_theme = state
        .draft
        .as_ref()
        .and_then(|draft| draft.published().appearance.override_agent_theme)
        .unwrap_or(false);
    rows.push(SettingsRow {
        label: "Apply theme to agent".to_owned(),
        value: if override_agent_theme { "[x]" } else { "[ ]" }.to_owned(),
        kind: SettingsRowKind::Toggle {
            path: SyntaxPath::OverrideAgentTheme,
            value: override_agent_theme,
        },
    });
    rows
}

/// The theme the document names but the manager cannot resolve, if any.
fn missing_theme<'a>(state: &SettingsState, drafted: Option<&'a String>) -> Option<&'a str> {
    let slug = drafted?;
    if state.themes.iter().any(|choice| choice.id.as_str() == slug) {
        return None;
    }
    Some(slug.as_str())
}

fn diagnostic_rows(state: &SettingsState) -> Vec<SettingsRow> {
    diagnostics(state)
        .iter()
        .map(|diagnostic| SettingsRow {
            label: format!("{} {}", diagnostic.code.as_str(), diagnostic.path.as_str()),
            value: format!("{} — {}", diagnostic.redacted_detail, diagnostic.correction),
            kind: SettingsRowKind::Diagnostic {
                code: diagnostic.code,
                severity: diagnostic.severity,
            },
        })
        .collect()
}

/// The sorted diagnostics the screen reports, from the draft or from what
/// stopped one being bound at all.
#[must_use]
pub fn diagnostics(state: &SettingsState) -> &[crate::persistence::diagnostic::Diagnostic] {
    state
        .draft
        .as_ref()
        .map_or(state.blocked.as_slice(), SettingsDraft::validation)
}

fn diagnostic_count(state: &SettingsState) -> usize {
    diagnostics(state).len()
}

/// The row index of the first error, which validation failure focuses.
#[must_use]
pub fn first_error_row(state: &SettingsState) -> Option<usize> {
    diagnostics(state)
        .iter()
        .position(|diagnostic| diagnostic.severity == Severity::Error)
}

/// The recovery choices this state offers, in display order.
///
/// A conflict means the file on disk is fine and newer, so the choices are
/// about which copy to keep. A write failure means nothing was written, so the
/// choices are about trying again.
#[must_use]
pub fn recovery_choices(state: &SettingsState) -> Vec<RecoveryChoice> {
    let Some(draft) = state.draft.as_ref() else {
        return Vec::new();
    };
    match draft.status() {
        DraftStatus::Conflict { .. } => vec![
            RecoveryChoice::Reload,
            RecoveryChoice::Export,
            RecoveryChoice::Retry,
        ],
        DraftStatus::Failed { .. } => vec![
            RecoveryChoice::Retry,
            RecoveryChoice::Export,
            RecoveryChoice::Discard,
        ],
        DraftStatus::Clean | DraftStatus::Dirty | DraftStatus::Saving { .. } => Vec::new(),
    }
}

fn fact(label: &str, value: impl AsRef<str>) -> SettingsRow {
    SettingsRow {
        label: label.to_owned(),
        value: value.as_ref().to_owned(),
        kind: SettingsRowKind::Fact,
    }
}
