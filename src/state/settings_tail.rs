//! Settings interaction helpers kept separate from the draft reducer.
//!
//! These methods own modal bookkeeping and bounded row navigation. They read
//! the committed screen registry only to clamp the visible detail selection;
//! they do not load, save, or reconstruct Settings declarations.

use crate::messages::NavDir;
use crate::messages::settings::SettingsSection;
use crate::persistence::diagnostic::Diagnostic;

use super::{AppState, SettingsFocus, settings_view};

impl AppState {
    /// Raise the confirmation a reload needs before it can discard work.
    ///
    /// A clean draft needs no confirmation, so the boundary reads the disk
    /// straight away; this only ever puts the question on screen.
    pub(super) fn request_settings_reload(&mut self) -> bool {
        if !self.settings_state.is_dirty() || self.settings_state.reload_confirm {
            return false;
        }
        self.settings_state.reload_confirm = true;
        true
    }

    pub(super) fn cancel_settings_reload(&mut self) -> bool {
        if !self.settings_state.reload_confirm {
            return false;
        }
        self.settings_state.reload_confirm = false;
        true
    }

    /// Report where a draft was exported, or why it was not.
    pub(super) fn complete_settings_export(
        &mut self,
        result: Result<std::path::PathBuf, Diagnostic>,
    ) -> bool {
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

    pub(super) fn clamp_settings_selection(&mut self) {
        let count = match self.settings_state.focus {
            SettingsFocus::Sections => SettingsSection::ALL.len(),
            SettingsFocus::Detail => settings_view::detail_rows(
                &self.settings_state,
                self.settings_projection_authority(),
            )
            .len(),
        };
        self.settings_state.selected_row = self
            .settings_state
            .selected_row
            .min(count.saturating_sub(1));
    }
}

pub(super) fn step(current: usize, count: usize, direction: NavDir) -> usize {
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
