//! Pure projection of the Settings screen into renderable rows.
//!
//! This is the iocraft-free view model. It is also the reducer's authority for
//! how many rows a section has, so the selection cannot point at a row the
//! renderer does not draw — one list, one answer, no drift.
//!
//! The Appearance theme rows are the theme picker's projection moved here: same
//! slug, name, selection and active marker, plus the availability the picker
//! could not express because it only ever listed installed themes.

use crate::domain::{Id, ThemeId};
use crate::messages::settings::{RecoveryChoice, SettingsSection};
use crate::persistence::diagnostic::{CfgCode, Severity};
use crate::persistence::{SettingsEdit, SyntaxPath};
use crate::workbench::ScreenId;

use super::settings_types::{DraftStatus, SettingsDraft, SettingsState};

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
    /// The edit activating this row makes, when it makes one.
    #[must_use]
    pub fn activation(&self) -> Option<SettingsEdit> {
        match &self.kind {
            SettingsRowKind::Theme {
                id: Some(id),
                available: true,
                ..
            } => Some(SettingsEdit::Theme(id.clone())),
            SettingsRowKind::Toggle { path, value } => match path {
                SyntaxPath::OverrideAgentTheme => Some(SettingsEdit::OverrideAgentTheme(!value)),
                SyntaxPath::AgentEnabled(agent) => Some(SettingsEdit::AgentEnabled {
                    agent: agent.clone(),
                    enabled: !value,
                }),
                // Every remaining leaf holds something other than a boolean, so
                // a toggle row can never name one.
                SyntaxPath::Theme
                | SyntaxPath::InitialScreen
                | SyntaxPath::EnabledScreens
                | SyntaxPath::ScreenOrder
                | SyntaxPath::LayoutOverride(_)
                | SyntaxPath::Keymap { .. } => None,
            },
            SettingsRowKind::Screen { id, .. } => crate::domain::Id::parse(id.as_str())
                .ok()
                .map(SettingsEdit::InitialScreen),
            SettingsRowKind::Theme { .. }
            | SettingsRowKind::Fact
            | SettingsRowKind::Diagnostic { .. } => None,
        }
    }

    /// The leaf this row writes, which Reset returns to its compiled default.
    #[must_use]
    pub fn editable_path(&self) -> Option<SyntaxPath> {
        match &self.kind {
            SettingsRowKind::Theme { .. } => Some(SyntaxPath::Theme),
            SettingsRowKind::Toggle { path, .. } => Some(path.clone()),
            SettingsRowKind::Screen { .. } => Some(SyntaxPath::InitialScreen),
            SettingsRowKind::Fact | SettingsRowKind::Diagnostic { .. } => None,
        }
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
            count: match section {
                SettingsSection::Diagnostics if diagnostics > 0 => Some(diagnostics),
                SettingsSection::General
                | SettingsSection::Appearance
                | SettingsSection::Diagnostics => None,
            },
        })
        .collect()
}

/// Project the focused section's rows.
#[must_use]
pub fn detail_rows(state: &SettingsState) -> Vec<SettingsRow> {
    match state.section {
        SettingsSection::General => general_rows(state),
        SettingsSection::Appearance => appearance_rows(state),
        SettingsSection::Diagnostics => diagnostic_rows(state),
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
