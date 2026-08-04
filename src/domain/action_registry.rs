//! Closed action and binding value types for configurable keymaps.
use super::effects::Correlation;
use super::input_context::{ContextId, ContextStack};
use super::keymap::{Chord, MAX_CHORDS_PER_BINDING, MAX_EFFECTIVE_BINDINGS};
use std::fmt;
use unicode_width::UnicodeWidthStr;

#[path = "action_registry_chord_cmp.rs"]
mod action_registry_chord_cmp;
use action_registry_chord_cmp::{chords_equivalent, terminal_intercepts};

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

    #[must_use]
    pub(crate) fn actions(&self) -> &[Action] {
        &self.actions
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

fn diagnostic(kind: RegistryDiagnosticKind) -> RegistryDiagnostic {
    RegistryDiagnostic(kind)
}

fn validate_actions_and_bindings(
    actions: &[Action],
    bindings: &[Binding],
) -> Result<(), RegistryDiagnostic> {
    for (index, action) in actions.iter().enumerate() {
        if actions[..index].iter().any(|seen| seen.id == action.id) {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateAction(
                action.id.clone(),
            )));
        }
    }
    for (index, binding) in bindings.iter().enumerate() {
        validate_action_context(actions, &binding.action, &binding.context)?;
        validate_chord_list(
            binding.context.clone(),
            binding.action.clone(),
            &binding.chords,
        )?;
        if bindings[..index]
            .iter()
            .any(|seen| seen.context == binding.context && seen.action == binding.action)
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateBinding(
                binding.context.clone(),
                binding.action.clone(),
            )));
        }
    }
    Ok(())
}

fn validate_action_context(
    actions: &[Action],
    action_id: &ActionId,
    context: &ContextId,
) -> Result<(), RegistryDiagnostic> {
    let Some(action) = find_action(actions, action_id) else {
        return Err(diagnostic(RegistryDiagnosticKind::UnknownAction(
            action_id.clone(),
        )));
    };
    if !action.contexts.contains(context) {
        return Err(diagnostic(RegistryDiagnosticKind::ActionContextMismatch(
            context.clone(),
            action_id.clone(),
        )));
    }
    Ok(())
}

fn validate_context_stacks(
    actions: &[Action],
    stacks: &[ContextStack],
) -> Result<(), RegistryDiagnostic> {
    for stack in stacks {
        for context in stack.iter() {
            if !actions
                .iter()
                .any(|action| action.contexts.contains(context))
            {
                return Err(diagnostic(RegistryDiagnosticKind::UnknownContext(
                    context.clone(),
                )));
            }
        }
    }
    Ok(())
}

fn validate_overrides(
    actions: &[Action],
    overrides: &[BindingOverride],
) -> Result<(), RegistryDiagnostic> {
    for (index, candidate) in overrides.iter().enumerate() {
        let known_context = actions
            .iter()
            .any(|action| action.contexts.contains(&candidate.context));
        if !known_context {
            return Err(diagnostic(RegistryDiagnosticKind::UnknownContext(
                candidate.context.clone(),
            )));
        }
        validate_action_context(actions, &candidate.action, &candidate.context)?;
        validate_chord_list(
            candidate.context.clone(),
            candidate.action.clone(),
            &candidate.chords,
        )?;
        if overrides[..index]
            .iter()
            .any(|seen| seen.context == candidate.context && seen.action == candidate.action)
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateOverride(
                candidate.context.clone(),
                candidate.action.clone(),
            )));
        }
    }
    Ok(())
}

fn validate_chord_list(
    context: ContextId,
    action: ActionId,
    chords: &[Chord],
) -> Result<(), RegistryDiagnostic> {
    if chords.len() > MAX_CHORDS_PER_BINDING {
        return Err(diagnostic(RegistryDiagnosticKind::TooManyChords(
            context,
            action,
            chords.len(),
            MAX_CHORDS_PER_BINDING,
        )));
    }
    for (index, chord) in chords.iter().enumerate() {
        if chords[..index]
            .iter()
            .any(|seen| chords_equivalent(seen, chord))
        {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateChord(
                context, action, *chord,
            )));
        }
    }
    Ok(())
}

