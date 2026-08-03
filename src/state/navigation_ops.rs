//! How the rest of the reducer asks to change screen (issue #386).
//!
//! Every screen change in the program funnels through these three verbs, and
//! each is one call into [`reduce_navigation`]. Nothing assigns a screen any
//! more, so the stack, the generations, and the dirty guard cannot disagree
//! with what is on screen.
//!
//! The three verbs preserve exactly the movement the modes used to perform by
//! hand:
//!
//! - [`AppState::enter_screen`] is how a mode was opened from the dashboard,
//!   and it keeps the screen it came from so Back returns there;
//! - [`AppState::switch_screen`] is the cross-mode jump (`i` from pull
//!   requests, `p` from issues), which took the place of the current screen
//!   rather than stacking on it, so Back still returns to the dashboard;
//! - [`AppState::leave_screen`] is how a mode was closed, returning to
//!   whatever was underneath.

use crate::workbench::{ActivationValues, ScreenId, screen_registry};

use super::AppState;
use super::navigation::{
    Activation, NavIntent, NavMessage, NavOutcome, NavState, reduce_navigation,
};
use super::navigation_dirty::{DirtyChoice, DraftAction, DraftToken, SaveIntent};

impl AppState {
    /// The screen the session is on.
    ///
    /// Reads route through the navigation authority rather than a field of
    /// their own, so there is exactly one answer to "which screen is this".
    #[must_use]
    pub const fn screen(&self) -> ScreenId {
        self.nav.screen()
    }

    /// Open `screen`, keeping the current one to come back to.
    pub fn enter_screen(&mut self, screen: ScreenId) -> DraftAction {
        let activation = self.activation_for(screen);
        self.navigate(NavMessage::Navigate(NavIntent::Push(activation)))
    }

    /// Ensure the session is on `screen`, without stacking a second copy of it.
    ///
    /// Some transitions state where the session should end up rather than that
    /// it should move — hiding a shell returns to the terminal manager whether
    /// or not the manager is already the current screen. Pushing in that case
    /// would stack a second instance of the screen the user is already looking
    /// at, and repeating it would fill the stack.
    pub fn show_screen(&mut self, screen: ScreenId) -> DraftAction {
        if self.nav.screen() == screen {
            return DraftAction::None;
        }
        self.enter_screen(screen)
    }

    /// Move to `screen` in place of the current one.
    pub fn switch_screen(&mut self, screen: ScreenId) -> DraftAction {
        let activation = self.activation_for(screen);
        self.navigate(NavMessage::Navigate(NavIntent::Replace(activation)))
    }

    /// Return to the screen underneath this one.
    ///
    /// A restored session opens directly on whatever screen it was last on,
    /// with nothing beneath it, and leaving that screen has always taken the
    /// user home rather than stranding them. So leaving means: go back if
    /// there is somewhere to go back to, otherwise go home — and if this
    /// already is home, stay.
    pub fn leave_screen(&mut self) -> DraftAction {
        if self.nav.depth() == 0 && self.nav.screen() != ScreenId::default() {
            return self.switch_screen(ScreenId::default());
        }
        self.navigate(NavMessage::Navigate(NavIntent::Back))
    }

    /// Record that this screen now holds unsaved work.
    pub fn mark_screen_dirty(&mut self, draft: DraftToken, save: SaveIntent) -> DraftAction {
        self.navigate(NavMessage::MarkDirty { draft, save })
    }

    /// Record that this screen no longer holds unsaved work.
    pub fn mark_screen_clean(&mut self) -> DraftAction {
        self.navigate(NavMessage::MarkClean)
    }

    /// Answer the dirty guard.
    pub fn resolve_dirty(&mut self, choice: DirtyChoice) -> DraftAction {
        self.navigate(NavMessage::ResolveDirty(choice))
    }

    /// An activation for `screen`, computed from the live current instance.
    ///
    /// Compiled screens declare no activation fields, so this carries no
    /// values; it carries the provenance that lets the reducer refuse a
    /// request computed against a screen that has since been replaced.
    fn activation_for(&self, screen: ScreenId) -> Activation {
        Activation::from_source(
            crate::workbench::route_of(screen),
            ActivationValues::empty(),
            self.nav.current(),
        )
    }

    /// Commit one navigation message, surfacing any refusal and reporting what
    /// the draft's owner must now do.
    fn navigate(&mut self, message: NavMessage) -> DraftAction {
        let Ok(registry) = screen_registry() else {
            // The compiled screen table is malformed, which the descriptor
            // tests already fail on. Refusing to move and saying so is the only
            // honest answer; moving anyway would be worse than not moving.
            self.error_message = Some("NAV-E001: the screen registry is unavailable".to_owned());
            return DraftAction::None;
        };
        let transition = reduce_navigation(
            std::mem::replace(&mut self.nav, NavState::rooted(ScreenId::default())),
            registry,
            message,
        );
        self.nav = transition.state;
        if let NavOutcome::Refused(refusal) = &transition.outcome {
            self.error_message = Some(refusal.to_string());
        }
        transition.draft
    }
}
