//! Event bus for application-wide event handling.
//!
//! This module defines the core event types and provides a simple
//! synchronous event bus for handling user input and UI events.

#![allow(dead_code)]
#![allow(clippy::unnecessary_wraps)]

/// Application-wide events triggered by user input or system actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// Quit the application.
    Quit,

    /// Navigate up in the current list/view.
    NavigateUp,

    /// Navigate down in the current list/view.
    NavigateDown,

    /// Navigate to the previous pane/section.
    NavigateLeft,

    /// Navigate to the next pane/section.
    NavigateRight,

    /// Select/activate the current item (Enter key).
    Select,

    /// Go back/cancel current action (Escape key).
    Back,

    /// Create a new agent ('n' key).
    NewAgent,

    /// Create a new repository ('N' key).
    NewRepository,

    /// Delete the selected agent ('d' key).
    DeleteAgent,

    /// Open search/filter input ('/' key).
    OpenSearch,

    /// Open help dialog ('?' key).
    OpenHelp,

    /// Focus repository pane ('r' key).
    FocusRepository,

    /// Focus agent list pane ('a' key).
    FocusAgentList,

    /// Focus terminal pane ('t' key).
    FocusTerminal,

    /// Toggle split mode ('s' key).
    ToggleSplitMode,

    /// Kill/terminate the running agent ('k' key).
    KillAgent,

    /// Relaunch dead agent ('l' key on dead agent).
    RelaunchAgent,

    /// Exit split mode to main with current selection and terminal focus ('m' key).
    ReturnToMainFocused,

    /// Toggle terminal focus mode (F12 key).
    ToggleTerminalFocus,

    /// Character input for text entry modes.
    Char(char),

    /// Delete the selected repository ('D' key).
    DeleteRepository,

    /// Submit the current form (Enter on forms).
    SubmitForm,

    /// Move to the next form field (Tab).
    NextField,

    /// Move to the previous form field (Shift+Tab).
    PrevField,

    /// Backspace in form text input.
    Backspace,
}

impl AppEvent {
    /// Attempts to parse a keyboard input into an `AppEvent`.
    ///
    /// Returns `None` if the input doesn't map to any known event.
    #[must_use]
    pub const fn from_key(key: char) -> Option<Self> {
        match key {
            'q' => Some(Self::Quit),
            'n' => Some(Self::NewAgent),
            'N' => Some(Self::NewRepository),
            'd' => Some(Self::DeleteAgent),
            '/' => Some(Self::OpenSearch),
            '?' => Some(Self::OpenHelp),
            'r' => Some(Self::FocusRepository),
            'a' => Some(Self::FocusAgentList),
            't' => Some(Self::FocusTerminal),
            's' => Some(Self::ToggleSplitMode),
            'k' => Some(Self::KillAgent),
            'l' => Some(Self::RelaunchAgent),
            'm' => Some(Self::ReturnToMainFocused),
            c => Some(Self::Char(c)),
        }
    }

    /// Checks if this event represents a quit action.
    #[must_use]
    pub const fn is_quit(&self) -> bool {
        matches!(self, Self::Quit)
    }

    /// Checks if this event represents navigation.
    #[must_use]
    pub const fn is_navigation(&self) -> bool {
        matches!(
            self,
            Self::NavigateUp | Self::NavigateDown | Self::NavigateLeft | Self::NavigateRight
        )
    }

    /// Checks if this event represents a character input.
    #[must_use]
    pub const fn is_char(&self) -> bool {
        matches!(self, Self::Char(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_equality() {
        assert_eq!(AppEvent::Quit, AppEvent::Quit);
        assert_ne!(AppEvent::Quit, AppEvent::Back);
    }

    #[test]
    fn test_from_key_quit() {
        assert_eq!(AppEvent::from_key('q'), Some(AppEvent::Quit));
    }

    #[test]
    fn test_from_key_new_agent() {
        assert_eq!(AppEvent::from_key('n'), Some(AppEvent::NewAgent));
    }

    #[test]
    fn test_from_key_help() {
        assert_eq!(AppEvent::from_key('?'), Some(AppEvent::OpenHelp));
    }

    #[test]
    fn test_from_key_focus_and_split_actions() {
        assert_eq!(AppEvent::from_key('r'), Some(AppEvent::FocusRepository));
        assert_eq!(AppEvent::from_key('a'), Some(AppEvent::FocusAgentList));
        assert_eq!(AppEvent::from_key('t'), Some(AppEvent::FocusTerminal));
        assert_eq!(AppEvent::from_key('s'), Some(AppEvent::ToggleSplitMode));
        assert_eq!(AppEvent::from_key('m'), Some(AppEvent::ReturnToMainFocused));
    }

    #[test]
    fn test_from_key_char() {
        assert_eq!(AppEvent::from_key('x'), Some(AppEvent::Char('x')));
    }

    #[test]
    fn test_is_quit() {
        assert!(AppEvent::Quit.is_quit());
        assert!(!AppEvent::Back.is_quit());
        assert!(!AppEvent::NavigateUp.is_quit());
    }

    #[test]
    fn test_is_navigation() {
        assert!(AppEvent::NavigateUp.is_navigation());
        assert!(AppEvent::NavigateDown.is_navigation());
        assert!(AppEvent::NavigateLeft.is_navigation());
        assert!(AppEvent::NavigateRight.is_navigation());
        assert!(!AppEvent::Quit.is_navigation());
        assert!(!AppEvent::Select.is_navigation());
    }

    #[test]
    fn test_is_char() {
        assert!(AppEvent::Char('x').is_char());
        assert!(!AppEvent::Quit.is_char());
        assert!(!AppEvent::NavigateUp.is_char());
    }

    #[test]
    fn test_char_extraction() {
        if let AppEvent::Char(c) = AppEvent::from_key('z').unwrap() {
            assert_eq!(c, 'z');
        } else {
            panic!("Expected Char event");
        }
    }
}
