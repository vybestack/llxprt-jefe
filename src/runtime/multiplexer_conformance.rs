//! Classifying a multiplexer's answers against the declared contract
//! (issue #540).
//!
//! Deliberately free of I/O: probes are executed at the boundary and the
//! results classified here, so every verdict is reachable in a test without a
//! live binary.
//!
//! The distinction this module exists to preserve is between *absent* and
//! *wrong*. A psmux predating upstream#509 renders `#{server_instance}` as
//! empty text and still exits zero. Treating that as a violation would reject
//! every released build; treating a required format's empty answer as an
//! absence would let jefe run against something that is not the multiplexer it
//! needs. Both mistakes are silent, so the difference is encoded in types.

use std::fmt::Write as _;
use std::path::Path;

use super::multiplexer_contract::{
    ContractCapability, ContractItem, ContractItemKind, ResponseShape,
};

/// What a probe of the multiplexer produced.
///
/// `exit_code` is `None` when the process could not be run or was terminated by
/// a signal, which is never evidence about an optional capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// The verdict for one contract item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConformanceVerdict {
    /// The multiplexer honours the item.
    Satisfied,
    /// A capability-gated item this build predates. Qualification still passes;
    /// callers must degrade rather than assume the item is present.
    Unsupported,
    /// The multiplexer failed to honour a required item.
    Violated { detail: String },
}

/// One item's outcome, retained for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceFinding {
    pub kind: ContractItemKind,
    pub name: &'static str,
    pub verdict: ConformanceVerdict,
}

/// The result of probing a binary against the whole contract surface.
///
/// The default is an empty report: nothing probed, so nothing found. It is not
/// a passing result, and `is_qualified` treats it as such only because an
/// absence of violations is all that can be said about an absence of probes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConformanceReport {
    findings: Vec<ConformanceFinding>,
}

impl ConformanceReport {
    /// Whether the binary may be used at all.
    ///
    /// Unsupported optional items do not disqualify a build; they reduce what
    /// jefe may rely on.
    #[must_use]
    pub fn is_qualified(&self) -> bool {
        self.violations().is_empty()
    }

    /// Every item the binary failed to honour, for an actionable refusal.
    #[must_use]
    pub fn violations(&self) -> Vec<&ConformanceFinding> {
        self.findings
            .iter()
            .filter(|finding| matches!(finding.verdict, ConformanceVerdict::Violated { .. }))
            .collect()
    }

    /// Whether a named item is actually available on this binary.
    ///
    /// An item that was never probed is reported as unavailable rather than
    /// assumed present, so a caller cannot depend on evidence not gathered.
    #[must_use]
    pub fn supports(&self, name: &str) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.name == name && finding.verdict == ConformanceVerdict::Satisfied)
    }

    #[must_use]
    pub fn findings(&self) -> &[ConformanceFinding] {
        &self.findings
    }
}

/// How an item is to be probed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbePlan {
    /// Arguments to run against the throwaway conformance namespace.
    Command { args: Vec<String> },
    /// Needs a controlling terminal, so it cannot be exercised
    /// non-interactively. Its availability is established when the dashboard
    /// attaches rather than by a probe that would block forever.
    RequiresTerminal,
}

/// Decide how to probe one item inside a throwaway namespace.
///
/// Commands target `scratch_session`, the session the runner brings up and
/// leaves standing for the duration of the check. The lifecycle verbs are the
/// exception: `new-session` and `kill-session` address `disposable_session`
/// instead, so creating one does not collide with the session already standing
/// and destroying one does not pull the floor out from under later probes.
///
/// Namespace isolation keeps these verbs away from the caller's agents. It does
/// not make them harmless to the run itself, which is a separate problem and
/// was originally conflated with the first.
#[must_use]
pub fn probe_plan_for(
    item: &ContractItem,
    scratch_session: &str,
    disposable_session: &str,
) -> ProbePlan {
    match item.kind {
        ContractItemKind::Format => ProbePlan::Command {
            args: vec![
                "display-message".to_owned(),
                "-p".to_owned(),
                "-t".to_owned(),
                scratch_session.to_owned(),
                format!("#{{{}}}", item.name),
            ],
        },
        ContractItemKind::ServerOption => ProbePlan::Command {
            args: vec![
                "show-options".to_owned(),
                "-s".to_owned(),
                item.name.to_owned(),
            ],
        },
        ContractItemKind::Verb => verb_probe_plan(item.name, scratch_session, disposable_session),
    }
}

