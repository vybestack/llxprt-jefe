//! Executing the declared contract against a real multiplexer binary
//! (issue #540).
//!
//! The runner owns a throwaway `-L` namespace for the duration of the check and
//! destroys it afterwards. That isolation keeps `kill-session` and `kill-server`
//! away from the caller's agents.
//!
//! Isolation alone is not enough, and originally this module claimed it was.
//! Those verbs are equally destructive to the run itself: `kill-server` ends the
//! namespace every later probe depends on, and `new-session` collides with the
//! session the runner has already brought up. Both are handled explicitly --
//! lifecycle verbs address their own disposable session, and are probed last.

use std::collections::BTreeMap;
use std::process::Stdio;

use super::liveness::run_tmux_with_timeout;
use super::multiplexer_conformance::{
    ConformanceReport, ConformanceVerdict, MultiplexerQualification, ProbeOutcome, ProbePlan,
    classify_contract_probe, probe_ordered_items, probe_plan_for, qualification_from_report,
    summarize_conformance,
};
use super::multiplexer_contract::{ContractItem, ContractItemKind, contract_items};
use super::{MultiplexerIsolation, MultiplexerPlan};

/// Session created inside the throwaway namespace for session-addressed probes.
const SCRATCH_SESSION: &str = "jefe-conformance";

/// The session the lifecycle verbs create and destroy.
///
/// Distinct from [`SCRATCH_SESSION`] so that probing `new-session` does not
/// collide with the session the runner already brought up, and probing
/// `kill-session` does not remove the one later probes still need.
const DISPOSABLE_SESSION: &str = "jefe-conformance-disposable";

