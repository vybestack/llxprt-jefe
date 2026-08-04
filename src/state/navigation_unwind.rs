//! What one Back key actually does (issue #386, CW06-04).
//!
//! Back used to mean whatever the focused mode decided it meant, and each mode
//! decided separately: issues had its own Esc chain, pull requests had a
//! near-identical one, actions and errors and the terminal manager had shorter
//! ones. The chains agreed by coincidence rather than by construction, so
//! "which thing closes first" was a question with five answers.
//!
//! There is now one precedence order, stated once, in
//! [`BackLayer::PRECEDENCE`]. Back closes the innermost thing that is open and
//! nothing else — one key press unwinds exactly one layer — and only when no
//! layer is open at all does it reach navigation.
//!
//! The resolver takes the layers that are open rather than a struct of flags,
//! so adding a layer is an addition to one ordered list instead of another
//! boolean whose position in a chain of `if`s is the real specification.
//!
//! **What is not yet true.** The per-mode key chains in `app_input` still
//! decide Esc and `q` for themselves; they have not been converted to ask this
//! resolver. So this states the order and answers correctly for anything that
//! consults it, but it is not yet the only thing deciding. Converting those
//! chains is the remaining half of the cutover, and until it lands the shipped
//! precedence is whatever those chains do — which the existing mode tests pin,
//! and which agrees with the order above.

use super::navigation_dirty::DirtyChoice;

/// One thing a Back key can close, named so it can be ordered against the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BackLayer {
    /// A host confirmation modal is trapping input.
    HostConfirmation,
    /// The host dirty guard is asking about unsaved work.
    DirtyGuard,
    /// A chooser overlay is open.
    Chooser,
    /// An inline editor or composer is open.
    Editor,
    /// A search input is focused.
    Search,
    /// A filter is applied or its controls are open.
    Filter,
    /// A non-dirty overlay is open.
    Overlay,
    /// The focused panel holds a transient of its own.
    PanelTransient,
}

impl BackLayer {
    /// Every layer, innermost first. This list *is* the precedence rule.
    pub const PRECEDENCE: [Self; 8] = [
        Self::HostConfirmation,
        Self::DirtyGuard,
        Self::Chooser,
        Self::Editor,
        Self::Search,
        Self::Filter,
        Self::Overlay,
        Self::PanelTransient,
    ];

    /// What closing this layer asks its owner to do.
    #[must_use]
    pub const fn intent(self) -> LocalIntent {
        match self {
            Self::HostConfirmation => LocalIntent::CloseHostConfirmation,
            // Esc answers the guard the same way the Cancel control does: keep
            // the draft, keep the screen, drop the navigation that raised it.
            Self::DirtyGuard => LocalIntent::ResolveDirty(DirtyChoice::Cancel),
            Self::Chooser => LocalIntent::CloseChooser,
            Self::Editor => LocalIntent::CloseEditor,
            Self::Search => LocalIntent::CloseSearch,
            Self::Filter => LocalIntent::ClearFilter,
            Self::Overlay => LocalIntent::CloseOverlay,
            Self::PanelTransient => LocalIntent::ClearPanelTransient,
        }
    }
}

/// What the owner of the unwound layer must do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalIntent {
    /// Dismiss the host confirmation modal.
    CloseHostConfirmation,
    /// Answer the host dirty guard.
    ResolveDirty(DirtyChoice),
    /// Close the open chooser.
    CloseChooser,
    /// Close the open editor or composer.
    CloseEditor,
    /// Leave the search input.
    CloseSearch,
    /// Clear the applied filter.
    ClearFilter,
    /// Close the open overlay.
    CloseOverlay,
    /// Clear the focused panel's own transient state.
    ClearPanelTransient,
}

/// What one Back key press resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackResolution {
    /// Unwind exactly this layer; the session stays on its screen.
    Local(LocalIntent),
    /// Nothing local is open; leave the screen.
    Leave,
    /// Nothing is open and there is nowhere to go back to.
    Nothing,
}

/// Resolve one Back key press.
///
/// `open` is the set of layers currently open, in any order — precedence comes
/// from [`BackLayer::PRECEDENCE`], never from the caller. `can_leave` says
/// whether navigation has somewhere to return to.
#[must_use]
pub fn resolve_back(open: &[BackLayer], can_leave: bool) -> BackResolution {
    for layer in BackLayer::PRECEDENCE {
        if open.contains(&layer) {
            return BackResolution::Local(layer.intent());
        }
    }
    if can_leave {
        BackResolution::Leave
    } else {
        BackResolution::Nothing
    }
}
