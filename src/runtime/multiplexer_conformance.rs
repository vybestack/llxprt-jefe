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
#[derive(Debug, Clone, PartialEq, Eq)]
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
/// Every returned command targets `scratch_session` explicitly. The runner
/// creates and destroys its own namespace, so verbs that would be destructive
/// against live agent state — `kill-session`, `kill-server`, `send-keys` — are
/// safe here precisely because they never address the caller's namespace.
#[must_use]
pub fn probe_plan_for(item: &ContractItem, scratch_session: &str) -> ProbePlan {
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
        ContractItemKind::Verb => verb_probe_plan(item.name, scratch_session),
    }
}

fn verb_probe_plan(name: &str, scratch_session: &str) -> ProbePlan {
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
        "new-window" => with("new-window", vec!["-d".to_owned()]),
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
        // Safe only because the runner owns this namespace outright.
        "kill-session" => with("kill-session", target()),
        "kill-server" => with("kill-server", vec![]),
        "new-session" => with(
            "new-session",
            vec!["-d".to_owned(), "-s".to_owned(), scratch_session.to_owned()],
        ),
        other => with(other, vec![]),
    }
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
        _ => classify_output(item, exit_code, outcome),
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