fn verb_probe_plan(name: &str, scratch_session: &str, disposable_session: &str) -> ProbePlan {
    let target = || vec!["-t".to_owned(), scratch_session.to_owned()];
    let with = |verb: &str, rest: Vec<String>| {
        let mut args = vec![verb.to_owned()];
        args.extend(rest);
        ProbePlan::Command { args }
    };

    match name {
        // Attaching needs a TTY and would block a non-interactive probe.
        "attach-session" => ProbePlan::RequiresTerminal,
        "has-session" => with("has-session", target()),
        "list-sessions" => with("list-sessions", vec![]),
        "list-panes" => with("list-panes", target()),
        "list-windows" => with("list-windows", target()),
        "display-message" => with(
            "display-message",
            vec![
                "-p".to_owned(),
                "-t".to_owned(),
                scratch_session.to_owned(),
                "#{session_name}".to_owned(),
            ],
        ),
        "capture-pane" => with(
            "capture-pane",
            vec!["-p".to_owned(), "-t".to_owned(), scratch_session.to_owned()],
        ),
        "show-options" => with("show-options", vec!["-s".to_owned()]),
        "set-option" => with(
            "set-option",
            vec!["-s".to_owned(), "exit-empty".to_owned(), "off".to_owned()],
        ),
        "unbind-key" => with("unbind-key", vec!["-T".to_owned(), "root".to_owned()]),
        "select-window" => with("select-window", target()),
        // Targeted like every other session-scoped verb. Untargeted it lands in
        // whatever session the multiplexer considers current, which during the
        // run is whichever session was touched last rather than a session the
        // probe chose.
        "new-window" => with(
            "new-window",
            vec!["-d".to_owned(), "-t".to_owned(), scratch_session.to_owned()],
        ),
        // An empty literal key sequence: exercises the verb without typing
        // anything into the pane.
        "send-keys" => with(
            "send-keys",
            vec![
                "-t".to_owned(),
                scratch_session.to_owned(),
                "-l".to_owned(),
                String::new(),
            ],
        ),
        // Kills the session `new-session` just made, never the one the rest of
        // the probes depend on.
        "kill-session" => with(
            "kill-session",
            vec!["-t".to_owned(), disposable_session.to_owned()],
        ),
        // Ends the namespace outright, so the runner probes it last.
        "kill-server" => with("kill-server", vec![]),
        "new-session" => with(
            "new-session",
            vec![
                "-d".to_owned(),
                "-s".to_owned(),
                disposable_session.to_owned(),
            ],
        ),
        other => with(other, vec![]),
    }
}

/// When an item may be probed, relative to the others.
///
/// Probing is otherwise declaration-ordered. These verbs dismantle the very
/// environment the remaining probes need, so running them in place made every
/// later item fail with "no server running" and reported twenty violations for
/// one ordering mistake.
#[must_use]
pub fn probe_rank(item: &ContractItem) -> u8 {
    match item.name {
        // Depends on the session `new-session` created.
        "kill-session" => 1,
        // Ends the namespace; nothing can be probed afterwards.
        "kill-server" => 2,
        _ => 0,
    }
}

/// The contract surface in the order it is safe to probe.
///
/// A stable sort, so declaration order still governs everything that has no
/// ordering constraint of its own.
#[must_use]
pub fn probe_ordered_items() -> Vec<&'static ContractItem> {
    let mut items: Vec<&'static ContractItem> = super::multiplexer_contract::contract_items()
        .iter()
        .collect();
    items.sort_by_key(|item| probe_rank(item));
    items
}

/// Classify one probe against the item it was gathered for.
#[must_use]
pub fn classify_contract_probe(item: &ContractItem, outcome: &ProbeOutcome) -> ConformanceVerdict {
    let Some(exit_code) = outcome.exit_code else {
        return ConformanceVerdict::Violated {
            detail: format!(
                "the multiplexer could not be executed while probing `{}`: {}",
                item.name,
                describe_stderr(&outcome.stderr),
            ),
        };
    };

    if mentions_unknown_command(&outcome.stderr) {
        return ConformanceVerdict::Violated {
            detail: format!(
                "the multiplexer does not recognise `{}`: {}",
                item.name,
                describe_stderr(&outcome.stderr),
            ),
        };
    }

    match item.response {
        // The exit status *is* the answer, so any status the binary understood
        // is a legitimate one. `has-session` returning non-zero means the
        // session is absent, not that the verb is unsupported.
        ResponseShape::ExitStatusOnly => ConformanceVerdict::Satisfied,
        // A verb declared to produce no output is judged by whether it
        // succeeded. Requiring stdout from `kill-session` or `new-window`
        // condemns every correct implementation for behaving as declared.
        //
        // Pane content is the same case for a different reason: what
        // `capture-pane` prints belongs to the pane, not to the multiplexer. A
        // freshly created pane has produced nothing yet, so demanding output
        // tests how fast a shell draws its prompt rather than whether the verb
        // works.
        ResponseShape::NoOutput | ResponseShape::RawPaneContent => {
            classify_silent(item, exit_code, outcome)
        }
        _ => classify_output(item, exit_code, outcome),
    }
}

