//! Validated source-derived defaults for the CW-03 action registry.
//!
//! The inventory is data-only in S0. Runtime input dispatch remains unchanged.
//! Each handler names one current operation; raw text insertion and rapid `qqq`
//! sequence tracking are intentionally outside the single-chord registry.

use std::fmt;

use super::action_registry::{
    Action, ActionError, ActionId, ActionIdError, ActionMetadata, Availability, Binding,
    BindingError, HandlerKey, Provenance,
};
use super::input_context::{ContextId, ContextIdError};
use super::keymap::{Chord, ChordError};
use HandlerKey as H;

/// Production keyboard authorities audited for this frozen inventory.
pub const AUDITED_DISPATCH_SOURCES: &[&str] = &[
    "src/input.rs",
    "src/app_shell_key_routing.rs",
    "src/app_input/mod.rs",
    "src/app_input/normal.rs",
    "src/app_input/dashboard_search.rs",
    "src/app_input/errors.rs",
    "src/app_input/terminal_manager.rs",
    "src/app_input/shell_overlay.rs",
    "src/app_input/modal_handlers.rs",
    "src/app_input/actions.rs",
    "src/app_input/issues.rs",
    "src/app_input/issues_filter.rs",
    "src/app_input/filter_controls.rs",
    "src/app_input/prs.rs",
    "src/app_input/pty_passthrough.rs",
    "src/pty_encoding.rs",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledInventory {
    pub actions: Vec<Action>,
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Copy)]
struct Spec {
    context: &'static str,
    id: &'static str,
    handler: HandlerKey,
    chords: &'static [&'static str],
    protected: bool,
}

macro_rules! spec {
    ($context:literal, $id:literal, $handler:expr, [$($chord:literal),+ $(,)?]) => {
        Spec { context: $context, id: $id, handler: $handler, chords: &[$($chord),+], protected: false }
    };
    (protected $context:literal, $id:literal, $handler:expr, [$($chord:literal),+ $(,)?]) => {
        Spec { context: $context, id: $id, handler: $handler, chords: &[$($chord),+], protected: true }
    };
}

