//! Generated inventory completeness golden (issue #383 S8, CW03-01).
//!
//! Provides the bidirectional source-dispatch projection for the compiled
//! action inventory.
//!
//! Two independent directions are gated here:
//!
//! 1. **No orphan row** — every generated `(context, chord, action, handler)`
//!    row resolves to a declared action whose handler is a member of the closed
//!    production dispatch surface.
//! 2. **No orphan handler** — every member of that closed dispatch surface is
//!    reachable from at least one generated row.
//!
//! [`handler_name`] is an exhaustive match, so adding a `HandlerKey` variant
//! fails to compile until it is named. [`ALL_HANDLERS`] enumerates the closed
//! surface; its completeness against the declared enum is gated by the focused
//! source scan in this module's tests.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::action_registry::{ActionId, HandlerKey};
use super::default_action_inventory::{InventoryError, compiled_inventory, golden_projection};
use super::input_context::ContextId;
use super::keymap::Chord;

/// One generated golden row: context, chord, action, and handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryGoldenRow {
    pub context: ContextId,
    pub chord: Chord,
    /// Canonical text of `chord`, carried so the golden is diffable as text.
    pub chord_text: String,
    pub action: ActionId,
    pub handler: HandlerKey,
    pub handler_name: String,
}

/// The bidirectional completeness result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletenessReport {
    /// Generated rows whose handler is outside the closed dispatch surface.
    pub rows_without_dispatch: Vec<String>,
    /// Closed dispatch handlers absent from every generated row.
    pub handlers_without_row: Vec<String>,
    /// Size of the closed dispatch surface.
    pub handler_count: usize,
    /// Number of generated rows examined.
    pub row_count: usize,
}

/// Project the generated golden in a deterministic, diffable order.
///
/// # Errors
///
/// [`InventoryError`] when the compiled inventory is not self-consistent.
pub fn generated_golden() -> Result<Vec<InventoryGoldenRow>, InventoryError> {
    let inventory = compiled_inventory()?;
    let rows = golden_projection(&inventory)?;
    let mut generated: Vec<InventoryGoldenRow> = rows
        .into_iter()
        .map(|row| InventoryGoldenRow {
            chord_text: row.chord.to_string(),
            handler_name: handler_name(row.handler).to_owned(),
            context: row.context,
            chord: row.chord,
            action: row.action,
            handler: row.handler,
        })
        .collect();
    generated.sort_by(|left, right| {
        (
            left.context.as_str(),
            left.chord_text.as_str(),
            left.action.as_str(),
        )
            .cmp(&(
                right.context.as_str(),
                right.chord_text.as_str(),
                right.action.as_str(),
            ))
    });
    Ok(generated)
}

/// Compute both completeness directions over the generated golden.
///
/// # Errors
///
/// [`InventoryError`] when the compiled inventory is not self-consistent.
pub fn inventory_completeness() -> Result<CompletenessReport, InventoryError> {
    let rows = generated_golden()?;
    let dispatchable: BTreeSet<&'static str> = dispatchable_handlers().into_iter().collect();

    let mut rows_without_dispatch = Vec::new();
    let mut covered: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        if dispatchable.contains(row.handler_name.as_str()) {
            *covered.entry(row.handler_name.clone()).or_default() += 1;
        } else {
            rows_without_dispatch.push(format!(
                "{} + {} -> {} ({})",
                row.context.as_str(),
                row.chord_text,
                row.action.as_str(),
                row.handler_name
            ));
        }
    }

    let handlers_without_row: Vec<String> = dispatchable
        .iter()
        .filter(|name| !covered.contains_key(**name))
        .map(|name| (*name).to_owned())
        .collect();

    Ok(CompletenessReport {
        rows_without_dispatch,
        handlers_without_row,
        handler_count: dispatchable.len(),
        row_count: rows.len(),
    })
}

/// The closed production dispatch surface, by stable handler name.
#[must_use]
pub fn dispatchable_handlers() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = ALL_HANDLERS.iter().copied().map(handler_name).collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The closed dispatch surface, declared exactly once.
///
/// Each entry names a `HandlerKey` variant and the representative value used
/// to enumerate it. The macro derives both `ALL_HANDLERS` and `handler_name`
/// from this one list, so the two can never drift, and a new variant fails to
/// compile until it is added here.
///
/// `JumpAgent` is parameterized; every accepted index shares one handler name,
/// so one representative is sufficient.
macro_rules! handler_surface {
    ($($variant:ident $(($representative:expr))?),* $(,)?) => {
        /// Every closed `HandlerKey`.
        ///
        /// One representative value per variant.
        pub const ALL_HANDLERS: &[HandlerKey] = &[
            $(HandlerKey::$variant $(($representative))?),*
        ];

        /// The stable name of one handler.
        ///
        /// Exhaustive: a new variant fails to compile until it is declared
        /// in `handler_surface!`.
        #[must_use]
        pub const fn handler_name(handler: HandlerKey) -> &'static str {
            match handler {
                $(HandlerKey::$variant { .. } => stringify!($variant)),*
            }
        }
    };
}

handler_surface! {
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
    SettingsMoveUp,
    SettingsMoveDown,
    JumpAgent(1),
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
}

impl fmt::Display for InventoryGoldenRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}\t{}\t{}\t{}",
            self.context.as_str(),
            self.chord_text,
            self.action.as_str(),
            self.handler_name
        )
    }
}