/// Probe `plan`'s binary against the whole declared contract surface.
///
/// `plan` must already address an isolated namespace reserved for this check;
/// [`qualify_multiplexer`] is the entry point that arranges that.
fn run_contract_probes(plan: &MultiplexerPlan) -> ConformanceReport {
    let items = probe_ordered_items();
    let batched = batched_format_outcomes(plan, &items);

    let findings: Vec<(&'static ContractItem, ConformanceVerdict)> = items
        .into_iter()
        .map(|item| {
            if let Some(outcome) = batched.get(item.name) {
                return (item, classify_contract_probe(item, outcome));
            }
            let verdict = match probe_plan_for(item, SCRATCH_SESSION, DISPOSABLE_SESSION) {
                // Attaching needs a controlling terminal. Reporting it as
                // satisfied would be a claim no probe supports, so it is
                // recorded as unverified capability instead.
                ProbePlan::RequiresTerminal => ConformanceVerdict::Unsupported,
                ProbePlan::Command { args } => {
                    let outcome = execute_probe(plan, &args);
                    classify_contract_probe(item, &outcome)
                }
            };
            (item, verdict)
        })
        .collect();

    summarize_conformance(findings)
}

/// Separator between batched format values.
///
/// A format string passes literals through untouched, so this arrives back
/// verbatim between the expanded values.
const FORMAT_BATCH_SEPARATOR: &str = "@|@";

/// Read every declared format in one call instead of one process per variable.
///
/// Startup previously spawned a process per format. On Windows a spawn is the
/// expensive part -- far more than the work the multiplexer does once running --
/// so a dozen of them delayed the first frame by seconds for a check whose
/// result is only ever a warning.
///
/// Batching is only sound if it cannot weaken a verdict, so anything unexpected
/// yields an empty map and every format is probed individually as before: a
/// non-zero exit, or a reply that does not split into exactly the values asked
/// for. An *unsupported* format is not unexpected -- it expands to nothing and
/// arrives as an empty field, which is precisely what the individual probe
/// would have seen, so it still classifies as a violation.
fn batched_format_outcomes(
    plan: &MultiplexerPlan,
    items: &[&'static ContractItem],
) -> BTreeMap<&'static str, ProbeOutcome> {
    let formats: Vec<&'static ContractItem> = items
        .iter()
        .copied()
        .filter(|item| matches!(item.kind, ContractItemKind::Format))
        .collect();
    // One format is already one call; batching would only add a parsing step.
    if formats.len() < 2 {
        return BTreeMap::new();
    }

    let combined = formats
        .iter()
        .map(|item| format!("#{{{}}}", item.name))
        .collect::<Vec<_>>()
        .join(FORMAT_BATCH_SEPARATOR);

    let outcome = execute_probe(
        plan,
        &[
            "display-message".to_owned(),
            "-p".to_owned(),
            "-t".to_owned(),
            SCRATCH_SESSION.to_owned(),
            combined,
        ],
    );

    if outcome.exit_code != Some(0) {
        return BTreeMap::new();
    }
    let reply = outcome.stdout.trim_end();
    let values: Vec<&str> = reply.split(FORMAT_BATCH_SEPARATOR).collect();
    if values.len() != formats.len() {
        return BTreeMap::new();
    }

    formats
        .iter()
        .zip(values)
        .map(|(item, value)| {
            (
                item.name,
                ProbeOutcome {
                    exit_code: outcome.exit_code,
                    stdout: value.to_owned(),
                    stderr: String::new(),
                },
            )
        })
        .collect()
}

fn execute_probe(plan: &MultiplexerPlan, args: &[String]) -> ProbeOutcome {
    let mut command = plan.command();
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    match run_tmux_with_timeout(&mut command) {
        Ok(output) => ProbeOutcome {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(()) => ProbeOutcome {
            exit_code: None,
            stdout: String::new(),
            stderr: "the multiplexer did not answer within the probe timeout".to_owned(),
        },
    }
}

/// Qualify a multiplexer binary against the declared contract.
///
/// Creates a throwaway namespace, probes it, and tears it down. The teardown is
/// best-effort and unconditional: leaving a stray server behind would leak the
/// very kind of orphan the runtime spends effort reaping.
#[must_use]
pub fn qualify_multiplexer(plan: &MultiplexerPlan) -> ConformanceReport {
    let Some(scratch) = scratch_plan(plan) else {
        return summarize_conformance(contract_items().iter().map(|item| {
            (
                item,
                ConformanceVerdict::Violated {
                    detail: "could not reserve an isolated namespace to probe in".to_owned(),
                },
            )
        }));
    };

    // Bring the namespace up before probing. A failure here is not fatal on its
    // own: the probes will report precisely which items the binary could not
    // honour, which is a better diagnosis than "setup failed".
    let _ = execute_probe(
        &scratch,
        &[
            "new-session".to_owned(),
            "-d".to_owned(),
            "-s".to_owned(),
            SCRATCH_SESSION.to_owned(),
        ],
    );

    let report = run_contract_probes(&scratch);

    let _ = execute_probe(&scratch, &["kill-server".to_owned()]);

    report
}

/// Qualify the multiplexer jefe is about to use, at startup.
///
/// Returns the report when the binary may be used, or an operator-facing
/// refusal naming the binary, the version found, every failing requirement and
/// the remedy. jefe neither ships nor installs psmux, so this message is the
/// whole of what an operator gets to act on.
///
/// The probe runs against the resolved binary, which honours the
/// `JEFE_PSMUX_BIN` override: pointing jefe at a specific build changes which
/// binary is qualified, never whether qualification applies.
#[must_use]
pub fn qualify_multiplexer_for_startup(plan: &MultiplexerPlan) -> MultiplexerQualification {
    let report = qualify_multiplexer(plan);
    qualification_from_report(plan.executable(), probe_version(plan), report)
}

/// Ask the binary for its version, for the refusal message.
///
/// A binary too broken to answer yields `None`, which the refusal reports as
/// unknown rather than implying a version it never observed.
fn probe_version(plan: &MultiplexerPlan) -> Option<String> {
    let outcome = execute_probe(plan, &["-V".to_owned()]);
    let reported = outcome.stdout.trim();
    if outcome.exit_code == Some(0) && !reported.is_empty() {
        Some(reported.to_owned())
    } else {
        None
    }
}

/// Derive a plan addressing isolation reserved for conformance probing.
///
/// The scratch isolation matches the kind the caller uses, so the probe
/// exercises the same addressing mode production traffic will.
fn scratch_plan(plan: &MultiplexerPlan) -> Option<MultiplexerPlan> {
    let name = conformance_namespace();
    let isolation = match plan.isolation() {
        MultiplexerIsolation::Namespace(_) => MultiplexerIsolation::Namespace(name),
        MultiplexerIsolation::Socket(_) => {
            MultiplexerIsolation::Socket(std::env::temp_dir().join(name))
        }
    };
    plan.with_isolation(isolation).ok()
}

/// Namespace name for the throwaway server.
///
/// Includes this process's PID so two jefe processes qualifying at once cannot
/// tear down each other's scratch server mid-probe, and a per-invocation
/// counter so two qualifications *within* one process cannot either. The PID
/// alone was not enough: the teardown of one run killed the server another was
/// still probing, which reads as the binary failing rather than the runner
/// colliding with itself.
fn conformance_namespace() -> String {
    static INVOCATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let invocation = INVOCATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("jefe-conformance-{}-{invocation}", std::process::id())
}
