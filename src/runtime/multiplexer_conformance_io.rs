//! Executing the declared contract against a real multiplexer binary
//! (issue #540).
//!
//! The runner owns a throwaway `-L` namespace for the duration of the check and
//! destroys it afterwards. That isolation is what makes probing otherwise
//! destructive verbs safe: `kill-session` and `kill-server` are exercised
//! against sessions the runner created, never against the caller's agents.

use std::process::Stdio;

use super::liveness::run_tmux_with_timeout;
use super::multiplexer_conformance::{
    ConformanceReport, ConformanceVerdict, MultiplexerQualification, ProbeOutcome, ProbePlan,
    classify_contract_probe, probe_plan_for, qualification_from_report, summarize_conformance,
};
use super::multiplexer_contract::{ContractItem, contract_items};
use super::{MultiplexerIsolation, MultiplexerPlan};

/// Session created inside the throwaway namespace for session-addressed probes.
const SCRATCH_SESSION: &str = "jefe-conformance";

/// Probe `plan`'s binary against the whole declared contract surface.
///
/// `plan` must already address an isolated namespace reserved for this check;
/// [`qualify_multiplexer`] is the entry point that arranges that.
fn run_contract_probes(plan: &MultiplexerPlan) -> ConformanceReport {
    let findings: Vec<(&'static ContractItem, ConformanceVerdict)> = contract_items()
        .iter()
        .map(|item| {
            let verdict = match probe_plan_for(item, SCRATCH_SESSION) {
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
/// tear down each other's scratch server mid-probe.
fn conformance_namespace() -> String {
    format!("jefe-conformance-{}", std::process::id())
}
