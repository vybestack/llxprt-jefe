//! The Settings shell's draft, save, reload, and export authority (issue #387).
//!
//! Everything the Settings screen knows about its own draft is decided here, in
//! one pure reducer. The boundary reads bytes, writes bytes, and shows a theme;
//! it never decides whether a save may happen, which revision answers which
//! attempt, or what a conflict leaves behind.
//!
//! Two rules do most of the work:
//!
//! - a draft is bound to the exact bytes it was taken from, so a save can
//!   refuse to overwrite a file somebody else changed rather than discovering
//!   the change afterwards; and
//! - only the newest scheduled revision is answerable, so a completion that
//!   arrives after the user has saved again is a fact about work that has been
//!   superseded, not an instruction.

use std::sync::Arc;

use crate::config_owners::builtin_owner_catalog;
use crate::domain::ThemeId;
use crate::domain::effects::{EffectFamily, SemanticKey};
use crate::messages::NavDir;
use crate::messages::settings::{SettingsMessage, SettingsSection, SettingsSource};
use crate::persistence::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, Severity};
use crate::persistence::migration::SettingsMigration;
use crate::persistence::settings_edit::load_settings_base;
use crate::persistence::writer::ExpectedHash;
use crate::persistence::{SettingsCandidate, SettingsEdit, SettingsSaveOutcome, SyntaxPath};
use crate::theme::ThemePreviewToken;
use crate::workbench::ScreenId;

use super::navigation_dirty::{DirtyChoice, DraftAction, SaveIntent};
use super::settings_types::{DraftCandidate, DraftStatus, SettingsDraft, SettingsFocus};
use super::{AppState, settings_view};

/// The owner identity the navigation dirty guard waits on for a settings save.
pub const SETTINGS_OWNER: &str = "core.settings";

/// The operation a settings save registers, and the key its completion carries.
#[must_use]
pub fn settings_save_key() -> SemanticKey {
    SemanticKey::new(EffectFamily::Persistence, "settings-save")
}

/// The notice a structural save shows, verbatim.
pub const RESTART_NOTICE: &str = "Restart Jefe to apply structural changes";

impl AppState {
    /// Apply one settings intent or completion.
    pub(super) fn reduce_settings(&mut self, message: SettingsMessage) -> bool {
        match message {
            SettingsMessage::Open(source) => self.open_settings(*source),
            SettingsMessage::Close => self.close_settings(),
            SettingsMessage::SelectSection(section) => self.select_settings_section(section),
            SettingsMessage::CycleFocus => self.cycle_settings_focus(true),
            SettingsMessage::CycleFocusReverse => self.cycle_settings_focus(false),
            SettingsMessage::Navigate(direction) => self.navigate_settings(direction),
            SettingsMessage::Activate => self.activate_settings_row(),
            SettingsMessage::Edit(edit) => self.edit_settings(edit),
            SettingsMessage::Reset(path) => self.edit_settings(SettingsEdit::Reset(path)),
            SettingsMessage::Save => self.save_settings(false),
            SettingsMessage::SaveAndExit => self.save_settings(true),
            SettingsMessage::Discard => self.discard_settings(),
            SettingsMessage::Back => self.leave_settings(),
            SettingsMessage::ResolveDirty(choice) => self.resolve_settings_dirty(choice),
            SettingsMessage::Reload => self.request_settings_reload(),
            SettingsMessage::ReloadCancelled => self.cancel_settings_reload(),
            SettingsMessage::Reloaded(source) => self.rebind_settings(*source),
            SettingsMessage::NavigateRecovery(direction) => self.navigate_recovery(direction),
            SettingsMessage::NavigateDirty(direction) => self.navigate_dirty(direction),
            SettingsMessage::SaveCompleted(outcome) => self.complete_settings_save(*outcome),
            SettingsMessage::ExportCompleted(result) => self.complete_settings_export(*result),
        }
    }

    /// Bind a fresh draft to the exact loaded bytes and open the screen.
    fn open_settings(&mut self, source: SettingsSource) -> bool {
        self.settings_state.active = true;
        self.settings_state.section = SettingsSection::General;
        self.settings_state.focus = SettingsFocus::Sections;
        self.settings_state.selected_row = 0;
        self.settings_state.recovery_row = 0;
        self.settings_state.reload_confirm = false;
        self.settings_state.notice = None;
        self.bind_settings_source(source);
        let _ = self.enter_screen(ScreenId::Settings);
        true
    }

