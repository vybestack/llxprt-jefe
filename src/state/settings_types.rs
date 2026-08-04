//! The Settings shell's draft and screen state (issue #387, CW-07).
//!
//! A draft is bound to the exact bytes it was taken from, not to a copy of the
//! values in them. That is what lets a save refuse to overwrite a file somebody
//! else changed, and what lets a conflict keep both the disk and the draft
//! instead of picking a winner.
//!
//! Nothing here is persisted. A draft, its preview, and the screen's selection
//! all belong to the session that is looking at them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::sha256::Sha256;
use crate::messages::settings::{SettingsEnvironment, SettingsSection, ThemeChoice};
use crate::persistence::diagnostic::{CfgCode, Diagnostic};
use crate::persistence::migration::SettingsMigration;
use crate::persistence::settings_document::PublishedSettings;
use crate::persistence::writer::ExpectedHash;
use crate::persistence::{SettingsCandidate, SettingsEdit, SyntaxPath};
use crate::theme::ThemePreviewToken;

use super::navigation_dirty::{DirtyChoice, DraftToken};

/// How far a draft has got.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DraftStatus {
    /// The draft holds nothing the base does not.
    #[default]
    Clean,
    /// The draft holds unsaved work.
    Dirty,
    /// This revision is being made authoritative.
    Saving {
        /// The revision in flight.
        revision: u64,
    },
    /// The target changed since the draft was bound to it.
    Conflict {
        /// The digest of the bytes now on disk, when they could be read.
        disk_hash: Option<Sha256>,
    },
    /// The durable write failed and the target is unchanged.
    Failed {
        /// The typed reason.
        code: CfgCode,
    },
}

impl DraftStatus {
    /// Whether the draft is waiting on a durable write.
    #[must_use]
    pub const fn is_saving(&self) -> bool {
        matches!(self, Self::Saving { .. })
    }

    /// Whether the draft needs the user to choose a recovery.
    #[must_use]
    pub const fn needs_recovery(&self) -> bool {
        matches!(self, Self::Conflict { .. } | Self::Failed { .. })
    }
}

/// The complete document a draft would save, or what blocks it.
///
/// A blocked draft is not an empty one: the edits are still there and the base
/// is still there, so the user can correct the document and try again without
/// losing the work that was already typed.
#[derive(Debug, Clone)]
pub enum DraftCandidate {
    /// The candidate validates and can be saved.
    Valid(Box<SettingsCandidate>),
    /// These sorted diagnostics stop the candidate from existing.
    Blocked(Vec<Diagnostic>),
}

impl DraftCandidate {
    /// The candidate a save would write, when there is one.
    #[must_use]
    pub fn valid(&self) -> Option<&SettingsCandidate> {
        match self {
            Self::Valid(candidate) => Some(candidate),
            Self::Blocked(_) => None,
        }
    }

    /// The sorted diagnostics blocking this candidate, if any.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Valid(_) => &[],
            Self::Blocked(diagnostics) => diagnostics,
        }
    }
}

/// One unsaved edit set over an exactly identified settings document.
#[derive(Debug, Clone)]
pub struct SettingsDraft {
    token: DraftToken,
    generation: u64,
    base: Arc<SettingsMigration>,
    base_expected: ExpectedHash,
    base_revision: u64,
    edits: BTreeMap<SyntaxPath, SettingsEdit>,
    candidate: DraftCandidate,
    preview: Option<ThemePreviewToken>,
    status: DraftStatus,
    pending_revision: Option<u64>,
    exit_after_save: bool,
}

