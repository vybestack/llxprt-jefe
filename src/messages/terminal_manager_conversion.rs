//! `AppEvent` <-> `TerminalManagerMessage` conversion (issue #361 PR B).

use crate::messages::{NavDir, TerminalManagerMessage};
use crate::state::AppEvent;

impl From<TerminalManagerMessage> for AppEvent {
    fn from(message: TerminalManagerMessage) -> Self {
        message.into_app_event()
    }
}

impl TerminalManagerMessage {
    /// Convert an [`AppEvent`] into the corresponding [`TerminalManagerMessage`].
    ///
    /// # Panics
    /// Panics via `unreachable!` if the event does not belong to the Terminal
    /// Manager domain. Callers must only pass terminal-manager events
    /// (guaranteed by [`AppMessage::from`]'s routing gate).
    pub(super) fn from_app_event(event: AppEvent) -> Self {
        match event {
            AppEvent::EnterTerminalManagerMode => Self::EnterMode,
            AppEvent::ExitTerminalManagerMode => Self::ExitMode,
            AppEvent::TerminalManagerNavigateUp => Self::Navigate(NavDir::Up),
            AppEvent::TerminalManagerNavigateDown => Self::Navigate(NavDir::Down),
            AppEvent::TerminalManagerNavigateHome => Self::Navigate(NavDir::Home),
            AppEvent::TerminalManagerNavigateEnd => Self::Navigate(NavDir::End),
            AppEvent::RequestShellFocus { agent_id, origin } => {
                Self::RequestFocus { agent_id, origin }
            }
            AppEvent::ConfirmShellFocus(agent_id) => Self::ConfirmFocus(agent_id),
            AppEvent::FailShellFocus => Self::FailFocus,
            AppEvent::ShellPreviewResult {
                agent_id,
                generation,
                ok,
                lines,
            } => {
                let result = if ok { Ok(lines) } else { Err(()) };
                Self::PreviewResult {
                    agent_id,
                    generation,
                    result,
                }
            }
            AppEvent::ShellClosed(agent_id) => Self::ShellClosed(agent_id),
            _ => unreachable!("unhandled event for TerminalManagerMessage: {:?}", event),
        }
    }

    #[must_use]
    pub fn into_app_event(self) -> AppEvent {
        match self {
            Self::EnterMode => AppEvent::EnterTerminalManagerMode,
            Self::ExitMode => AppEvent::ExitTerminalManagerMode,
            Self::Navigate(dir) => Self::map_navigation(dir),
            Self::RequestFocus { agent_id, origin } => {
                AppEvent::RequestShellFocus { agent_id, origin }
            }
            Self::ConfirmFocus(agent_id) => AppEvent::ConfirmShellFocus(agent_id),
            Self::FailFocus => AppEvent::FailShellFocus,
            Self::PreviewResult {
                agent_id,
                generation,
                result,
            } => {
                let (ok, lines) = match result {
                    Ok(lines) => (true, lines),
                    Err(()) => (false, Vec::new()),
                };
                AppEvent::ShellPreviewResult {
                    agent_id,
                    generation,
                    ok,
                    lines,
                }
            }
            Self::ShellClosed(agent_id) => AppEvent::ShellClosed(agent_id),
        }
    }

    fn map_navigation(dir: NavDir) -> AppEvent {
        match dir {
            NavDir::Up | NavDir::Prev | NavDir::PageUp(_) => AppEvent::TerminalManagerNavigateUp,
            NavDir::Down | NavDir::Next | NavDir::PageDown(_) => {
                AppEvent::TerminalManagerNavigateDown
            }
            NavDir::Home => AppEvent::TerminalManagerNavigateHome,
            NavDir::End => AppEvent::TerminalManagerNavigateEnd,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::list_viewport::PageItemCount;
    use crate::messages::{NavDir, TerminalManagerMessage};
    use crate::state::AppEvent;

    #[test]
    fn navigation_aliases_preserve_direction() {
        for direction in [
            NavDir::Up,
            NavDir::Prev,
            NavDir::PageUp(PageItemCount::new(2)),
        ] {
            assert!(matches!(
                TerminalManagerMessage::Navigate(direction).into_app_event(),
                AppEvent::TerminalManagerNavigateUp
            ));
        }
        for direction in [
            NavDir::Down,
            NavDir::Next,
            NavDir::PageDown(PageItemCount::new(2)),
        ] {
            assert!(matches!(
                TerminalManagerMessage::Navigate(direction).into_app_event(),
                AppEvent::TerminalManagerNavigateDown
            ));
        }
    }
}
