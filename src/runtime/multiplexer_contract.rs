//! The enumerated psmux/tmux contract surface jefe depends on (issue #540).
//!
//! psmux is an external, unbundled binary that jefe treats as a drop-in tmux.
//! It is not one, and the delta has been discovered one production incident at
//! a time: the pane-command byte budget calibrated against macOS tmux (#433), a
//! default `PageUp` root binding tmux does not ship (#465), a server that
//! auto-exits when empty (#493), and `#{pid}` naming a per-session server
//! rather than the `-L` namespace (#540).
//!
//! This module is the authoritative list of every verb, format string and
//! server option jefe issues, with the response shape each must produce. It is
//! pure data: the conformance runner asserts these against the live binary, and
//! the mechanical surface check fails the build when production code uses
//! something absent from here.

/// Whether an item exists on every supported multiplexer or only on builds
/// carrying a specific upstream change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractCapability {
    /// Present on every multiplexer jefe supports.
    Always,
    /// Present only on psmux builds carrying the stable per-namespace server
    /// identity (upstream psmux#509). Older builds render the format as empty
    /// text and still exit zero, so absence is detectable without a version
    /// comparison.
    SincePsmuxNamespaceToken,
}

/// The category of a contract item, so lookups cannot confuse a verb with a
/// format string that happens to share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractItemKind {
    /// A command word such as `has-session`.
    Verb,
    /// A `#{…}` format variable.
    Format,
    /// A server option set through `set-option -s`.
    ServerOption,
}

/// The shape of the response an item produces, which is what the conformance
/// runner asserts. Checking for stdout that never arrives is as wrong as
/// failing to check stdout that does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseShape {
    /// The answer is the process exit status; stdout carries nothing useful.
    ExitStatusOnly,
    /// Exactly one line of output.
    SingleLine,
    /// One line per session.
    LinePerSession,
    /// One line per pane.
    LinePerPane,
    /// One line per window.
    LinePerWindow,
    /// Raw terminal content, whose shape depends on the pane rather than a
    /// format string.
    RawPaneContent,
    /// Success is signalled by exit status and no output is produced.
    NoOutput,
}

/// One declared dependency on the multiplexer.
#[derive(Debug, Clone, Copy)]
pub struct ContractItem {
    pub kind: ContractItemKind,
    /// The verb, format variable, or option name, without `#{}` decoration.
    pub name: &'static str,
    pub response: ResponseShape,
    pub capability: ContractCapability,
    /// Whether the value identifies the `-L` namespace itself and therefore
    /// stays constant while that namespace is up.
    ///
    /// Only meaningful for [`ContractItemKind::Format`]. This is the field that
    /// separates a namespace identity from the identity of whichever process
    /// answered, and getting it wrong is the #540 defect.
    pub namespace_stable: bool,
    /// Why jefe depends on this, so a future reader can tell a load-bearing
    /// item from an incidental one.
    pub rationale: &'static str,
}

const fn verb(
    name: &'static str,
    response: ResponseShape,
    rationale: &'static str,
) -> ContractItem {
    ContractItem {
        kind: ContractItemKind::Verb,
        name,
        response,
        capability: ContractCapability::Always,
        namespace_stable: false,
        rationale,
    }
}

const fn format(
    name: &'static str,
    namespace_stable: bool,
    capability: ContractCapability,
    rationale: &'static str,
) -> ContractItem {
    ContractItem {
        kind: ContractItemKind::Format,
        name,
        response: ResponseShape::SingleLine,
        capability,
        namespace_stable,
        rationale,
    }
}

const fn server_option(name: &'static str, rationale: &'static str) -> ContractItem {
    ContractItem {
        kind: ContractItemKind::ServerOption,
        name,
        response: ResponseShape::NoOutput,
        capability: ContractCapability::Always,
        namespace_stable: false,
        rationale,
    }
}

