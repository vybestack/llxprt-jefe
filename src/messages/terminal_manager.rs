//! Terminal-manager messages (issue #361 PR B).
//!
//! Covers Terminal Manager screen lifecycle (enter/exit), list navigation,
//! cross-agent shell focus request/confirm/fail (generation-guarded), preview
//! capture correlation, and shell-closed cleanup. All payloads are plain data;
//! side effects happen at the runtime boundary before these messages reach
//! the reducer.

use crate::domain::AgentId;
use crate::messages::NavDir;

/// Terminal-manager mode messages.
#[derive(Debug, Clone)]
pub enum TerminalManagerMessage {
    /// Enter the Terminal Manager screen (F7).
    EnterMode,
    /// Exit back to Dashboard (Esc/F12).
    ExitMode,
    /// Navigate the shell list.
    Navigate(NavDir),
    /// Request a cross-agent focus on the selected Running owner. The input
    /// boundary drives the attach scheduler BEFORE dispatching this; the
    /// reducer only records the generation-guarded pending state.
    RequestFocus(AgentId),
    /// Confirm a pending focus after the expected owner attached. Rejected if
    /// the generation or owner no longer matches.
    ConfirmFocus(AgentId),
    /// Fail a pending focus (attach failed or owner no longer Running).
    FailFocus,
    /// A preview capture result for the selected shell. Correlated by owner
    /// and generation so stale captures are discarded.
    PreviewResult {
        agent_id: AgentId,
        generation: u64,
        result: Result<Vec<String>, ()>,
    },
    /// A shell was closed (runtime already removed the inventory entry).
    /// Clears the preview if it belonged to the closed shell and re-clamps
    /// the selection.
    ShellClosed(AgentId),
}

impl TerminalManagerMessage {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EnterMode => "EnterTerminalManager",
            Self::ExitMode => "ExitTerminalManager",
            Self::Navigate(_) => "TerminalManagerNavigate",
            Self::RequestFocus(_) => "RequestShellFocus",
            Self::ConfirmFocus(_) => "ConfirmShellFocus",
            Self::FailFocus => "FailShellFocus",
            Self::PreviewResult { .. } => "ShellPreviewResult",
            Self::ShellClosed(_) => "ShellClosed",
        }
    }
}
