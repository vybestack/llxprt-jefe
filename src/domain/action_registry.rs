//! Closed action and binding value types for configurable keymaps.
use super::effects::Correlation;
use super::input_context::{ContextId, ContextStack};
use super::keymap::{Chord, MAX_CHORDS_PER_BINDING};
use std::fmt;
use unicode_width::UnicodeWidthStr;

#[path = "action_registry_chord_cmp.rs"]
mod action_registry_chord_cmp;
#[path = "action_registry_validate.rs"]
mod action_registry_validate;
use action_registry_chord_cmp::chords_equivalent;
use action_registry_validate::{
    apply_overrides, build_resolved, find_resolved, validate_actions_and_bindings,
    validate_availability, validate_context_conflicts, validate_context_stacks,
    validate_cross_contexts, validate_effective_binding_count, validate_overrides,
    validate_protected,
};

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
    OpenSettings,
    SettingsBack,
    SettingsUp,
    SettingsDown,
    SettingsCyclePane,
    SettingsCyclePaneReverse,
    SettingsActivate,
    SettingsSelectPrevious,
    SettingsSelectNext,
    SettingsSave,
    SettingsSaveAndExit,
    SettingsReset,
    SettingsToggle,
    SettingsUnbind,
    SettingsAddChord,
    SettingsMoveUp,
    SettingsMoveDown,
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
    WorkbenchBack,
    ProviderPanelPrevious,
    ProviderPanelNext,
    ProviderPanelActivate,
    ProviderPanelRetry,
    ProviderPanelCancel,
    ProviderPanelAction,
    ProviderPanelSubmit,
    ProviderPanelPageNext,
    ProviderPanelLinkSelect,
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
    DashboardGrabStart,
    DashboardGrabDrop,
    DashboardGrabUp,
    DashboardGrabDown,
    ExitSplit,
    EnterSplitGrab,
    WorkbenchToggleFilter,
    WorkbenchFilterPrev,
    WorkbenchFilterNext,
    WorkbenchPrevPage,
    WorkbenchNextPage,
    WorkbenchSelectPrev,
    WorkbenchSelectNext,
    WorkbenchAttach,
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
    SearchApply,
    SearchCancel,
    SearchBackspace,
    FilterApply,
    FilterCancel,
    FilterNextField,
    FilterPreviousField,
    FilterClearCurrent,
    FilterClearAll,
    FilterPreviousChoice,
    FilterNextChoice,
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
    /// A provider action dispatched through the post-commit provider effect
    /// path (issue #390 CW-10, Slice D). The `ActionId` arrives separately
    /// from the `Resolution::Dispatch` — this variant is a closed unit marker
    /// so `HandlerKey` stays `Copy`.
    ProviderAction,
}
/// Why a protected action cannot be rebound.
///
/// The rule is the registry's own: a protected action must stay bound and
/// available in every context that declares it, and composition refuses a
/// candidate that breaks either half. The wording lives beside the rule so a
/// screen reporting it and a refusal say the same thing.
pub const PROTECTED_ACTION_REASON: &str =
    "protected controls are read-only: they must stay bound and available";

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
        // Duplicate detection is canonical, matching composition: `Shift+Tab`
        // and `BackTab` are the same chord, so a binding cannot claim both here
        // and then be rejected later by the composition gate.
        for (index, chord) in self.chords.iter().enumerate() {
            if self.chords[..index]
                .iter()
                .any(|seen| chords_equivalent(seen, chord))
            {
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
/// One action's availability in a complete generation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct ActionAvailability(ActionId, Availability);

impl ActionAvailability {
    /// Pair an action with its snapshot availability.
    pub const fn new(action: ActionId, availability: Availability) -> Self {
        Self(action, availability)
    }

    #[must_use]
    pub(crate) const fn action(&self) -> &ActionId {
        &self.0
    }

    #[must_use]
    pub(crate) const fn availability(&self) -> &Availability {
        &self.1
    }
}

/// Complete availability data stamped with the existing exact correlation.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailabilityGeneration(Correlation, Vec<ActionAvailability>);

impl AvailabilityGeneration {
    /// Build one complete generation; composition validates coverage.
    pub const fn new(correlation: Correlation, entries: Vec<ActionAvailability>) -> Self {
        Self(correlation, entries)
    }

    pub(crate) fn entries(&self) -> &[ActionAvailability] {
        &self.1
    }
}

/// Typed whole-list replacement from one settings source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingOverride {
    context: ContextId,
    action: ActionId,
    chords: Vec<Chord>,
    provenance: Provenance,
}

impl BindingOverride {
    /// Build a whole-list replacement. An empty list explicitly unbinds.
    #[must_use]
    pub fn new(context: ContextId, action: ActionId, chords: Vec<Chord>, source: &str) -> Self {
        Self {
            context,
            action,
            chords,
            provenance: Provenance::Settings {
                source: source.to_owned(),
            },
        }
    }
}

/// Complete immutable input to one atomic registry composition attempt.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryCandidate {
    actions: Vec<Action>,
    bindings: Vec<Binding>,
    overrides: Vec<BindingOverride>,
    context_stacks: Vec<ContextStack>,
    availability: AvailabilityGeneration,
}

