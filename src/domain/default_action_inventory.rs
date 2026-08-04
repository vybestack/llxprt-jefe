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
use super::input_context::{ContextId, ContextIdError, ContextStack, ContextStackError};
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
    pub(crate) context_stacks: Vec<ContextStack>,
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

#[path = "default_action_inventory_s3.rs"]
mod s3;

#[path = "default_action_inventory_s4.rs"]
mod s4;

#[path = "default_action_inventory_display.rs"]
pub(crate) mod display;
// The first declaration for a leaf is its provider-free explain order. Later
// declarations cover source-valid alternate parents during candidate validation.
const CONTEXT_STACK_SPECS: &[(&[&str], bool)] = &[
    (&["global"], false),
    (&["terminal", "global"], true),
    (&["shell-overlay"], false),
    (&["dashboard", "global"], false),
    (
        &["dashboard.grab", "dashboard.reorder", "dashboard", "global"],
        false,
    ),
    (&["dashboard.reorder", "dashboard", "global"], false),
    (&["split", "global"], false),
    (&["errors", "global"], false),
    (&["settings", "global"], false),
    (&["terminal-manager", "global"], false),
    (&["actions", "global"], false),
];

const SPECS: &[Spec] = &[
    spec!(protected "global", "core.emergency-exit", H::EmergencyExit, ["Ctrl+Q"]),
    spec!("global", "core.open-settings", H::OpenSettings, [","]),
    // `,` opened the Keys editor until the Settings shell claimed it. The Keys
    // editor keeps a single-key entry point on the chord the retired theme
    // picker used to hold, so no entry point was lost in the exchange.
    spec!("global", "core.open-keys", H::OpenKeys, ["F9"]),
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
    spec!(protected "errors", "errors.back", H::ErrorsBack, ["Esc"]),
    spec!("errors", "errors.up", H::ErrorsUp, ["Up", "k"]),
    spec!("errors", "errors.down", H::ErrorsDown, ["Down", "j"]),
    spec!("errors", "errors.page-up", H::ErrorsPageUp, ["PageUp"]),
    spec!(
        "errors",
        "errors.page-down",
        H::ErrorsPageDown,
        ["PageDown"]
    ),
    spec!("errors", "errors.activate", H::ErrorsActivate, ["Enter"]),
    spec!(
        "errors",
        "errors.cycle-pane",
        H::ErrorsCyclePane,
        ["Tab", "Right", "BackTab", "Left"]
    ),
    spec!("errors", "errors.clear", H::ErrorsClear, ["Ctrl+C"]),
    spec!(protected "settings", "settings.back", H::SettingsBack, ["Esc", "q"]),
    spec!("settings", "settings.up", H::SettingsUp, ["Up", "k"]),
    spec!("settings", "settings.down", H::SettingsDown, ["Down", "j"]),
    spec!(
        "settings",
        "settings.cycle-pane",
        H::SettingsCyclePane,
        ["Tab"]
    ),
    spec!(
        "settings",
        "settings.cycle-pane-reverse",
        H::SettingsCyclePaneReverse,
        ["BackTab"]
    ),
    spec!(
        "settings",
        "settings.activate",
        H::SettingsActivate,
        ["Enter", " "]
    ),
    // Left/Right step the horizontal choice a recovery offers, and otherwise
    // move the same selection the vertical keys do.
    spec!(
        "settings",
        "settings.select-previous",
        H::SettingsSelectPrevious,
        ["Left"]
    ),
    spec!(
        "settings",
        "settings.select-next",
        H::SettingsSelectNext,
        ["Right"]
    ),
    spec!("settings", "settings.save", H::SettingsSave, ["s"]),
    spec!(
        "settings",
        "settings.save-and-exit",
        H::SettingsSaveAndExit,
        ["Shift+S"]
    ),
    spec!("settings", "settings.reset", H::SettingsReset, ["r"]),
    spec!(
        "settings",
        "settings.open-help",
        H::OpenHelp,
        ["Shift+?", "F1"]
    ),
    spec!(protected
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
    let specs = SPECS.iter().chain(s3::SPECS).chain(s4::SPECS);
    let spec_count = specs.clone().count();
    let mut actions = Vec::with_capacity(spec_count);
    let mut bindings = Vec::with_capacity(spec_count);
    for spec in specs {
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
    let context_stacks = CONTEXT_STACK_SPECS
        .iter()
        .chain(s4::CONTEXT_STACK_SPECS)
        .map(|(contexts, terminal_capture)| {
            ContextStack::from_ordered(contexts.iter().copied(), *terminal_capture)
                .map_err(InventoryError::ContextStack)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CompiledInventory {
        actions,
        bindings,
        context_stacks,
    })
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
    ContextStack(ContextStackError),
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