    /// Rebind the draft to freshly read bytes, keeping the screen where it is.
    fn rebind_settings(&mut self, source: SettingsSource) -> bool {
        self.settings_state.reload_confirm = false;
        self.bind_settings_source(source);
        self.settings_state.notice = Some("Reloaded from disk".to_owned());
        let _ = self.mark_screen_clean();
        true
    }

    fn bind_settings_source(&mut self, source: SettingsSource) {
        self.settings_state.themes = source.themes;
        self.settings_state.environment = Some(source.environment);
        self.settings_state.opened_theme = Some(source.active_theme);
        let expected = source
            .bytes
            .as_deref()
            .map_or(ExpectedHash::Absent, |bytes| {
                ExpectedHash::Present(crate::domain::sha256::Sha256::digest(bytes))
            });
        match load_base(source.bytes.as_deref()) {
            Ok(base) => {
                let base = Arc::new(base);
                let candidate = build_candidate(&base, &[], expected);
                self.settings_state.blocked.clear();
                self.settings_state.draft = Some(SettingsDraft::bound(
                    base,
                    expected,
                    source.revision,
                    candidate,
                ));
            }
            Err(diagnostics) => {
                self.settings_state.blocked = diagnostics;
                self.settings_state.draft = None;
            }
        }
        self.clamp_settings_selection();
    }

    /// Release the draft and its preview when the screen closes.
    fn close_settings(&mut self) -> bool {
        if !self.settings_state.active {
            return false;
        }
        self.settings_state.active = false;
        self.settings_state.draft = None;
        self.settings_state.blocked.clear();
        self.settings_state.opened_theme = None;
        self.settings_state.reload_confirm = false;
        self.settings_state.notice = None;
        let _ = self.mark_screen_clean();
        true
    }

    fn select_settings_section(&mut self, section: SettingsSection) -> bool {
        self.settings_state.section = section;
        self.settings_state.selected_row = 0;
        true
    }

    fn cycle_settings_focus(&mut self, forward: bool) -> bool {
        self.settings_state.focus = match (self.settings_state.focus, forward) {
            (SettingsFocus::Sections, _) => SettingsFocus::Detail,
            (SettingsFocus::Detail, _) => SettingsFocus::Sections,
        };
        self.settings_state.selected_row = 0;
        self.clamp_settings_selection();
        true
    }

    fn navigate_settings(&mut self, direction: NavDir) -> bool {
        let count = match self.settings_state.focus {
            SettingsFocus::Sections => SettingsSection::ALL.len(),
            SettingsFocus::Detail => settings_view::detail_rows(&self.settings_state).len(),
        };
        let current = self.settings_state.selected_row;
        self.settings_state.selected_row = step(current, count, direction);
        if self.settings_state.focus == SettingsFocus::Sections {
            self.settings_state.section = SettingsSection::ALL
                .get(self.settings_state.selected_row)
                .copied()
                .unwrap_or_default();
        }
        true
    }

    fn navigate_recovery(&mut self, direction: NavDir) -> bool {
        let count = settings_view::recovery_choices(&self.settings_state).len();
        self.settings_state.recovery_row = step(self.settings_state.recovery_row, count, direction);
        true
    }

    /// Move the dirty guard's Save/Discard/Cancel focus, which wraps.
    fn navigate_dirty(&mut self, direction: NavDir) -> bool {
        let cursor = self.settings_state.dirty_choice;
        self.settings_state.dirty_choice = match direction {
            NavDir::Up | NavDir::Prev | NavDir::Home => cursor.previous(),
            NavDir::Down | NavDir::Next | NavDir::End => cursor.next(),
            NavDir::PageUp(_) | NavDir::PageDown(_) => cursor,
        };
        true
    }