impl RegistryCandidate {
    /// Own every value needed to compose one validated snapshot.
    pub const fn new(
        actions: Vec<Action>,
        bindings: Vec<Binding>,
        overrides: Vec<BindingOverride>,
        context_stacks: Vec<ContextStack>,
        availability: AvailabilityGeneration,
    ) -> Self {
        Self {
            actions,
            bindings,
            overrides,
            context_stacks,
            availability,
        }
    }

    /// Validate the entire candidate and produce one immutable snapshot.
    pub fn compose(mut self) -> Result<ActionRegistrySnapshot, RegistryDiagnostic> {
        validate_actions_and_bindings(&self.actions, &self.bindings)?;
        validate_context_stacks(&self.actions, &self.context_stacks)?;
        validate_overrides(&self.actions, &self.overrides)?;
        apply_overrides(&mut self.bindings, &self.overrides);
        validate_effective_binding_count(&self.bindings)?;
        validate_context_conflicts(&self.bindings)?;
        let availability = validate_availability(&self.actions, self.availability)?;
        validate_cross_contexts(
            &self.actions,
            &self.bindings,
            &self.overrides,
            &self.context_stacks,
        )?;
        validate_protected(&self.actions, &self.bindings, &availability)?;
        let resolved = build_resolved(&self.actions, &self.bindings, &availability)?;
        Ok(ActionRegistrySnapshot {
            actions: self.actions,
            availability,
            bindings: self.bindings,
            context_stacks: self.context_stacks,
            resolved,
        })
    }
}

/// Immutable, completely validated action/binding/availability authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRegistrySnapshot {
    actions: Vec<Action>,
    availability: AvailabilityGeneration,
    bindings: Vec<Binding>,
    context_stacks: Vec<ContextStack>,
    resolved: Vec<ResolvedBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedBinding(ContextId, Chord, Resolution, bool);

impl ActionRegistrySnapshot {
    /// Resolve one canonical chord against one validated ordered stack.
    #[must_use]
    pub fn resolve(&self, chord: &Chord, stack: &ContextStack) -> Resolution {
        if stack.is_terminal_capture() {
            return self.resolve_terminal(chord, stack);
        }
        for context in stack.iter() {
            if let Some(binding) = find_resolved(&self.resolved, context, chord) {
                return binding.2.clone();
            }
        }
        Resolution::Unbound
    }

    /// Exact correlation that produced this snapshot's availability values.
    #[must_use]
    pub const fn availability_correlation(&self) -> &Correlation {
        &self.availability.0
    }

