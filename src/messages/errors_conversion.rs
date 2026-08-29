//! `AppEvent` <-> `ErrorsMessage` conversion (issue #292).

use std::ops::ControlFlow;

use crate::messages::{ErrorsMessage, NavDir, ScrollDir};
use crate::state::AppEvent;

impl From<ErrorsMessage> for AppEvent {
    fn from(message: ErrorsMessage) -> Self {
        message.into_app_event()
    }
}

impl ErrorsMessage {
    /// Convert an errors-domain [`AppEvent`] into the typed message.
    ///
    /// Returns [`ControlFlow::Continue`] with the event when it belongs to no
    /// errors layer, so the dispatcher can hand it to another domain instead
    /// of panicking.
    pub(super) fn try_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterErrorsMode => ControlFlow::Break(Self::EnterMode),
            AppEvent::ExitErrorsMode => ControlFlow::Break(Self::ExitMode),
            AppEvent::RefocusErrorList => ControlFlow::Break(Self::RefocusList),
            AppEvent::ErrorsNavigateUp => ControlFlow::Break(Self::Navigate(NavDir::Up)),
            AppEvent::ErrorsNavigateDown => ControlFlow::Break(Self::Navigate(NavDir::Down)),
            AppEvent::ErrorsNavigateHome => ControlFlow::Break(Self::Navigate(NavDir::Home)),
            AppEvent::ErrorsNavigateEnd => ControlFlow::Break(Self::Navigate(NavDir::End)),
            AppEvent::ErrorsEnter => ControlFlow::Break(Self::Enter),
            AppEvent::ErrorsCycleFocus => ControlFlow::Break(Self::CycleFocus),
            AppEvent::ErrorsCycleFocusReverse => ControlFlow::Break(Self::CycleFocusReverse),
            AppEvent::ErrorsScrollDetailUp => ControlFlow::Break(Self::ScrollDetail(ScrollDir::Up)),
            AppEvent::ErrorsScrollDetailDown => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::Down))
            }
            AppEvent::ErrorsScrollDetailPageUp => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::PageUp))
            }
            AppEvent::ErrorsScrollDetailPageDown => {
                ControlFlow::Break(Self::ScrollDetail(ScrollDir::PageDown))
            }
            AppEvent::CaptureSilentError(title, detail, source, timestamp) => {
                ControlFlow::Break(Self::CaptureSilent {
                    title,
                    detail,
                    source,
                    timestamp,
                })
            }
            AppEvent::ErrorsClearAll => ControlFlow::Break(Self::ClearAll),
            other => ControlFlow::Continue(other),
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
            Self::CaptureSilent {
                title,
                detail,
                source,
                timestamp,
            } => AppEvent::CaptureSilentError(title, detail, source, timestamp),
            Self::ClearAll => AppEvent::ErrorsClearAll,
        }
    }

    fn map_navigation(dir: NavDir) -> AppEvent {
        match dir {
            NavDir::Up | NavDir::Next | NavDir::Prev => AppEvent::ErrorsNavigateUp,
            NavDir::Down => AppEvent::ErrorsNavigateDown,
            NavDir::Home => AppEvent::ErrorsNavigateHome,
            NavDir::End => AppEvent::ErrorsNavigateEnd,
            // PageUp/PageDown scroll the detail pane in errors mode (the key
            // handler maps PageUp/PageDown to scroll events directly, so these
            // branches are only reached if Navigate(PageUp/Down) is constructed
            // programmatically).
            NavDir::PageUp(_) => AppEvent::ErrorsScrollDetailPageUp,
            NavDir::PageDown(_) => AppEvent::ErrorsScrollDetailPageDown,
        }
    }

    fn map_scroll(dir: ScrollDir) -> AppEvent {
        match dir {
            ScrollDir::Up => AppEvent::ErrorsScrollDetailUp,
            ScrollDir::Down => AppEvent::ErrorsScrollDetailDown,
            ScrollDir::PageUp => AppEvent::ErrorsScrollDetailPageUp,
            ScrollDir::PageDown => AppEvent::ErrorsScrollDetailPageDown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_errors_events_continue_to_next_domain() {
        assert!(matches!(
            ErrorsMessage::try_from_app_event(AppEvent::Quit),
            ControlFlow::Continue(AppEvent::Quit)
        ));
        assert!(matches!(
            ErrorsMessage::try_from_app_event(AppEvent::ErrorsNavigateDown),
            ControlFlow::Break(ErrorsMessage::Navigate(NavDir::Down))
        ));
    }
}