    /// Apply the focused detail row, when it has an edit to make.
    fn activate_settings_row(&mut self) -> bool {
        if self.settings_state.focus == SettingsFocus::Sections {
            self.settings_state.focus = SettingsFocus::Detail;
            self.settings_state.selected_row = 0;
            return true;
        }
        let rows = settings_view::detail_rows(&self.settings_state);
        let Some(edit) = rows
            .get(self.settings_state.selected_row)
            .and_then(settings_view::SettingsRow::activation)
        else {
            return false;
        };
        self.edit_settings(edit)
    }

    /// Write one typed value into the draft and revalidate the whole candidate.
    fn edit_settings(&mut self, edit: SettingsEdit) -> bool {
        if self.settings_state.draft.is_none() {
            return false;
        }
        if let SettingsEdit::Theme(theme) = &edit
            && !self.settings_theme_available(theme)
        {
            self.settings_state.notice = Some(format!("{theme} is not installed"));
            return true;
        }
        let preview = self.next_theme_preview(&edit);
        let Some(draft) = self.settings_state.draft.as_mut() else {
            return false;
        };
        if draft.status().is_saving() {
            return false;
        }
        draft.record(edit);
        if let Some(preview) = preview {
            draft.set_preview(Some(preview));
        }
        revalidate(draft);
        self.settings_state.notice = None;
        self.publish_settings_dirty()
    }

    /// The preview token this edit puts in flight, if it is a theme edit.
    fn next_theme_preview(&self, edit: &SettingsEdit) -> Option<ThemePreviewToken> {
        let theme = match edit {
            SettingsEdit::Theme(theme) => theme.clone(),
            SettingsEdit::Reset(SyntaxPath::Theme) => self.settings_state.opened_theme.clone()?,
            _ => return None,
        };
        let draft = self.settings_state.draft.as_ref()?;
        let active = self.settings_state.opened_theme.clone()?;
        ThemePreviewToken::apply(draft.generation(), draft.preview(), &active, theme).ok()
    }

    fn settings_theme_available(&self, theme: &ThemeId) -> bool {
        self.settings_state
            .themes
            .iter()
            .any(|choice| &choice.id == theme)
    }

    /// Tell the navigation dirty guard whether this screen now holds work.
    fn publish_settings_dirty(&mut self) -> bool {
        let Some(draft) = self.settings_state.draft.as_ref() else {
            return true;
        };
        let (dirty, token) = (draft.is_dirty(), draft.token());
        if dirty {
            let Ok(owner) = crate::domain::Id::parse(SETTINGS_OWNER) else {
                return true;
            };
            let _ = self.mark_screen_dirty(
                token,
                SaveIntent::Owner {
                    owner,
                    semantic_key: settings_save_key(),
                },
            );
        } else {
            let _ = self.mark_screen_clean();
        }
        true
    }

    /// Schedule one durable save of the current candidate.
    fn save_settings(&mut self, exit_after: bool) -> bool {
        let revision = self
            .settings_state
            .last_scheduled_revision
            .saturating_add(1);
        let Some(draft) = self.settings_state.draft.as_mut() else {
            return false;
        };
        revalidate(draft);
        if !draft.is_saveable() {
            self.settings_state.notice =
                Some("Save is blocked until the draft validates".to_owned());
            return true;
        }
        if exit_after {
            draft.exit_after_save();
        }
        draft.schedule(revision);
        draft.set_status(DraftStatus::Saving { revision });
        self.settings_state.last_scheduled_revision = revision;
        self.settings_state.notice = None;
        true
    }

    /// Adopt, ignore, or recover from one writer completion.
    fn complete_settings_save(&mut self, outcome: SettingsSaveOutcome) -> bool {
        let Some(draft) = self.settings_state.draft.as_mut() else {
            return false;
        };
        let revision = completion_revision(&outcome);
        if let Some(revision) = revision
            && !draft.answers_pending(revision)
        {
            // A completion for a revision the user has already replaced is a
            // fact about superseded work, so the newest pending save stands.
            return false;
        }
        match outcome {
            SettingsSaveOutcome::Written { revision, hash } => self.adopt_saved(revision, hash),
            SettingsSaveOutcome::Superseded { .. } => {
                draft.clear_pending();
                draft.set_status(DraftStatus::Dirty);
                true
            }
            SettingsSaveOutcome::Conflict { disk_hash } => {
                draft.clear_pending();
                draft.set_status(DraftStatus::Conflict { disk_hash });
                self.settings_state.recovery_row = 0;
                self.settings_state.notice =
                    Some("External edit detected: disk and draft preserved".to_owned());
                true
            }
            SettingsSaveOutcome::Failed { diagnostic } => {
                draft.clear_pending();
                draft.set_status(DraftStatus::Failed {
                    code: diagnostic.code,
                });
                self.settings_state.recovery_row = 0;
                self.settings_state.notice = Some(diagnostic.redacted_detail.clone());
                true
            }
        }
    }