impl SettingsDraft {
    /// Bind a fresh draft to one loaded base.
    ///
    /// The draft's identity doubles as its preview generation. Draft tokens are
    /// allocated once and never reused, so a preview issued for one draft can
    /// never be mistaken for a preview of the draft that replaced it.
    #[must_use]
    pub fn bound(
        base: Arc<SettingsMigration>,
        base_expected: ExpectedHash,
        base_revision: u64,
        candidate: DraftCandidate,
    ) -> Self {
        let token = DraftToken::next();
        Self {
            token,
            generation: token.get(),
            base,
            base_expected,
            base_revision,
            edits: BTreeMap::new(),
            candidate,
            preview: None,
            status: DraftStatus::Clean,
            pending_revision: None,
            exit_after_save: false,
        }
    }

    /// This draft's identity, which the navigation dirty guard holds.
    #[must_use]
    pub const fn token(&self) -> DraftToken {
        self.token
    }

    /// The generation preview tokens issued for this draft carry.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The digest of the exact bytes this draft was bound to.
    ///
    /// `None` means the settings file did not exist when the draft was bound,
    /// which a save turns into the file's creation.
    #[must_use]
    pub const fn base_hash(&self) -> Option<Sha256> {
        match self.base_expected {
            ExpectedHash::Absent => None,
            ExpectedHash::Present(hash) => Some(hash),
        }
    }

    /// The freshness expectation a save carries.
    #[must_use]
    pub const fn base_expected(&self) -> ExpectedHash {
        self.base_expected
    }

    /// The document revision this draft was bound at.
    #[must_use]
    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }

    /// Borrow the lossless base, which no edit changes.
    #[must_use]
    pub fn base(&self) -> &SettingsMigration {
        &self.base
    }

    /// The exact leaves this draft holds edits for.
    pub fn edited_paths(&self) -> impl Iterator<Item = &SyntaxPath> + '_ {
        self.edits.keys()
    }

    /// The edit held for one leaf, if there is one.
    #[must_use]
    pub fn edit(&self, path: &SyntaxPath) -> Option<&SettingsEdit> {
        self.edits.get(path)
    }

    /// The complete candidate this draft would save, or what blocks it.
    #[must_use]
    pub const fn candidate(&self) -> &DraftCandidate {
        &self.candidate
    }

    /// The complete candidate this draft would save, when it has one.
    #[must_use]
    pub fn candidate_bytes(&self) -> Option<SettingsCandidate> {
        self.candidate.valid().cloned()
    }

    /// The typed settings this draft currently describes.
    ///
    /// A blocked candidate falls back to the base's published values, so the
    /// screen keeps showing what the document actually holds while the user
    /// corrects whatever is wrong with it.
    #[must_use]
    pub fn published(&self) -> &PublishedSettings {
        self.candidate
            .valid()
            .map_or_else(|| self.base.published(), SettingsCandidate::published)
    }

    /// The sorted diagnostics that block this draft, if any.
    #[must_use]
    pub fn validation(&self) -> &[Diagnostic] {
        self.candidate.diagnostics()
    }

    /// The theme preview this draft is showing, if any.
    #[must_use]
    pub const fn preview(&self) -> Option<&ThemePreviewToken> {
        self.preview.as_ref()
    }

    /// Replace the theme preview this draft is showing.
    pub fn set_preview(&mut self, preview: Option<ThemePreviewToken>) {
        self.preview = preview;
    }

    /// How far this draft has got.
    #[must_use]
    pub const fn status(&self) -> &DraftStatus {
        &self.status
    }

    /// Move this draft to a new status.
    pub fn set_status(&mut self, status: DraftStatus) {
        self.status = status;
    }

    /// Whether this draft holds unsaved work.
    ///
    /// An edit is only held while it still changes something: rebuilding the
    /// candidate forgets every edit once the bytes match the base again, so
    /// after the revalidation the reducer performs on each edit, "holds edits"
    /// and "would change the file" are the same statement.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.edits.is_empty()
    }

    /// Whether this draft can be made authoritative right now.
    #[must_use]
    pub fn is_saveable(&self) -> bool {
        self.is_dirty() && self.candidate.valid().is_some() && !self.status.is_saving()
    }

    /// Whether a completed save should also leave the screen.
    #[must_use]
    pub const fn exits_after_save(&self) -> bool {
        self.exit_after_save
    }

    /// Record that a completed save should also leave the screen.
    pub const fn exit_after_save(&mut self) {
        self.exit_after_save = true;
    }

    /// The newest revision this draft has scheduled a save for.
    #[must_use]
    pub const fn pending_revision(&self) -> Option<u64> {
        self.pending_revision
    }

    /// Schedule the next save revision, which is always the newest.
    pub const fn schedule(&mut self, revision: u64) {
        self.pending_revision = Some(revision);
    }

    /// Whether a completion for `revision` answers the newest scheduled save.
    #[must_use]
    pub fn answers_pending(&self, revision: u64) -> bool {
        self.pending_revision == Some(revision)
    }

    /// Clear the scheduled save once it has been answered.
    pub const fn clear_pending(&mut self) {
        self.pending_revision = None;
    }

    /// Record one typed edit for one leaf, replacing any edit already held for
    /// that leaf.
    ///
    /// Whether the accumulated edits still differ from the base is decided by
    /// rebuilding the candidate, so a draft is only known to be clean again
    /// after the caller revalidates it.
    pub fn record(&mut self, edit: SettingsEdit) {
        self.edits.insert(edit.path(), edit);
    }

    /// Forget every edit, returning the draft to its base.
    pub fn forget_edits(&mut self) {
        self.edits.clear();
    }

    /// Replace the validated candidate after the edits changed.
    pub fn set_candidate(&mut self, candidate: DraftCandidate) {
        self.candidate = candidate;
    }

    /// Adopt a completed save as the new base.
    pub fn adopt(&mut self, base: Arc<SettingsMigration>, hash: Sha256, revision: u64) {
        self.base = base;
        self.base_expected = ExpectedHash::Present(hash);
        self.base_revision = revision;
        self.edits.clear();
        self.pending_revision = None;
        self.status = DraftStatus::Clean;
    }
}

