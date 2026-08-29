//! `AppEvent` <-> `TerminalManagerMessage` conversion (issue #361 PR B).

use std::ops::ControlFlow;

use crate::messages::{NavDir, TerminalManagerMessage};
use crate::state::AppEvent;

impl From<TerminalManagerMessage> for AppEvent {
    fn from(message: TerminalManagerMessage) -> Self {
        message.into_app_event()
    }
}

impl TerminalManagerMessage {
    /// Convert a terminal-manager-domain [`AppEvent`] into the typed message.
    ///
    /// Returns [`ControlFlow::Continue`] with the event when it belongs to no
    /// terminal-manager layer, so the dispatcher can hand it to another domain
    /// instead of panicking.
    pub(super) fn try_from_app_event(event: AppEvent) -> ControlFlow<Self, AppEvent> {
        match event {
            AppEvent::EnterTerminalManagerMode => ControlFlow::Break(Self::EnterMode),
            AppEvent::ExitTerminalManagerMode => ControlFlow::Break(Self::ExitMode),
            AppEvent::TerminalManagerNavigateUp => ControlFlow::Break(Self::Navigate(NavDir::Up)),
            AppEvent::TerminalManagerNavigateDown => {
                ControlFlow::Break(Self::Navigate(NavDir::Down))
            }
            AppEvent::TerminalManagerNavigateHome => {
                ControlFlow::Break(Self::Navigate(NavDir::Home))
            }
            AppEvent::TerminalManagerNavigateEnd => ControlFlow::Break(Self::Navigate(NavDir::End)),
            AppEvent::RequestShellFocus { agent_id, origin } => {
                ControlFlow::Break(Self::RequestFocus { agent_id, origin })
            }
            AppEvent::ConfirmShellFocus(agent_id) => {
                ControlFlow::Break(Self::ConfirmFocus(agent_id))
            }
            AppEvent::FailShellFocus => ControlFlow::Break(Self::FailFocus),
            AppEvent::ShellPreviewResult {
                agent_id,
                generation,
                ok,
                lines,
            } => {
                let result = if ok { Ok(lines) } else { Err(()) };
                ControlFlow::Break(Self::PreviewResult {
                    agent_id,
                    generation,
                    result,
                })
            }
            AppEvent::ShellClosed(agent_id) => ControlFlow::Break(Self::ShellClosed(agent_id)),
            other => ControlFlow::Continue(other),
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
    use std::ops::ControlFlow;

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

    #[test]
    fn non_terminal_manager_events_continue_to_next_domain() {
        assert!(matches!(
            TerminalManagerMessage::try_from_app_event(AppEvent::Quit),
            ControlFlow::Continue(AppEvent::Quit)
        ));
        assert!(matches!(
            TerminalManagerMessage::try_from_app_event(AppEvent::FailShellFocus),
            ControlFlow::Break(TerminalManagerMessage::FailFocus)
        ));
    }
}
