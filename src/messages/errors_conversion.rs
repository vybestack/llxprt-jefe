//! `AppEvent` <-> `ErrorsMessage` conversion (issue #292).

use crate::messages::{ErrorsMessage, NavDir, ScrollDir};
use crate::state::AppEvent;

impl From<ErrorsMessage> for AppEvent {
    fn from(message: ErrorsMessage) -> Self {
        message.into_app_event()
    }
}

impl ErrorsMessage {
    /// Convert an [`AppEvent`] into the corresponding [`ErrorsMessage`].
    ///
    /// # Panics
    /// Panics via `unreachable!` if the event does not belong to the Errors
    /// domain. Callers must only pass errors-domain events (guaranteed by
    /// [`AppMessage::from`]'s routing gate).
    pub(super) fn from_app_event(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterErrorsMode => Self::EnterMode,
            AppEvent::ExitErrorsMode => Self::ExitMode,
            AppEvent::RefocusErrorList => Self::RefocusList,
            AppEvent::ErrorsNavigateUp => Self::Navigate(NavDir::Up),
            AppEvent::ErrorsNavigateDown => Self::Navigate(NavDir::Down),
            AppEvent::ErrorsNavigateHome => Self::Navigate(NavDir::Home),
            AppEvent::ErrorsNavigateEnd => Self::Navigate(NavDir::End),
            AppEvent::ErrorsEnter => Self::Enter,
            AppEvent::ErrorsCycleFocus => Self::CycleFocus,
            AppEvent::ErrorsCycleFocusReverse => Self::CycleFocusReverse,
            AppEvent::ErrorsScrollDetailUp => Self::ScrollDetail(ScrollDir::Up),
            AppEvent::ErrorsScrollDetailDown => Self::ScrollDetail(ScrollDir::Down),
            AppEvent::ErrorsClearAll => Self::ClearAll,
            _ => unreachable!("unhandled event for ErrorsMessage: {:?}", event),
        }
    }

    #[must_use]
    pub fn into_app_event(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterErrorsMode,
            Self::ExitMode => AppEvent::ExitErrorsMode,
            Self::RefocusList => AppEvent::RefocusErrorList,
            Self::Navigate(dir) => Self::map_navigation(dir),
            Self::Enter => AppEvent::ErrorsEnter,
            Self::CycleFocus => AppEvent::ErrorsCycleFocus,
            Self::CycleFocusReverse => AppEvent::ErrorsCycleFocusReverse,
            Self::ScrollDetail(dir) => Self::map_scroll(dir),
            Self::ClearAll => AppEvent::ErrorsClearAll,
        }
    }

    fn map_navigation(dir: NavDir) -> AppEvent {
        match dir {
            NavDir::Up => AppEvent::ErrorsNavigateUp,
            NavDir::Down => AppEvent::ErrorsNavigateDown,
            NavDir::Home => AppEvent::ErrorsNavigateHome,
            NavDir::End => AppEvent::ErrorsNavigateEnd,
            // Page/Next/Prev don't have dedicated error variants; map Up/Down
            // as no-ops to keep the enum closed.
            NavDir::PageUp(_) | NavDir::PageDown(_) | NavDir::Next | NavDir::Prev => {
                AppEvent::ErrorsNavigateUp
            }
        }
    }

    fn map_scroll(dir: ScrollDir) -> AppEvent {
        match dir {
            ScrollDir::Up | ScrollDir::PageUp => AppEvent::ErrorsScrollDetailUp,
            ScrollDir::Down | ScrollDir::PageDown => AppEvent::ErrorsScrollDetailDown,
        }
    }
}