/// The authoritative contract surface.
///
/// Adding a verb or format to production code without adding it here is a
/// policy violation the mechanical check reports.
static CONTRACT: &[ContractItem] = &[
    // --- verbs ---
    verb(
        "has-session",
        ResponseShape::ExitStatusOnly,
        "authoritative existence check for a named session; the only probe that \
         answered correctly while the server identity probe was misreporting",
    ),
    verb(
        "list-sessions",
        ResponseShape::LinePerSession,
        "enumerates live sessions for liveness reconciliation",
    ),
    verb(
        "list-panes",
        ResponseShape::LinePerPane,
        "reads pane death state and pane leader PIDs",
    ),
    verb(
        "list-windows",
        ResponseShape::LinePerWindow,
        "enumerates windows for pane addressing and display indices",
    ),
    verb(
        "display-message",
        ResponseShape::SingleLine,
        "evaluates format strings, including the server identity probe",
    ),
    verb(
        "new-session",
        ResponseShape::NoOutput,
        "creates the agent session",
    ),
    verb(
        "new-window",
        ResponseShape::NoOutput,
        "creates additional windows within an agent session",
    ),
    verb(
        "attach-session",
        ResponseShape::RawPaneContent,
        "attaches the dashboard to an agent session",
    ),
    verb(
        "kill-session",
        ResponseShape::NoOutput,
        "terminates one agent session",
    ),
    verb(
        "kill-server",
        ResponseShape::NoOutput,
        "terminates the namespace during teardown and test cleanup",
    ),
    verb(
        "capture-pane",
        ResponseShape::RawPaneContent,
        "reads pane contents for the dashboard view",
    ),
    verb(
        "send-keys",
        ResponseShape::NoOutput,
        "delivers prompts and control keys to the agent",
    ),
    verb(
        "select-window",
        ResponseShape::NoOutput,
        "moves focus between windows",
    ),
    verb(
        "set-option",
        ResponseShape::NoOutput,
        "applies server and session options, including the exit-empty workaround",
    ),
    verb(
        "show-options",
        ResponseShape::SingleLine,
        "reads back an applied option to confirm it took effect",
    ),
    verb(
        "unbind-key",
        ResponseShape::NoOutput,
        "removes the psmux-only default root-table PageUp binding (#465)",
    ),
    // --- format strings ---
    format(
        "server_instance",
        true,
        ContractCapability::SincePsmuxNamespaceToken,
        "stable identity of the -L namespace; the only namespace-stable identity \
         available, minted by the first server and reported by all of them (psmux#509)",
    ),
    format(
        "pid",
        false,
        ContractCapability::Always,
        "PID of whichever per-session server answered the request. NOT the \
         namespace: psmux runs one server process per session, so this changes \
         when a session is merely added (#540)",
    ),
    format(
        "version",
        false,
        ContractCapability::Always,
        "multiplexer version, paired with the identity probe",
    ),
    format(
        "session_name",
        false,
        ContractCapability::Always,
        "maps a session back to its agent",
    ),
    format(
        "pane_dead",
        false,
        ContractCapability::Always,
        "reports whether a pane's process has exited",
    ),
    format(
        "pane_dead_signal",
        false,
        ContractCapability::Always,
        "distinguishes a signalled pane death from a clean exit",
    ),
    format(
        "pane_pid",
        false,
        ContractCapability::Always,
        "PID of the pane leader. On Windows this is the session host, not the \
         agent worker below it (#543)",
    ),
    format(
        "pane_index",
        false,
        ContractCapability::Always,
        "addresses a pane within its window",
    ),
    format(
        "window_index",
        false,
        ContractCapability::Always,
        "addresses a window within its session",
    ),
    format(
        "window_name",
        false,
        ContractCapability::Always,
        "identifies windows in the dashboard",
    ),
    format(
        "history_size",
        false,
        ContractCapability::Always,
        "sizes scrollback reads",
    ),
    format(
        "next_display_index",
        false,
        ContractCapability::Always,
        "allocates the next window display index",
    ),
    // --- server options ---
    server_option(
        "exit-empty",
        "psmux servers auto-exit when empty, which tmux does not do in our \
         configuration; disabling it keeps a namespace alive between sessions (#493)",
    ),
];

/// Return the full contract surface.
#[must_use]
pub fn contract_items() -> &'static [ContractItem] {
    CONTRACT
}

/// Look up one declared item by kind and name.
///
/// Returns `None` for anything not declared, which is what lets the mechanical
/// surface check treat an unknown verb or format as a policy violation rather
/// than silently trusting it.
#[must_use]
pub fn contract_item(kind: ContractItemKind, name: &str) -> Option<&'static ContractItem> {
    CONTRACT
        .iter()
        .find(|item| item.kind == kind && item.name == name)
}
