//! Closed action and binding value types for configurable keymaps.

use std::fmt;

use unicode_width::UnicodeWidthStr;

use super::input_context::ContextId;
use super::keymap::{Chord, MAX_CHORDS_PER_BINDING};

pub const ACTION_ID_BYTE_LIMIT: usize = 128;
pub const ACTION_LABEL_CELL_LIMIT: usize = 128;
pub const ACTION_DESCRIPTION_BYTE_LIMIT: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(String);

impl ActionId {
    pub fn parse(value: &str) -> Result<Self, ActionIdError> {
        let valid = !value.is_empty()
            && value.len() <= ACTION_ID_BYTE_LIMIT
            && value.as_bytes()[0].is_ascii_lowercase()
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
            && !value.contains("..")
            && !value.ends_with('.');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(ActionIdError {
                value: value.to_owned(),
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionIdError {
    pub value: String,
}

impl fmt::Display for ActionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid action id: {:?}", self.value)
    }
}

impl std::error::Error for ActionIdError {}

/// Closed dispatch intents. Every variant denotes one current source operation;
/// aliases may share a variant, but unrelated behavior never does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandlerKey {
    EmergencyExit,
    OpenKeys,
    JumpAgent(u8),
    TerminalScrollPageUp,
    TerminalScrollPageDown,
    TerminalScrollTop,
    TerminalScrollTail,
    TerminalScrollUp,
    TerminalScrollDown,
    LeaveTerminal,
    HideShellOverlay,
    CloseShellOverlay,
    OpenEmbeddedShell,
    OpenExternalTerminal,
    ToggleTerminalFocus,
    OpenHelp,
    HelpClose,
    HelpScrollUp,
    HelpScrollDown,
    HelpPageUp,
    HelpPageDown,
    HelpHome,
    HelpEnd,
    NavigateUp,
    NavigateDown,
    NavigatePageUp,
    NavigatePageDown,
    NavigateHome,
    NavigateEnd,
    NavigateLeft,
    NavigateRight,
    CyclePaneFocus,
    NewAgentOrRepository,
    OpenNewRepository,
    OpenDeleteSelection,
    KillSelectedAgent,
    RestartSelectedAgent,
    RelaunchSelectedAgent,
    EnterIssues,
    EnterPullRequests,
    EnterActions,
    EnterErrors,
    EnterSplit,
    EnterTerminalManager,
    FocusDashboardSearch,
    ToggleHiddenRepositories,
    FocusRepositories,
    FocusAgents,
    FocusTerminal,
    ActivateDashboardSelection,
    OpenThemePicker,
    DashboardGrabStart,
    DashboardGrabDrop,
    DashboardGrabUp,
    DashboardGrabDown,
    ExitSplit,
    EnterSplitGrab,
    ErrorsBack,
    ErrorsUp,
    ErrorsDown,
    ErrorsPageUp,
    ErrorsPageDown,
    ErrorsActivate,
    ErrorsCyclePane,
    ErrorsClear,
    TerminalManagerBack,
    TerminalManagerUp,
    TerminalManagerDown,
    TerminalManagerHome,
    TerminalManagerEnd,
    TerminalManagerCloseShell,
    TerminalManagerFocusShell,
    ConfirmCancel,
    ConfirmCycleFocus,
    ConfirmAccept,
    ConfirmToggleDeleteWorkDir,
    AuthCancel,
    AuthRetry,
    FormCancel,
    FormSubmit,
    FormNextField,
    FormPreviousField,
    FormCursorLeft,
    FormCursorRight,
    FormCursorStart,
    FormCursorEnd,
    FormBackspace,
    FormDelete,
    ThemeUp,
    ThemeDown,
    ThemeToggleOverride,
    ThemeApply,
    ThemeCancel,
    SearchApply,
    SearchCancel,
    SearchClear,
    SearchBackspace,
    FilterApply,
    FilterCancel,
    FilterNextField,
    FilterPreviousField,
    FilterClearCurrent,
    FilterClearAll,
    FilterPreviousChoice,
    FilterNextChoice,
    FilterBackspace,
    IssuesExit,
    IssuesBack,
    IssuesOpen,
    IssuesNew,
    IssuesOpenFilter,
    IssuesFocusSearch,
    IssuesEdit,
    IssuesComment,
    IssuesReply,
    IssuesSendToAgent,
    IssuesCyclePane,
    IssuesSubmitInline,
    IssuesCancelInline,
    IssuesChooserPrevious,
    IssuesChooserNext,
    IssuesChooserConfirm,
    IssuesChooserCancel,
    PullRequestsExit,
    PullRequestsBack,
    PullRequestsOpen,
    PullRequestsOpenFilter,
    PullRequestsFocusSearch,
    PullRequestsComment,
    PullRequestsReply,
    PullRequestsResolveThread,
    PullRequestsEdit,
    PullRequestsSendToAgent,
    PullRequestsOpenBrowser,
    PullRequestsOpenMerge,
    PullRequestsCyclePane,
    PullRequestsSubmitInline,
    PullRequestsCancelInline,
    PullRequestsChooserPrevious,
    PullRequestsChooserNext,
    PullRequestsChooserConfirm,
    PullRequestsChooserCancel,
    ActionsExit,
    ActionsReload,
    ActionsOpenFilter,
    ActionsFocusSearch,
    ActionsUp,
    ActionsDown,
    ActionsPageUp,
    ActionsPageDown,
    ActionsActivate,
    ActionsBack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    Compiled,
    Settings { source: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    pub label: String,
    pub description: String,
    pub category: String,
    pub contexts: Vec<ContextId>,
    pub handler: HandlerKey,
    pub protected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionMetadata {
    pub id: ActionId,
    pub label: String,
    pub description: String,
    pub category: String,
    pub contexts: Vec<ContextId>,
}

impl Action {
    pub fn new(
        metadata: ActionMetadata,
        handler: HandlerKey,
        protected: bool,
    ) -> Result<Self, ActionError> {
        let ActionMetadata {
            id,
            label,
            description,
            category,
            contexts,
        } = metadata;
        if label.trim().is_empty() {
            return Err(ActionError::EmptyLabel);
        }
        let cells = UnicodeWidthStr::width(label.as_str());
        if cells > ACTION_LABEL_CELL_LIMIT {
            return Err(ActionError::LabelTooWide {
                cells,
                limit: ACTION_LABEL_CELL_LIMIT,
            });
        }
        if description.trim().is_empty() {
            return Err(ActionError::EmptyDescription);
        }
        if description.len() > ACTION_DESCRIPTION_BYTE_LIMIT {
            return Err(ActionError::DescriptionTooLong {
                bytes: description.len(),
                limit: ACTION_DESCRIPTION_BYTE_LIMIT,
            });
        }
        if category.trim().is_empty() {
            return Err(ActionError::EmptyCategory);
        }
        if contexts.is_empty() {
            return Err(ActionError::EmptyContexts);
        }
        for (index, context) in contexts.iter().enumerate() {
            if contexts[..index].contains(context) {
                return Err(ActionError::DuplicateContext(context.clone()));
            }
        }
        Ok(Self {
            id,
            label,
            description,
            category,
            contexts,
            handler,
            protected,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionError {
    EmptyLabel,
    LabelTooWide { cells: usize, limit: usize },
    EmptyDescription,
    DescriptionTooLong { bytes: usize, limit: usize },
    EmptyCategory,
    EmptyContexts,
    DuplicateContext(ContextId),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid action metadata: {self:?}")
    }
}
impl std::error::Error for ActionError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub context: ContextId,
    pub action: ActionId,
    pub chords: Vec<Chord>,
    pub provenance: Provenance,
}

impl Binding {
    pub fn new(
        context: ContextId,
        action: ActionId,
        chords: Vec<Chord>,
        provenance: Provenance,
    ) -> Result<Self, BindingError> {
        let binding = Self {
            context,
            action,
            chords,
            provenance,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), BindingError> {
        if self.chords.is_empty() {
            return Err(BindingError::EmptyChords);
        }
        if self.chords.len() > MAX_CHORDS_PER_BINDING {
            return Err(BindingError::TooManyChords {
                count: self.chords.len(),
                limit: MAX_CHORDS_PER_BINDING,
            });
        }
        for (index, chord) in self.chords.iter().enumerate() {
            if self.chords[..index].contains(chord) {
                return Err(BindingError::DuplicateChord(*chord));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingError {
    EmptyChords,
    TooManyChords { count: usize, limit: usize },
    DuplicateChord(Chord),
}

impl fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid binding: {self:?}")
    }
}
impl std::error::Error for BindingError {}