    /// Make a completed save the new base.
    fn adopt_saved(&mut self, revision: u64, hash: crate::domain::sha256::Sha256) -> bool {
        let Some(draft) = self.settings_state.draft.as_mut() else {
            return false;
        };
        let Some(candidate) = draft.candidate().valid() else {
            return false;
        };
        let bytes = candidate.bytes().to_vec();
        let structural = candidate.structural();
        let Ok(base) = load_base(Some(&bytes)) else {
            return false;
        };
        draft.adopt(Arc::new(base), hash, revision);
        let adopted = draft.preview().cloned().map(ThemePreviewToken::adopt);
        draft.set_preview(None);
        revalidate(draft);
        let exits = draft.exits_after_save();
        if let Some(theme) = adopted {
            self.settings_state.opened_theme = Some(theme);
        }
        self.settings_state.last_scheduled_revision =
            self.settings_state.last_scheduled_revision.max(revision);
        self.settings_state.notice = Some(if structural {
            RESTART_NOTICE.to_owned()
        } else {
            "Saved".to_owned()
        });
        let _ = self.mark_screen_clean();
        if exits {
            let _ = self.leave_screen();
            self.close_settings();
        }
        true
    }

    /// Ask to leave, which the host dirty guard holds back when work is unsaved.
    ///
    /// The guard is the navigation reducer's, not this screen's: leaving a
    /// screen with unsaved work is the same question everywhere and has one
    /// answer. All this does is ask, and release the draft if the ask
    /// succeeded.
    fn leave_settings(&mut self) -> bool {
        let before = self.screen();
        let _ = self.leave_screen();
        if self.screen() == before {
            // The guard held the navigation back, so the draft stays exactly
            // where it is until the user answers.
            return true;
        }
        self.close_settings();
        true
    }

    /// Answer the host dirty guard, and do whatever it says the owner must now do.
    fn resolve_settings_dirty(&mut self, choice: DirtyChoice) -> bool {
        let before = self.screen();
        match self.resolve_dirty(choice) {
            DraftAction::Save { .. } => self.save_settings(true),
            DraftAction::RestoreBase { .. } => {
                self.discard_settings();
                if self.screen() != before {
                    self.close_settings();
                }
                true
            }
            DraftAction::None => {
                if self.screen() == before {
                    // Cancel is the only answer that leaves everything exactly
                    // as it was, which is worth saying out loud: the user asked
                    // to leave and did not.
                    self.settings_state.notice = Some("Kept your changes".to_owned());
                } else {
                    self.close_settings();
                }
                true
            }
        }
    }

    /// Abandon every edit and return to the draft's base.
    fn discard_settings(&mut self) -> bool {
        let Some(draft) = self.settings_state.draft.as_mut() else {
            return false;
        };
        draft.forget_edits();
        draft.clear_pending();
        draft.set_preview(None);
        draft.set_status(DraftStatus::Clean);
        revalidate(draft);
        self.settings_state.reload_confirm = false;
        self.settings_state.notice = Some("Changes discarded".to_owned());
        let _ = self.mark_screen_clean();
        true
    }

    /// Raise the confirmation a reload needs before it can discard work.
    ///
    /// A clean draft needs no confirmation, so the boundary reads the disk
    /// straight away; this only ever puts the question on screen.
    fn request_settings_reload(&mut self) -> bool {
        if !self.settings_state.is_dirty() || self.settings_state.reload_confirm {
            return false;
        }
        self.settings_state.reload_confirm = true;
        true
    }

    fn cancel_settings_reload(&mut self) -> bool {
        if !self.settings_state.reload_confirm {
            return false;
        }
        self.settings_state.reload_confirm = false;
        true
    }

