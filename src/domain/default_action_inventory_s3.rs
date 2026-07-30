//! Supplemental source-audited S3 action rows.
//!
//! This internal split keeps the primary compiled inventory below the source
//! size hard limit while preserving one composition authority.

use super::{H, Spec};

pub(super) const SPECS: &[Spec] = &[
    spec!("split", "split.page-up", H::NavigatePageUp, ["PageUp"]),
    spec!(
        "split",
        "split.page-down",
        H::NavigatePageDown,
        ["PageDown"]
    ),
    spec!("split", "split.home", H::NavigateHome, ["Home"]),
    spec!("split", "split.end", H::NavigateEnd, ["End"]),
    spec!("split", "split.left", H::NavigateLeft, ["Left"]),
    spec!("split", "split.right", H::NavigateRight, ["Right"]),
    spec!("split", "split.cycle-pane", H::CyclePaneFocus, ["Tab"]),
    spec!("split", "split.new", H::NewAgentOrRepository, ["n"]),
    spec!(
        "split",
        "split.new-repository",
        H::OpenNewRepository,
        ["Shift+N"]
    ),
    spec!(
        "split",
        "split.delete-selection",
        H::OpenDeleteSelection,
        ["Ctrl+D"]
    ),
    spec!(
        "split",
        "split.kill-agent",
        H::KillSelectedAgent,
        ["Ctrl+K"]
    ),
    spec!(
        "split",
        "split.restart-agent",
        H::RestartSelectedAgent,
        ["Ctrl+R"]
    ),
    spec!(
        "split",
        "split.relaunch-agent",
        H::RelaunchSelectedAgent,
        ["l", "Shift+L"]
    ),
    spec!(
        "split",
        "split.open-help",
        H::OpenHelp,
        ["Shift+?", "h", "Shift+H", "F1"]
    ),
    spec!("split", "split.open-search", H::FocusDashboardSearch, ["/"]),
    spec!(
        "split",
        "split.focus-repositories",
        H::FocusRepositories,
        ["r", "Shift+R"]
    ),
    spec!(
        "split",
        "split.focus-agents",
        H::FocusAgents,
        ["a", "Shift+A"]
    ),
    spec!(
        "split",
        "split.focus-terminal",
        H::FocusTerminal,
        ["t", "Shift+T"]
    ),
    spec!(
        "split",
        "split.activate-selection",
        H::ActivateDashboardSelection,
        ["Enter"]
    ),
    spec!(
        "split",
        "split.toggle-terminal",
        H::ToggleTerminalFocus,
        ["F12"]
    ),
    spec!(
        "actions",
        "actions.toggle-terminal",
        H::ToggleTerminalFocus,
        ["F12"]
    ),
    spec!("errors", "errors.home", H::NavigateHome, ["Home"]),
    spec!("errors", "errors.end", H::NavigateEnd, ["End"]),
];
