//! Errors-mode messages (issue #292).
//!
//! The error log is purely local — no remote fetching, no async loads, no
//! mutation correlation. Messages cover only mode lifecycle, list navigation,
//! detail scrolling, and clearing the log.

use crate::messages::{NavDir, ScrollDir};

/// Errors mode messages.
#[derive(Debug, Clone)]
pub enum ErrorsMessage {
    EnterMode,
    ExitMode,
    RefocusList,
    Navigate(NavDir),
    Enter,
    CycleFocus,
    CycleFocusReverse,
    ScrollDetail(ScrollDir),
    ClearAll,
}

impl ErrorsMessage {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EnterMode => "EnterErrorsMode",
            Self::ExitMode => "ExitErrorsMode",
            Self::RefocusList => "RefocusErrorList",
            Self::Navigate(_) => "ErrorsNavigate",
            Self::Enter => "ErrorsEnter",
            Self::CycleFocus => "ErrorsCycleFocus",
            Self::CycleFocusReverse => "ErrorsCycleFocusReverse",
            Self::ScrollDetail(_) => "ErrorsScrollDetail",
            Self::ClearAll => "ErrorsClearAll",
        }
    }
}