    /// Report where a draft was exported, or why it was not.
    fn complete_settings_export(&mut self, result: Result<std::path::PathBuf, Diagnostic>) -> bool {
        self.settings_state.notice = Some(match result {
            Ok(path) => format!("Exported draft to {}", path.display()),
            Err(diagnostic) => format!(
                "{}: {}",
                diagnostic.code.as_str(),
                diagnostic.redacted_detail
            ),
        });
        true
    }

    fn clamp_settings_selection(&mut self) {
        let count = match self.settings_state.focus {
            SettingsFocus::Sections => SettingsSection::ALL.len(),
            SettingsFocus::Detail => settings_view::detail_rows(&self.settings_state).len(),
        };
        self.settings_state.selected_row = self
            .settings_state
            .selected_row
            .min(count.saturating_sub(1));
    }
}

/// The revision a completion answers for, when it names one.
const fn completion_revision(outcome: &SettingsSaveOutcome) -> Option<u64> {
    match outcome {
        SettingsSaveOutcome::Written { revision, .. }
        | SettingsSaveOutcome::Superseded { revision } => Some(*revision),
        // A conflict or a write failure never reached replacement, so the
        // writer has no revision of its own to report; the newest scheduled
        // save is the one it answers.
        SettingsSaveOutcome::Conflict { .. } | SettingsSaveOutcome::Failed { .. } => None,
    }
}

fn step(current: usize, count: usize, direction: NavDir) -> usize {
    if count == 0 {
        return 0;
    }
    let last = count - 1;
    match direction {
        NavDir::Up | NavDir::Prev => current.saturating_sub(1),
        NavDir::Down | NavDir::Next => (current + 1).min(last),
        // A Settings section is never longer than a screen, so a page is the
        // whole list and paging is the same movement as Home and End.
        NavDir::Home | NavDir::PageUp(_) => 0,
        NavDir::End | NavDir::PageDown(_) => last,
    }
}

/// Rebuild the complete candidate and the status the edits imply.
fn revalidate(draft: &mut SettingsDraft) {
    let edits = draft
        .edited_paths()
        .filter_map(|path| draft.edit(path).cloned())
        .collect::<Vec<_>>();
    let candidate = build_candidate(draft.base(), &edits, draft.base_expected());
    let unchanged = candidate
        .valid()
        .is_some_and(|candidate| candidate.bytes() == draft.base().document().original_bytes());
    draft.set_candidate(candidate);
    if unchanged {
        // Every edit put the document back exactly where it started, so there
        // is nothing unsaved left to warn about.
        draft.forget_edits();
        draft.set_preview(None);
    }
    if !draft.status().needs_recovery() && !draft.status().is_saving() {
        draft.set_status(if draft.is_dirty() {
            DraftStatus::Dirty
        } else {
            DraftStatus::Clean
        });
    }
}

/// Build the complete candidate one edit set describes.
fn build_candidate(
    base: &SettingsMigration,
    edits: &[SettingsEdit],
    expected: ExpectedHash,
) -> DraftCandidate {
    let Ok(catalog) = builtin_owner_catalog() else {
        return DraftCandidate::Blocked(vec![internal_diagnostic(
            "the compiled owner catalog is unavailable",
        )]);
    };
    match SettingsCandidate::from_edits(base, &catalog, edits, expected) {
        Ok(candidate) => DraftCandidate::Valid(Box::new(candidate)),
        Err(diagnostics) => DraftCandidate::Blocked(diagnostics),
    }
}

/// Load one settings base, or the diagnostics that stop it being editable.
fn load_base(bytes: Option<&[u8]>) -> Result<SettingsMigration, Vec<Diagnostic>> {
    let catalog = builtin_owner_catalog().map_err(|_| {
        vec![internal_diagnostic(
            "the compiled owner catalog is unavailable",
        )]
    })?;
    load_settings_base(bytes, &catalog)
}

fn internal_diagnostic(detail: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        CfgCode::E103,
        Severity::Error,
        DiagnosticPath::root(),
        None,
        "reinstall Jefe: the compiled configuration contract is malformed",
    );
    detail.clone_into(&mut diagnostic.redacted_detail);
    diagnostic
}