/// The binding one waiting chord capture will bind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordCapture {
    /// The context to bind in.
    pub context: crate::domain::input_context::ContextId,
    /// The action to bind.
    pub action: crate::domain::action_registry::ActionId,
}

/// Where the Settings screen's keyboard focus is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsFocus {
    /// The list of sections.
    #[default]
    Sections,
    /// The focused section's rows.
    Detail,
}

/// The Settings screen's runtime state.
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    /// Whether the screen is open.
    pub active: bool,
    /// Which section the detail pane shows.
    pub section: SettingsSection,
    /// Where the keyboard focus is.
    pub focus: SettingsFocus,
    /// The selected row of the focused pane.
    pub selected_row: usize,
    /// The live draft, when one could be bound.
    pub draft: Option<SettingsDraft>,
    /// The diagnostics that stop a draft from being bound at all.
    pub blocked: Vec<Diagnostic>,
    /// Every theme the manager can resolve, in list order.
    pub themes: Vec<ThemeChoice>,
    /// The theme the session was wearing when the screen opened.
    ///
    /// This is what "no preview" means: with no token in flight the session
    /// wears this, so cancel, discard, reload, and a failed save all restore it
    /// by clearing the token rather than by four separate undo paths. A
    /// successful save moves it to the theme that was saved.
    pub opened_theme: Option<crate::domain::ThemeId>,
    /// The theme a closed screen left for the boundary to restore.
    ///
    /// A preview that was never saved must not outlive the screen showing it,
    /// and the screen is gone by the time the boundary reconciles, so what to
    /// go back to is left here rather than forgotten with the draft.
    pub restore_theme: Option<crate::domain::ThemeId>,
    /// The identity the host dirty guard is waiting on for this screen's save.
    pub guard_correlation: Option<crate::domain::effects::Correlation>,
    /// The facts the read-only rows report.
    pub environment: Option<SettingsEnvironment>,
    /// The agent-type probe snapshot the Agent Types rows project from.
    ///
    /// Bound once when the screen opens and never changed while it is open. An
    /// editor reads a snapshot of what the session found; it does not probe,
    /// and a probe completing underneath it must not make the list move while
    /// the user is choosing from it.
    pub agent_types: Vec<crate::agent_status_view::AgentAvailabilityObservation>,
    /// The action registry snapshot the Keys rows project from.
    pub actions: Option<crate::domain::action_registry::ActionRegistrySnapshot>,
    /// The binding a chord capture is waiting for, while one is waiting.
    ///
    /// A capture takes exactly the next chord, so what it is for has to be
    /// remembered across exactly one keystroke and no longer.
    pub capture: Option<ChordCapture>,
    /// The layout tree editor, while it is open.
    pub layout_editor: Option<super::layout_editor::LayoutEditorState>,
    /// The selected recovery choice, when a recovery is offered.
    pub recovery_row: usize,
    /// The newest revision this session has scheduled a save for.
    ///
    /// Revisions are per session and strictly increasing, so a completion can
    /// be recognised as answering superseded work by its number alone. This
    /// counts scheduled saves rather than successful ones, so a retry after a
    /// conflict is a new attempt rather than a repeat of the one that failed.
    pub last_scheduled_revision: u64,
    /// A redacted notice about the last completed action.
    pub notice: Option<String>,
    /// Whether a reload is waiting for explicit confirmation.
    pub reload_confirm: bool,
    /// The choice the host dirty guard's modal has focus on.
    ///
    /// The guard itself deliberately holds no cursor: it is a question, not a
    /// widget. Settings is the one screen that can raise it today, so the
    /// cursor lives with the draft that raised it.
    pub dirty_choice: DirtyChoiceCursor,
}