const SPECS: &[Spec] = &[
    spec!(protected "global", "core.emergency-exit", H::EmergencyExit, ["Ctrl+Q"]),
    spec!("global", "core.open-keys", H::OpenKeys, [","]),
    spec!("global", "core.jump-agent.1", H::JumpAgent(1), ["Alt+1"]),
    spec!("global", "core.jump-agent.2", H::JumpAgent(2), ["Alt+2"]),
    spec!("global", "core.jump-agent.3", H::JumpAgent(3), ["Alt+3"]),
    spec!("global", "core.jump-agent.4", H::JumpAgent(4), ["Alt+4"]),
    spec!("global", "core.jump-agent.5", H::JumpAgent(5), ["Alt+5"]),
    spec!("global", "core.jump-agent.6", H::JumpAgent(6), ["Alt+6"]),
    spec!("global", "core.jump-agent.7", H::JumpAgent(7), ["Alt+7"]),
    spec!("global", "core.jump-agent.8", H::JumpAgent(8), ["Alt+8"]),
    spec!("global", "core.jump-agent.9", H::JumpAgent(9), ["Alt+9"]),
    spec!(protected "terminal", "core.leave-terminal", H::LeaveTerminal, ["F12"]),
    spec!(
        "terminal",
        "terminal.scroll-page-up",
        H::TerminalScrollPageUp,
        ["PageUp"]
    ),
    spec!(
        "terminal",
        "terminal.scroll-page-down",
        H::TerminalScrollPageDown,
        ["PageDown"]
    ),
    spec!(
        "terminal",
        "terminal.scroll-top",
        H::TerminalScrollTop,
        ["Home"]
    ),
    spec!(
        "terminal",
        "terminal.scroll-tail",
        H::TerminalScrollTail,
        ["End"]
    ),
    spec!(
        "terminal",
        "terminal.scroll-up",
        H::TerminalScrollUp,
        ["Up"]
    ),
    spec!(
        "terminal",
        "terminal.scroll-down",
        H::TerminalScrollDown,
        ["Down"]
    ),
    spec!("shell-overlay", "shell.hide", H::HideShellOverlay, ["F12"]),
    spec!(
        "shell-overlay",
        "shell.close",
        H::CloseShellOverlay,
        ["F10"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-up",
        H::NavigateUp,
        ["Up", "k"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-down",
        H::NavigateDown,
        ["Down", "j"]
    ),
    spec!(
        "dashboard",
        "dashboard.page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "dashboard",
        "dashboard.page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-home",
        H::NavigateHome,
        ["Home"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-end",
        H::NavigateEnd,
        ["End"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-left",
        H::NavigateLeft,
        ["Left"]
    ),
    spec!(
        "dashboard",
        "dashboard.navigate-right",
        H::NavigateRight,
        ["Right"]
    ),
    spec!(
        "dashboard",
        "dashboard.cycle-pane",
        H::CyclePaneFocus,
        ["Tab"]
    ),
    spec!("dashboard", "dashboard.new", H::NewAgentOrRepository, ["n"]),
    spec!(
        "dashboard",
        "dashboard.new-repository",
        H::OpenNewRepository,
        ["Shift+N"]
    ),
    spec!(
        "dashboard",
        "dashboard.delete-selection",
        H::OpenDeleteSelection,
        ["Ctrl+D"]
    ),
    spec!(
        "dashboard",
        "dashboard.kill-agent",
        H::KillSelectedAgent,
        ["Ctrl+K"]
    ),
    spec!(
        "dashboard",
        "dashboard.restart-agent",
        H::RestartSelectedAgent,
        ["Ctrl+R"]
    ),
    spec!(
        "dashboard",
        "dashboard.relaunch-agent",
        H::RelaunchSelectedAgent,
        ["l", "Shift+L"]
    ),
    spec!(
        "dashboard",
        "github.open-issues",
        H::EnterIssues,
        ["i", "Shift+I"]
    ),
    spec!(
        "dashboard",
        "github.open-pull-requests",
        H::EnterPullRequests,
        ["p", "Shift+P"]
    ),
    spec!(
        "dashboard",
        "github.open-actions",
        H::EnterActions,
        ["g", "Shift+G"]
    ),
    spec!(
        "dashboard",
        "dashboard.open-errors",
        H::EnterErrors,
        ["e", "Shift+E"]
    ),
    spec!(
        "dashboard",
        "dashboard.open-split",
        H::EnterSplit,
        ["s", "Shift+S"]
    ),
    spec!(
        "dashboard",
        "dashboard.open-terminals",
        H::EnterTerminalManager,
        ["F7"]
    ),
    spec!(
        "dashboard",
        "dashboard.open-help",
        H::OpenHelp,
        ["Shift+?", "h", "Shift+H", "F1"]
    ),
    spec!(
        "dashboard",
        "dashboard.focus-search",
        H::FocusDashboardSearch,
        ["/"]
    ),
    spec!(
        "dashboard",
        "dashboard.toggle-hidden-repositories",
        H::ToggleHiddenRepositories,
        ["v", "Shift+V"]
    ),
    spec!(
        "dashboard",
        "dashboard.focus-repositories",
        H::FocusRepositories,
        ["r", "Shift+R"]
    ),
    spec!(
        "dashboard",
        "dashboard.focus-agents",
        H::FocusAgents,
        ["a", "Shift+A"]
    ),
    spec!(
        "dashboard",
        "dashboard.focus-terminal",
        H::FocusTerminal,
        ["t", "Shift+T"]
    ),
    spec!(
        "dashboard",
        "dashboard.activate-selection",
        H::ActivateDashboardSelection,
        ["Enter"]
    ),
    spec!(
        "dashboard",
        "dashboard.open-theme-picker",
        H::OpenThemePicker,
        ["F9"]
    ),
    spec!(
        "dashboard",
        "shell.open-embedded",
        H::OpenEmbeddedShell,
        ["F10"]
    ),
    spec!(
        "dashboard",
        "shell.open-external",
        H::OpenExternalTerminal,
        ["F8"]
    ),
    spec!(
        "dashboard",
        "dashboard.toggle-terminal",
        H::ToggleTerminalFocus,
        ["F12"]
    ),
    spec!(
        "dashboard.grab",
        "dashboard.grab-drop",
        H::DashboardGrabDrop,
        [" ", "Enter"]
    ),
    spec!(
        "dashboard.grab",
        "dashboard.grab-up",
        H::DashboardGrabUp,
        ["Up"]
    ),
    spec!(
        "dashboard.grab",
        "dashboard.grab-down",
        H::DashboardGrabDown,
        ["Down"]
    ),
    spec!(
        "dashboard.reorder",
        "dashboard.grab-start",
        H::DashboardGrabStart,
        [" "]
    ),
    spec!(protected "split", "split.back", H::ExitSplit, ["Esc"]),
    spec!(
        "split",
        "split.enter-grab",
        H::EnterSplitGrab,
        ["g", "Shift+G"]
    ),
    spec!("split", "split.navigate-up", H::NavigateUp, ["Up", "k"]),
    spec!(
        "split",
        "split.navigate-down",
        H::NavigateDown,
        ["Down", "j"]
    ),
    spec!("errors", "errors.back", H::ErrorsBack, ["Esc"]),
    spec!("errors", "errors.up", H::ErrorsUp, ["Up"]),
    spec!("errors", "errors.down", H::ErrorsDown, ["Down"]),
    spec!("errors", "errors.page-up", H::ErrorsPageUp, ["PageUp"]),
    spec!(
        "errors",
        "errors.page-down",
        H::ErrorsPageDown,
        ["PageDown"]
    ),
    spec!("errors", "errors.activate", H::ErrorsActivate, ["Enter"]),
    spec!("errors", "errors.cycle-pane", H::ErrorsCyclePane, ["Tab"]),
    spec!("errors", "errors.clear", H::ErrorsClear, ["Ctrl+C"]),
    spec!(
        "terminal-manager",
        "terminal-manager.back",
        H::TerminalManagerBack,
        ["Esc", "F12"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.up",
        H::TerminalManagerUp,
        ["Up"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.down",
        H::TerminalManagerDown,
        ["Down"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.home",
        H::TerminalManagerHome,
        ["Home"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.end",
        H::TerminalManagerEnd,
        ["End"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.close-shell",
        H::TerminalManagerCloseShell,
        ["Ctrl+K"]
    ),
    spec!(
        "terminal-manager",
        "terminal-manager.focus-shell",
        H::TerminalManagerFocusShell,
        ["Enter"]
    ),
    spec!("help", "help.close", H::HelpClose, ["Esc", "Shift+?"]),
    spec!("help", "help.scroll-up", H::HelpScrollUp, ["Up"]),
    spec!("help", "help.scroll-down", H::HelpScrollDown, ["Down"]),
    spec!("help", "help.page-up", H::HelpPageUp, ["PageUp"]),
    spec!("help", "help.page-down", H::HelpPageDown, ["PageDown"]),
    spec!("help", "help.home", H::HelpHome, ["Home"]),
    spec!("help", "help.end", H::HelpEnd, ["End"]),
    spec!(
        "modal.confirm",
        "confirm.cancel",
        H::ConfirmCancel,
        ["Esc", "n", "Shift+N"]
    ),
    spec!(
        "modal.confirm",
        "confirm.cycle-focus",
        H::ConfirmCycleFocus,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!(
        "modal.confirm",
        "confirm.accept",
        H::ConfirmAccept,
        ["Enter"]
    ),
    spec!(
        "modal.confirm",
        "confirm.toggle-workdir",
        H::ConfirmToggleDeleteWorkDir,
        [" ", "d", "Shift+D", "Up", "Down"]
    ),
    spec!("modal.auth", "auth.cancel", H::AuthCancel, ["Esc"]),
    spec!(
        "modal.auth",
        "auth.retry",
        H::AuthRetry,
        ["r", "Shift+R", "Enter"]
    ),
    spec!("modal.form", "form.cancel", H::FormCancel, ["Esc"]),
    spec!("modal.form", "form.submit", H::FormSubmit, ["Enter"]),
    spec!(
        "modal.form",
        "form.next-field",
        H::FormNextField,
        ["Tab", "Down"]
    ),
    spec!(
        "modal.form",
        "form.previous-field",
        H::FormPreviousField,
        ["BackTab", "Up"]
    ),
    spec!(
        "modal.form",
        "form.cursor-left",
        H::FormCursorLeft,
        ["Left"]
    ),
    spec!(
        "modal.form",
        "form.cursor-right",
        H::FormCursorRight,
        ["Right"]
    ),
    spec!(
        "modal.form",
        "form.cursor-start",
        H::FormCursorStart,
        ["Home"]
    ),
    spec!("modal.form", "form.cursor-end", H::FormCursorEnd, ["End"]),
    spec!(
        "modal.form",
        "form.backspace",
        H::FormBackspace,
        ["Backspace"]
    ),
    spec!("modal.form", "form.delete", H::FormDelete, ["Delete"]),
    spec!("modal.theme", "theme.up", H::ThemeUp, ["Up"]),
    spec!("modal.theme", "theme.down", H::ThemeDown, ["Down"]),
    spec!(
        "modal.theme",
        "theme.toggle-override",
        H::ThemeToggleOverride,
        ["Tab"]
    ),
    spec!("modal.theme", "theme.apply", H::ThemeApply, ["Enter"]),
    spec!("modal.theme", "theme.cancel", H::ThemeCancel, ["Esc"]),
    spec!("search", "search.apply", H::SearchApply, ["Enter"]),
    spec!("search", "search.cancel", H::SearchCancel, ["Esc"]),
    spec!("search", "search.clear", H::SearchClear, ["Ctrl+L"]),
    spec!(
        "search",
        "search.backspace",
        H::SearchBackspace,
        ["Backspace"]
    ),
    spec!("filter", "filter.apply", H::FilterApply, ["Enter"]),
    spec!("filter", "filter.cancel", H::FilterCancel, ["Esc"]),
    spec!(
        "filter",
        "filter.next-field",
        H::FilterNextField,
        ["Tab", "Down"]
    ),
    spec!(
        "filter",
        "filter.previous-field",
        H::FilterPreviousField,
        ["BackTab", "Up"]
    ),
    spec!(
        "filter",
        "filter.clear-current",
        H::FilterClearCurrent,
        ["Ctrl+C"]
    ),
    spec!("filter", "filter.clear-all", H::FilterClearAll, ["Ctrl+L"]),
    spec!(
        "filter",
        "filter.previous-choice",
        H::FilterPreviousChoice,
        ["Left"]
    ),
    spec!(
        "filter",
        "filter.next-choice",
        H::FilterNextChoice,
        ["Right", " "]
    ),
    spec!(
        "filter",
        "filter.backspace",
        H::FilterBackspace,
        ["Backspace"]
    ),
    spec!("issues.list", "issues.exit", H::IssuesExit, ["a", "Esc"]),
    spec!("issues.list", "issues.open", H::IssuesOpen, ["Enter"]),
    spec!("issues.list", "issues.new", H::IssuesNew, ["n", "Shift+N"]),
    spec!(
        "issues.list",
        "issues.open-filter",
        H::IssuesOpenFilter,
        ["f"]
    ),
    spec!(
        "issues.list",
        "issues.focus-search",
        H::IssuesFocusSearch,
        ["/"]
    ),
    spec!("issues.list", "issues.up", H::NavigateUp, ["Up", "k"]),
    spec!("issues.list", "issues.down", H::NavigateDown, ["Down", "j"]),
    spec!(
        "issues.list",
        "issues.page-up",
        H::NavigatePageUp,
        ["PageUp"]
    ),
    spec!(
        "issues.list",
        "issues.page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!("issues.list", "issues.home", H::NavigateHome, ["Home"]),
    spec!("issues.list", "issues.end", H::NavigateEnd, ["End"]),
    spec!(
        "issues.list",
        "issues.cycle-pane",
        H::IssuesCyclePane,
        ["Left", "Right", "Tab", "BackTab"]
    ),
    spec!("issues.detail", "issues.back", H::IssuesBack, ["Esc"]),
    spec!("issues.detail", "issues.edit", H::IssuesEdit, ["e"]),
    spec!("issues.detail", "issues.comment", H::IssuesComment, ["c"]),
    spec!("issues.detail", "issues.reply", H::IssuesReply, ["r"]),
    spec!(
        "issues.detail",
        "issues.send-agent",
        H::IssuesSendToAgent,
        ["Shift+S"]
    ),
    spec!(
        "issues.inline",
        "issues.inline-submit",
        H::IssuesSubmitInline,
        ["Ctrl+Enter"]
    ),
    spec!(
        "issues.inline",
        "issues.inline-cancel",
        H::IssuesCancelInline,
        ["Ctrl+C", "Esc"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-previous",
        H::IssuesChooserPrevious,
        ["Up"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-next",
        H::IssuesChooserNext,
        ["Down"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-confirm",
        H::IssuesChooserConfirm,
        ["Enter"]
    ),
    spec!(
        "issues.agent-chooser",
        "issues.chooser-cancel",
        H::IssuesChooserCancel,
        ["Esc"]
    ),
    spec!("prs.list", "prs.exit", H::PullRequestsExit, ["a", "Esc"]),
    spec!("prs.list", "prs.open", H::PullRequestsOpen, ["Enter"]),
    spec!(
        "prs.list",
        "prs.open-filter",
        H::PullRequestsOpenFilter,
        ["f"]
    ),
    spec!(
        "prs.list",
        "prs.focus-search",
        H::PullRequestsFocusSearch,
        ["/"]
    ),
    spec!("prs.detail", "prs.back", H::PullRequestsBack, ["Esc"]),
    spec!("prs.detail", "prs.comment", H::PullRequestsComment, ["c"]),
    spec!("prs.detail", "prs.reply", H::PullRequestsReply, ["r"]),
    spec!(
        "prs.detail",
        "prs.resolve-thread",
        H::PullRequestsResolveThread,
        ["Shift+R"]
    ),
    spec!("prs.detail", "prs.edit", H::PullRequestsEdit, ["e"]),
    spec!(
        "prs.detail",
        "prs.send-agent",
        H::PullRequestsSendToAgent,
        ["Shift+S"]
    ),
    spec!(
        "prs.detail",
        "prs.open-browser",
        H::PullRequestsOpenBrowser,
        ["o"]
    ),
    spec!(
        "prs.detail",
        "prs.open-merge",
        H::PullRequestsOpenMerge,
        ["m"]
    ),
    spec!(
        "prs.inline",
        "prs.inline-submit",
        H::PullRequestsSubmitInline,
        ["Ctrl+Enter"]
    ),
    spec!(
        "prs.inline",
        "prs.inline-cancel",
        H::PullRequestsCancelInline,
        ["Ctrl+C", "Esc"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.chooser-previous",
        H::PullRequestsChooserPrevious,
        ["Up"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.chooser-next",
        H::PullRequestsChooserNext,
        ["Down"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.chooser-confirm",
        H::PullRequestsChooserConfirm,
        ["Enter"]
    ),
    spec!(
        "prs.agent-chooser",
        "prs.chooser-cancel",
        H::PullRequestsChooserCancel,
        ["Esc"]
    ),
    spec!("actions", "actions.exit", H::ActionsExit, ["a", "Esc"]),
    spec!("actions", "actions.reload", H::ActionsReload, ["r"]),
    spec!(
        "actions",
        "actions.open-filter",
        H::ActionsOpenFilter,
        ["f"]
    ),
    spec!(
        "actions",
        "actions.focus-search",
        H::ActionsFocusSearch,
        ["/"]
    ),
    spec!("actions", "actions.up", H::ActionsUp, ["Up", "k"]),
    spec!("actions", "actions.down", H::ActionsDown, ["Down", "j"]),
    spec!("actions", "actions.page-up", H::ActionsPageUp, ["PageUp"]),
    spec!(
        "actions",
        "actions.page-down",
        H::ActionsPageDown,
        ["PageDown"]
    ),
    spec!("actions", "actions.activate", H::ActionsActivate, ["Enter"]),
];

fn action_label(id: &str) -> String {
    let operation = id.rsplit('.').next().unwrap_or(id).replace('-', " ");
    let mut characters = operation.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => operation,
    }
}

pub fn compiled_inventory() -> Result<CompiledInventory, InventoryError> {
    let mut actions = Vec::with_capacity(SPECS.len());
    let mut bindings = Vec::with_capacity(SPECS.len());
    for spec in SPECS {
        let context = ContextId::parse(spec.context).map_err(InventoryError::Context)?;
        let id = ActionId::parse(spec.id).map_err(InventoryError::ActionId)?;
        if actions.iter().any(|action: &Action| action.id == id) {
            return Err(InventoryError::DuplicateAction(id));
        }
        let category = spec.id.split('.').next().unwrap_or(spec.id).to_owned();
        let label = action_label(spec.id);
        let action = Action::new(
            ActionMetadata {
                id: id.clone(),
                label: label.clone(),
                description: format!("{label}."),
                category,
                contexts: vec![context.clone()],
            },
            spec.handler,
            spec.protected,
        )
        .map_err(InventoryError::Action)?;
        let chords = spec
            .chords
            .iter()
            .map(|text| {
                Chord::parse(text).map_err(|source| InventoryError::Chord {
                    text: (*text).to_owned(),
                    source,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let binding = Binding::new(context, id, chords, Provenance::Compiled)
            .map_err(InventoryError::Binding)?;
        actions.push(action);
        bindings.push(binding);
    }
    Ok(CompiledInventory { actions, bindings })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenProjectionRow {
    pub context: ContextId,
    pub chord: Chord,
    pub action: ActionId,
    pub handler: HandlerKey,
    pub availability: Availability,
    pub provenance: Provenance,
}

pub fn golden_projection(
    inventory: &CompiledInventory,
) -> Result<Vec<GoldenProjectionRow>, InventoryError> {
    let mut rows = Vec::new();
    for binding in &inventory.bindings {
        let action = inventory
            .actions
            .iter()
            .find(|action| action.id == binding.action)
            .ok_or_else(|| InventoryError::UnknownAction(binding.action.clone()))?;
        for chord in &binding.chords {
            rows.push(GoldenProjectionRow {
                context: binding.context.clone(),
                chord: *chord,
                action: action.id.clone(),
                handler: action.handler,
                availability: Availability::Available,
                provenance: binding.provenance.clone(),
            });
        }
    }
    Ok(rows)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryError {
    Context(ContextIdError),
    ActionId(ActionIdError),
    Chord { text: String, source: ChordError },
    Action(ActionError),
    Binding(BindingError),
    DuplicateAction(ActionId),
    UnknownAction(ActionId),
}

impl fmt::Display for InventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid compiled action inventory: {self:?}")
    }
}
impl std::error::Error for InventoryError {}