fn classify_silent(
    item: &ContractItem,
    exit_code: i32,
    outcome: &ProbeOutcome,
) -> ConformanceVerdict {
    if exit_code == 0 {
        return ConformanceVerdict::Satisfied;
    }
    ConformanceVerdict::Violated {
        detail: format!(
            "probing `{}` exited {exit_code}: {}",
            item.name,
            describe_stderr(&outcome.stderr),
        ),
    }
}

fn classify_output(
    item: &ContractItem,
    exit_code: i32,
    outcome: &ProbeOutcome,
) -> ConformanceVerdict {
    if exit_code != 0 {
        return ConformanceVerdict::Violated {
            detail: format!(
                "probing `{}` exited {exit_code}: {}",
                item.name,
                describe_stderr(&outcome.stderr),
            ),
        };
    }

    if !outcome.stdout.trim().is_empty() {
        return ConformanceVerdict::Satisfied;
    }

    // An empty render is the documented way an older multiplexer reports an
    // unknown format variable: it substitutes nothing and still succeeds.
    match item.capability {
        ContractCapability::SincePsmuxNamespaceToken => ConformanceVerdict::Unsupported,
        ContractCapability::Always => ConformanceVerdict::Violated {
            detail: format!(
                "`{}` is required but the multiplexer rendered it as empty",
                item.name,
            ),
        },
    }
}

/// Collect classified items into a report.
#[must_use]
pub fn summarize_conformance(
    findings: impl IntoIterator<Item = (&'static ContractItem, ConformanceVerdict)>,
) -> ConformanceReport {
    ConformanceReport {
        findings: findings
            .into_iter()
            .map(|(item, verdict)| ConformanceFinding {
                kind: item.kind,
                name: item.name,
                verdict,
            })
            .collect(),
    }
}

fn mentions_unknown_command(stderr: &str) -> bool {
    let lowered = stderr.to_ascii_lowercase();
    lowered.contains("unknown command") || lowered.contains("ambiguous command")
}

fn describe_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        "no diagnostic output".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The outcome of qualifying a multiplexer binary at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiplexerQualification {
    /// The binary honours every required item. The report is retained so
    /// callers can degrade against capabilities it lacks.
    Qualified { report: ConformanceReport },
    /// The binary may not be used, with the operator-facing explanation.
    Refused { message: String },
}

/// Environment variable that points jefe at a specific multiplexer build.
const OVERRIDE_VARIABLE: &str = "JEFE_PSMUX_BIN";

/// Turn a conformance report into a startup decision.
///
/// jefe does not ship or install psmux, so when it refuses to run the message
/// is the entire remedy the operator receives. It therefore names the binary
/// rejected, the version found there, every failing requirement, and how to
/// supply a working build.
#[must_use]
pub fn qualification_from_report(
    executable: &Path,
    version: Option<String>,
    report: ConformanceReport,
) -> MultiplexerQualification {
    if report.is_qualified() {
        return MultiplexerQualification::Qualified { report };
    }

    let mut message = format!(
        "the multiplexer at {} does not meet jefe's requirements.\n  version found: {}\n",
        executable.display(),
        version.unwrap_or_else(|| "unknown (the binary did not report one)".to_owned()),
    );

    message.push_str("  failing requirements:\n");
    for violation in report.violations() {
        let kind = match violation.kind {
            ContractItemKind::Verb => "command",
            ContractItemKind::Format => "format",
            ContractItemKind::ServerOption => "server option",
        };
        let detail = match &violation.verdict {
            ConformanceVerdict::Violated { detail } => detail.as_str(),
            // Only violations are listed: a capability the build merely
            // predates is not a defect, and naming it would send the operator
            // after a fault that does not exist.
            _ => continue,
        };
        let _ = writeln!(message, "    - {kind} `{}`: {detail}", violation.name);
    }

    let _ = write!(
        message,
        "  remedy: install a psmux build that satisfies the requirements above, \
         or set {OVERRIDE_VARIABLE} to the full path of one.",
    );

    MultiplexerQualification::Refused { message }
}