fn apply_overrides(bindings: &mut Vec<Binding>, overrides: &[BindingOverride]) {
    for candidate in overrides {
        bindings.retain(|binding| {
            binding.context != candidate.context || binding.action != candidate.action
        });
        if candidate.chords.is_empty() {
            continue;
        }
        bindings.push(Binding {
            context: candidate.context.clone(),
            action: candidate.action.clone(),
            chords: candidate.chords.clone(),
            provenance: candidate.provenance.clone(),
        });
    }
}

fn validate_effective_binding_count(bindings: &[Binding]) -> Result<(), RegistryDiagnostic> {
    let count = bindings.iter().map(|binding| binding.chords.len()).sum();
    if count > MAX_EFFECTIVE_BINDINGS {
        Err(diagnostic(
            RegistryDiagnosticKind::TooManyEffectiveBindings(count, MAX_EFFECTIVE_BINDINGS),
        ))
    } else {
        Ok(())
    }
}

fn validate_context_conflicts(bindings: &[Binding]) -> Result<(), RegistryDiagnostic> {
    for (index, first) in bindings.iter().enumerate() {
        for second in &bindings[index + 1..] {
            if first.context != second.context {
                continue;
            }
            if let Some(chord) = overlapping_chord(first, second) {
                return Err(diagnostic(RegistryDiagnosticKind::ContextConflict(
                    first.context.clone(),
                    chord,
                    first.action.clone(),
                    second.action.clone(),
                )));
            }
        }
    }
    Ok(())
}

fn validate_availability(
    actions: &[Action],
    generation: AvailabilityGeneration,
) -> Result<AvailabilityGeneration, RegistryDiagnostic> {
    for (index, entry) in generation.1.iter().enumerate() {
        if find_action(actions, &entry.0).is_none() {
            return Err(diagnostic(RegistryDiagnosticKind::UnknownAvailability(
                entry.0.clone(),
            )));
        }
        if generation.1[..index].iter().any(|seen| seen.0 == entry.0) {
            return Err(diagnostic(RegistryDiagnosticKind::DuplicateAvailability(
                entry.0.clone(),
            )));
        }
    }
    for action in actions {
        if !generation.1.iter().any(|entry| entry.0 == action.id) {
            return Err(diagnostic(RegistryDiagnosticKind::MissingAvailability(
                action.id.clone(),
            )));
        }
    }
    Ok(generation)
}

fn validate_cross_contexts(
    actions: &[Action],
    bindings: &[Binding],
    overrides: &[BindingOverride],
    stacks: &[ContextStack],
) -> Result<(), RegistryDiagnostic> {
    for stack in stacks {
        let contexts: Vec<_> = stack.iter().collect();
        for child_index in 0..contexts.len() {
            for parent in &contexts[child_index + 1..] {
                validate_context_pair(actions, bindings, overrides, contexts[child_index], parent)?;
            }
        }
    }
    Ok(())
}

fn validate_context_pair(
    actions: &[Action],
    bindings: &[Binding],
    overrides: &[BindingOverride],
    child: &ContextId,
    parent: &ContextId,
) -> Result<(), RegistryDiagnostic> {
    for child_binding in bindings.iter().filter(|binding| binding.context == *child) {
        for parent_binding in bindings.iter().filter(|binding| binding.context == *parent) {
            let Some(chord) = overlapping_chord(child_binding, parent_binding) else {
                continue;
            };
            let protected_child = binding_is_protected(actions, child_binding);
            let protected_parent = binding_is_protected(actions, parent_binding);
            if !protected_child && protected_parent {
                return Err(diagnostic(RegistryDiagnosticKind::ProtectedShadowed(
                    parent_binding.action.clone(),
                    parent_binding.context.clone(),
                    chord,
                )));
            }
            let parent_changed = override_exists(overrides, parent_binding);
            let child_changed = override_exists(overrides, child_binding);
            if parent_changed && !child_changed {
                return Err(diagnostic(RegistryDiagnosticKind::ImplicitShadow(
                    child.clone(),
                    parent.clone(),
                    chord,
                )));
            }
        }
    }
    Ok(())
}

