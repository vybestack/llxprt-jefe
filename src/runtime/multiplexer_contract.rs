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
    /// A `#{Ã¢â‚¬Â¦}` format variable.
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

/// A behaviour where psmux differs from the tmux jefe was written against.
///
/// Each began as a patch at the point a symptom appeared. A patch records that
/// something was wrong; it does not record what jefe requires, so the next
/// divergence gets found the same way -- in production. Declaring them states
/// the expectation, cites what discovered it, and gives the remediation one
/// definition instead of a literal repeated wherever it was needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    pub name: &'static str,
    /// What jefe requires of the multiplexer.
    pub expectation: &'static str,
    /// What jefe does to obtain it.
    pub remediation: &'static str,
    /// The issue that discovered the divergence.
    pub discovered_by: &'static str,
}

static DIVERGENCES: &[Divergence] = &[
    Divergence {
        name: "exit-empty",
        expectation: "a namespace outlives its sessions: the server must not exit when its \
                      last session closes, because losing it loses the namespace identity \
                      bound to it",
        remediation: "set-option -s exit-empty off, once per observed server identity",
        discovered_by: "issue #493",
    },
    Divergence {
        name: "root-page-up-binding",
        expectation: "Page keys reach the child unintercepted; psmux ships a default \
                      root-table PageUp binding that tmux does not, and it consumes the key \
                      before the agent sees it",
        remediation: "unbind-key -T root PageUp during session setup",
        discovered_by: "issue #465",
    },
    Divergence {
        name: "reserved-prefix-key",
        expectation: "the prefix key can be released so keystrokes reach the agent \
                      untouched; psmux still reserves C-b when the prefix option is set to \
                      None, which tmux honours, so there is no way to have no prefix at all",
        remediation: "assign the prefix to jefe-owned F12 on Windows, which jefe intercepts \
                      before forwarding, rather than the None that psmux ignores",
        discovered_by: "issue #446, observed on psmux 3.3.6",
    },
    Divergence {
        name: "inherited-session-routing",
        expectation: "jefe must not appear nested inside a parent session; psmux exports \
                      session-routing variables that children inherit, and a jefe process \
                      inheriting them addresses the wrong session",
        remediation: "remove PSMUX_SESSION and PSMUX_TARGET_SESSION from every local command \
                      environment",
        discovered_by: "issue #260",
    },
];

/// Session-routing variables that must never be inherited.
///
/// `PSMUX_CLAUDE_TEAMMATE_MODE` and `PSMUX_CONFIG_FILE` are deliberately absent:
/// team mode is not session routing, and the plan's base args already carry
/// their own config selection.
pub const PSMUX_SESSION_ROUTING_VARS: [&str; 2] = ["PSMUX_SESSION", "PSMUX_TARGET_SESSION"];

/// The `exit-empty` remediation, as issued.
pub const EXIT_EMPTY_REMEDIATION: [&str; 4] = ["set-option", "-s", "exit-empty", "off"];

/// The root-table `PageUp` unbind, as issued.
pub const PAGE_UP_ROOT_UNBIND_COMMAND: [&str; 4] = ["unbind-key", "-T", "root", "PageUp"];

/// Every declared divergence.
#[must_use]
pub fn declared_divergences() -> &'static [Divergence] {
    DIVERGENCES
}

/// Look up one declared divergence.
#[must_use]
pub fn divergence(name: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|entry| entry.name == name)
}

/// Session-routing variables that must be scrubbed.
#[must_use]
pub const fn psmux_session_routing_vars() -> [&'static str; 2] {
    PSMUX_SESSION_ROUTING_VARS
}

/// The `exit-empty` remediation command.
#[must_use]
pub const fn exit_empty_remediation() -> [&'static str; 4] {
    EXIT_EMPTY_REMEDIATION
}

/// The root-table `PageUp` unbind command.
#[must_use]
pub const fn page_up_root_unbind() -> [&'static str; 4] {
    PAGE_UP_ROOT_UNBIND_COMMAND
}

/// What actually imposes the pane-command ceiling.
///
/// Naming this matters: jefe carried a limit attributed to the multiplexer
/// which measurement showed the multiplexer does not impose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// The multiplexer refuses the command itself, as tmux 3.x does.
    MultiplexerPaneCommand,
    /// The Windows `CreateProcess` command-line ceiling, reached through the
    /// PowerShell launch chain.
    WindowsCreateProcess,
    /// A POSIX shell/`ARG_MAX` ceiling.
    PosixShell,
}

/// A pane-command budget with the observation that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneCommandBudget {
    /// Usable bytes for the whole pane command.
    pub bytes: usize,
    /// What imposes the ceiling.
    pub source: BudgetSource,
    /// What it was measured against.
    pub measured_on: &'static str,
    /// The observation, so a reader can re-derive rather than trust it.
    pub evidence: &'static str,
}

#[cfg(windows)]
/// Measured evidence behind the Windows budget.
///
/// psmux exits 0 and creates the session whether or not the command survives,
/// so the boundary was found by having the command write its own payload and
/// comparing the bytes that arrived.
const WINDOWS_EVIDENCE: &str = "psmux fork 1a8b6d5 on Windows: via PowerShell (jefe's launch \
                                chain) 32,649 bytes delivered, 32,658 dropped; via cmd.exe \
                                8,164 delivered, 8,172 dropped. psmux reports success in both \
                                failing cases -- exit 0, session created, command never runs. \
                                The boundary tracks the shell, so the multiplexer imposes no \
                                pane-command limit of its own.";

#[cfg(not(windows))]
const POSIX_EVIDENCE: &str = "tmux 3.7b on macOS refuses a pane command at ~16,340 bytes, and \
                              reports the refusal. Retained for the tmux path only; the cmd.exe \
                              ceiling measured on Windows (8,164 delivered / 8,172 dropped) is \
                              narrower still, which is why the source is recorded alongside the \
                              number.";

/// The measured pane-command budget for this platform.
///
/// Windows is sized to the PowerShell chain jefe actually launches through,
/// held below the largest command observed to arrive intact.
#[must_use]
pub const fn pane_command_budget() -> PaneCommandBudget {
    #[cfg(windows)]
    {
        PaneCommandBudget {
            // Under the 32,649 observed to arrive, with margin for the
            // environment block CreateProcess counts against the same ceiling.
            bytes: 30_000,
            source: BudgetSource::WindowsCreateProcess,
            measured_on: "psmux fork 1a8b6d5, Windows PowerShell launch chain",
            evidence: WINDOWS_EVIDENCE,
        }
    }
    #[cfg(not(windows))]
    {
        PaneCommandBudget {
            bytes: 16_000,
            source: BudgetSource::MultiplexerPaneCommand,
            measured_on: "tmux 3.7b, macOS",
            evidence: POSIX_EVIDENCE,
        }
    }
}

/// The Windows prefix key, set by the `reserved-prefix-key` divergence.
///
/// `None` is the value that would mean "no prefix", but psmux ignores it and
/// keeps `C-b` reserved, so a key jefe owns is assigned instead.
pub const WINDOWS_RESERVED_PREFIX_REPLACEMENT: &str = "F12";

/// The prefix value for a platform, derived from the declared divergence.
#[must_use]
pub const fn prefix_value_for_platform(windows: bool) -> &'static str {
    if windows {
        WINDOWS_RESERVED_PREFIX_REPLACEMENT
    } else {
        "None"
    }
}
