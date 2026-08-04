//! Settings-shell messages (issue #387, CW-07).
//!
//! Every one of these is an intent the presenter emits or a completion the
//! boundary reports. None of them carries a value the reducer would have to
//! parse: an edit already names the leaf it writes and holds that leaf's type,
//! and a completion already says which revision it answers for.

use std::path::PathBuf;

use crate::domain::ThemeId;
use crate::persistence::diagnostic::Diagnostic;
use crate::persistence::{SettingsEdit, SettingsSaveOutcome, SyntaxPath};
use crate::state::agent_types_editor::AgentIntent;
use crate::state::navigation_dirty::DirtyChoice;
use crate::state::screens_editor::ScreenIntent;

use super::NavDir;

/// One theme the manager can resolve, as the Appearance list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeChoice {
    /// The theme's stable identity, which is what settings store.
    pub id: ThemeId,
    /// The theme's display name.
    pub name: String,
}

/// The facts the General and Diagnostics sections report but never change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvironment {
    /// The settings document this session reads and writes.
    pub settings_path: PathBuf,
    /// The durable state document this session reads and writes.
    pub state_path: PathBuf,
    /// The platform whose standard locations were resolved.
    pub platform: &'static str,
    /// Whether `--config` isolated this session from the default locations.
    pub isolated: bool,
}

/// Everything the boundary knows that a draft has to be bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSource {
    /// The exact bytes read from the settings target, or `None` when absent.
    pub bytes: Option<Vec<u8>>,
    /// The document revision these bytes were read at.
    pub revision: u64,
    /// The theme the session is wearing right now.
    pub active_theme: ThemeId,
    /// Every theme the manager can resolve, in list order.
    pub themes: Vec<ThemeChoice>,
    /// The facts the read-only rows report.
    pub environment: SettingsEnvironment,
}

/// Which section of the Settings screen the detail pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SettingsSection {
    /// Paths, platform, and the start screen.
    #[default]
    General,
    /// Theme selection and the agent-theme override.
    Appearance,
    /// Read-only provenance and validation reporting.
    Diagnostics,
}

impl SettingsSection {
    /// Every section, in display order.
    pub const ALL: [Self; 3] = [Self::General, Self::Appearance, Self::Diagnostics];

    /// The section's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Diagnostics => "Diagnostics",
        }
    }
}

/// What the user chose in the conflict or failure recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryChoice {
    /// Rebuild the draft from the exact bytes now on disk.
    Reload,
    /// Write the draft somewhere else, keeping it.
    Export,
    /// Revalidate and try the same save again.
    Retry,
    /// Abandon the draft and return to its base.
    Discard,
}

/// Settings-shell messages.
#[derive(Debug, Clone)]
pub enum SettingsMessage {
    /// Bind a fresh draft to these bytes and open the screen.
    Open(Box<SettingsSource>),
    /// Open the screen on the reason it could not read the settings target.
    OpenFailed(Box<Diagnostic>),
    /// Leave the screen, releasing the draft.
    Close,
    /// Show this section's detail.
    SelectSection(SettingsSection),
    /// Move between the section list and the detail pane.
    CycleFocus,
    /// Move between the detail pane and the section list.
    CycleFocusReverse,
    /// Move the selection inside the focused pane.
    Navigate(NavDir),
    /// Apply the selected row's edit, if it has one.
    Activate,
    /// Write one typed value into the draft.
    Edit(SettingsEdit),
    /// Remove one leaf's assignment so the compiled default is inherited.
    Reset(SyntaxPath),
    /// Draft one change to an agent type's enablement.
    Agent(AgentIntent),
    /// Draft one change to a screen's membership, order, or layout.
    Screen(Box<ScreenIntent>),
    /// Make the draft authoritative.
    Save,
    /// Make the draft authoritative and then leave the screen.
    SaveAndExit,
    /// Abandon the draft and return to its base.
    Discard,
    /// Leave the screen, letting the host dirty guard hold it back when the
    /// draft has unsaved work.
    Back,
    /// Answer the host dirty guard that Back raised.
    ResolveDirty(DirtyChoice),
    /// Ask to rebuild the draft from disk, raising a confirmation when dirty.
    Reload,
    /// Withdraw a reload that has not been confirmed.
    ReloadCancelled,
    /// Rebind the draft to these freshly read bytes.
    Reloaded(Box<SettingsSource>),
    /// Move the recovery selection.
    NavigateRecovery(NavDir),
    /// Move the dirty guard's Save/Discard/Cancel focus.
    NavigateDirty(NavDir),
    /// Report what a durable save did.
    SaveCompleted(Box<SettingsSaveOutcome>),
    /// Report where a draft was exported, or why it was not.
    ExportCompleted(Box<Result<PathBuf, Diagnostic>>),
}

impl SettingsMessage {
    /// The stable channel name used for routing, tracing, and policy tests.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Open(_) => "OpenSettings",
            Self::OpenFailed(_) => "OpenSettingsFailed",
            Self::Close => "CloseSettings",
            Self::SelectSection(_) => "SettingsSelectSection",
            Self::CycleFocus => "SettingsCycleFocus",
            Self::CycleFocusReverse => "SettingsCycleFocusReverse",
            Self::Navigate(_) => "SettingsNavigate",
            Self::Activate => "SettingsActivate",
            Self::Edit(_) => "SettingsEdit",
            Self::Reset(_) => "SettingsReset",
            Self::Agent(_) => "SettingsAgentIntent",
            Self::Screen(_) => "SettingsScreenIntent",
            Self::Save => "SettingsSave",
            Self::SaveAndExit => "SettingsSaveAndExit",
            Self::Discard => "SettingsDiscard",
            Self::Back => "SettingsBack",
            Self::ResolveDirty(_) => "SettingsResolveDirty",
            Self::Reload => "SettingsReload",
            Self::ReloadCancelled => "SettingsReloadCancelled",
            Self::Reloaded(_) => "SettingsReloaded",
            Self::NavigateRecovery(_) => "SettingsNavigateRecovery",
            Self::NavigateDirty(_) => "SettingsNavigateDirty",
            Self::SaveCompleted(_) => "SettingsSaveCompleted",
            Self::ExportCompleted(_) => "SettingsExportCompleted",
        }
    }
}