fn validate_protected(
    actions: &[Action],
    bindings: &[Binding],
    availability: &AvailabilityGeneration,
) -> Result<(), RegistryDiagnostic> {
    for action in actions.iter().filter(|action| protected_action(action)) {
        for context in &action.contexts {
            if !bindings
                .iter()
                .any(|binding| binding.context == *context && binding.action == action.id)
            {
                return Err(diagnostic(RegistryDiagnosticKind::ProtectedUnbound(
                    action.id.clone(),
                    context.clone(),
                )));
            }
        }
        if availability.1.iter().any(|entry| {
            entry.0 == action.id && matches!(entry.1, Availability::Unavailable { .. })
        }) {
            return Err(diagnostic(RegistryDiagnosticKind::ProtectedUnavailable(
                action.id.clone(),
            )));
        }
    }
    Ok(())
}

fn find_action<'a>(actions: &'a [Action], id: &ActionId) -> Option<&'a Action> {
    actions.iter().find(|action| action.id == *id)
}

fn build_resolved(
    actions: &[Action],
    bindings: &[Binding],
    availability: &AvailabilityGeneration,
) -> Result<Vec<ResolvedBinding>, RegistryDiagnostic> {
    let mut resolved = Vec::new();
    for binding in bindings {
        let action = find_action(actions, &binding.action).ok_or_else(|| {
            diagnostic(RegistryDiagnosticKind::UnknownAction(
                binding.action.clone(),
            ))
        })?;
        let entry = availability
            .1
            .iter()
            .find(|entry| entry.0 == binding.action)
            .ok_or_else(|| {
                diagnostic(RegistryDiagnosticKind::MissingAvailability(
                    binding.action.clone(),
                ))
            })?;
        let outcome = action_resolution(action, &entry.1);
        for chord in &binding.chords {
            resolved.push(ResolvedBinding(
                binding.context.clone(),
                *chord,
                outcome.clone(),
                terminal_intercepts(action, chord),
            ));
        }
    }
    Ok(resolved)
}

fn action_resolution(action: &Action, availability: &Availability) -> Resolution {
    match availability {
        Availability::Available => Resolution::Dispatch {
            action: action.id.clone(),
            handler: action.handler,
        },
        Availability::Unavailable { reason } => Resolution::Unavailable {
            action: action.id.clone(),
            reason: reason.clone(),
        },
    }
}

fn find_resolved<'a>(
    bindings: &'a [ResolvedBinding],
    context: &ContextId,
    chord: &Chord,
) -> Option<&'a ResolvedBinding> {
    bindings
        .iter()
        .find(|binding| binding.0 == *context && chords_equivalent(&binding.1, chord))
}

fn overlapping_chord(first: &Binding, second: &Binding) -> Option<Chord> {
    first.chords.iter().find_map(|first_chord| {
        second
            .chords
            .iter()
            .any(|second_chord| chords_equivalent(first_chord, second_chord))
            .then_some(*first_chord)
    })
}

fn override_exists(overrides: &[BindingOverride], binding: &Binding) -> bool {
    overrides
        .iter()
        .any(|candidate| candidate.context == binding.context && candidate.action == binding.action)
}

fn binding_is_protected(actions: &[Action], binding: &Binding) -> bool {
    find_action(actions, &binding.action).is_some_and(protected_action)
}

/// The `protected` flag is the single authority. Re-listing action IDs here
/// would let an inventory row that forgot the flag look protected anyway, which
/// hides the real defect; the inventory tests assert the flag instead.
fn protected_action(action: &Action) -> bool {
    action.protected
}