/// Which of Save, Discard, and Cancel the dirty modal has focus on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirtyChoiceCursor {
    /// Run the owner's save, and leave only if it succeeds.
    #[default]
    Save,
    /// Abandon the draft and leave.
    Discard,
    /// Stay, keeping the draft.
    Cancel,
}

impl DirtyChoiceCursor {
    /// Every choice, in display order.
    pub const ALL: [Self; 3] = [Self::Save, Self::Discard, Self::Cancel];

    /// The choice's label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Save => "Save",
            Self::Discard => "Discard",
            Self::Cancel => "Cancel",
        }
    }

    /// The guard answer this cursor stands for.
    #[must_use]
    pub const fn choice(self) -> DirtyChoice {
        match self {
            Self::Save => DirtyChoice::Save,
            Self::Discard => DirtyChoice::Discard,
            Self::Cancel => DirtyChoice::Cancel,
        }
    }

    /// The next choice, wrapping.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Save => Self::Discard,
            Self::Discard => Self::Cancel,
            Self::Cancel => Self::Save,
        }
    }

    /// The previous choice, wrapping.
    #[must_use]
    pub const fn previous(self) -> Self {
        match self {
            Self::Save => Self::Cancel,
            Self::Discard => Self::Save,
            Self::Cancel => Self::Discard,
        }
    }
}

impl SettingsState {
    /// The theme the session should be wearing for this state.
    ///
    /// The boundary makes the theme manager match this after every settings
    /// message, which is the whole of "apply, adopt, and revert a preview": a
    /// token in flight names the theme to show, and no token means the theme
    /// the screen opened on.
    #[must_use]
    pub fn desired_theme(&self) -> Option<&crate::domain::ThemeId> {
        if !self.active {
            return self.restore_theme.as_ref();
        }
        self.draft
            .as_ref()
            .and_then(SettingsDraft::preview)
            .map(crate::theme::ThemePreviewToken::preview_theme)
            .or(self.opened_theme.as_ref())
    }

    /// Whether the screen currently holds unsaved work.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(SettingsDraft::is_dirty)
    }
}