    /// This snapshot's availability for one action.
    ///
    /// The snapshot is the single authority for why an action cannot run, so
    /// every surface that shows a reason reads it from here and they cannot
    /// drift apart.
    #[must_use]
    pub fn availability_of(&self, action: &ActionId) -> Option<&Availability> {
        self.availability
            .entries()
            .iter()
            .find(|entry| entry.action() == action)
            .map(ActionAvailability::availability)
    }

    #[must_use]
    pub(crate) fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// Every action a package provider contributed (issue #390 CW-10).
    ///
    /// Identified by handler rather than by an id convention: `ProviderAction`
    /// is the only handler a package can be lowered onto, so a package cannot
    /// disguise itself as a compiled action by choosing its id.
    pub fn provider_actions(&self) -> impl Iterator<Item = &Action> {
        self.actions
            .iter()
            .filter(|action| matches!(action.handler, HandlerKey::ProviderAction))
    }

    pub(crate) fn availability_entries(&self) -> &[ActionAvailability] {
        self.availability.entries()
    }

    pub(crate) fn publish_availability(
        &self,
        generation: AvailabilityGeneration,
    ) -> Result<Self, RegistryDiagnostic> {
        let availability = validate_availability(&self.actions, generation)?;
        validate_protected(&self.actions, &self.bindings, &availability)?;
        let resolved = build_resolved(&self.actions, &self.bindings, &availability)?;
        Ok(Self {
            actions: self.actions.clone(),
            availability,
            bindings: self.bindings.clone(),
            context_stacks: self.context_stacks.clone(),
            resolved,
        })
    }

    #[must_use]
    pub(crate) fn effective_bindings(&self) -> &[Binding] {
        &self.bindings
    }

    #[must_use]
    pub(crate) fn context_stack(&self, context: &ContextId) -> Option<&ContextStack> {
        self.context_stacks
            .iter()
            .find(|stack| stack.iter().next() == Some(context))
    }

    fn resolve_terminal(&self, chord: &Chord, stack: &ContextStack) -> Resolution {
        stack
            .iter()
            .filter_map(|context| find_resolved(&self.resolved, context, chord))
            .find(|binding| binding.3)
            .map_or(Resolution::ForwardToPty, |binding| binding.2.clone())
    }
}

/// Single pure outcome of resolving a canonical chord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Dispatch {
        action: ActionId,
        handler: HandlerKey,
    },
    Unavailable {
        action: ActionId,
        reason: String,
    },
    ForwardToPty,
    Unbound,
}

/// Typed cause carried by every `KEY-E401` candidate rejection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryDiagnosticKind {
    DuplicateAction(ActionId),
    UnknownAction(ActionId),
    UnknownContext(ContextId),
    ActionContextMismatch(ContextId, ActionId),
    DuplicateBinding(ContextId, ActionId),
    DuplicateOverride(ContextId, ActionId),
    DuplicateChord(ContextId, ActionId, Chord),
    TooManyChords(ContextId, ActionId, usize, usize),
    TooManyEffectiveBindings(usize, usize),
    ContextConflict(ContextId, Chord, ActionId, ActionId),
    ImplicitShadow(ContextId, ContextId, Chord),
    ProtectedUnbound(ActionId, ContextId),
    ProtectedShadowed(ActionId, ContextId, Chord),
    ProtectedUnavailable(ActionId),
    DuplicateAvailability(ActionId),
    MissingAvailability(ActionId),
    UnknownAvailability(ActionId),
}

/// Complete typed registry diagnostic; composition errors use `KEY-E401`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryDiagnostic(RegistryDiagnosticKind);

impl RegistryDiagnostic {
    /// Stable keymap validation code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "KEY-E401"
    }

    /// Typed rejection cause.
    #[must_use]
    pub const fn kind(&self) -> &RegistryDiagnosticKind {
        &self.0
    }
}

impl fmt::Display for RegistryDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.code(), self.0)
    }
}
impl std::error::Error for RegistryDiagnostic {}
