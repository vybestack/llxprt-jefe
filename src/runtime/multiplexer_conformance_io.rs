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

use std::process::{Command, Stdio};

use super::liveness::run_tmux_with_timeout;
use super::multiplexer_conformance::{
    ConformanceReport, ConformanceVerdict, MultiplexerQualification, ProbeOutcome, ProbePlan,
    classify_contract_probe, probe_ordered_items, probe_plan_for, qualification_from_report,
    summarize_conformance,
};
use super::multiplexer_contract::{ContractItem, contract_items};
use super::provenance::BinaryFingerprint;
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
    let findings: Vec<(&'static ContractItem, ConformanceVerdict)> = probe_ordered_items()
        .into_iter()
        .map(|item| {
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

/// Client variables a multiplexer uses to detect that it is being run from
/// inside one of its own panes.
///
/// jefe scrubs these everywhere else it invokes the multiplexer (#171). The
/// conformance runner did not, so when jefe itself started inside a pane the
/// probes inherited them and the multiplexer refused to nest -- reporting the
/// binary as non-conforming for correctly declining to create a session inside
/// itself. Qualification must ask the same question jefe will ask later, in the
/// same environment.
const NESTING_VARS_TO_SCRUB: &[&str] = &["TMUX", "TMUX_PANE", "TMUX_TMPDIR"];

/// Build the command one probe runs, with the nesting variables scrubbed.
fn probe_command(plan: &MultiplexerPlan, args: &[String]) -> Command {
    let mut command = plan.command();
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in NESTING_VARS_TO_SCRUB {
        command.env_remove(variable);
    }
    command
}

fn execute_probe(plan: &MultiplexerPlan, args: &[String]) -> ProbeOutcome {
    let mut command = probe_command(plan, args);

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

/// Fingerprint the binary being qualified, so a later check can tell whether it
/// changed underneath the running process (issue #540 V7).
///
/// A binary that cannot be read yields `None`; provenance then says nothing
/// rather than inventing a digest, which is the same rule the rest of the
/// qualification follows.
#[must_use]
pub fn fingerprint_multiplexer(plan: &MultiplexerPlan) -> Option<BinaryFingerprint> {
    BinaryFingerprint::measure(plan.executable()).ok()
}

#[cfg(test)]
mod tests {
    use super::{NESTING_VARS_TO_SCRUB, probe_command};
    use crate::runtime::MultiplexerPlan;

    /// jefe may itself be started from inside a multiplexer pane. A probe that
    /// inherited the client variables would make the multiplexer refuse to
    /// nest, and qualification would condemn a correct binary for correctly
    /// declining to create a session inside itself (#171, #540).
    #[test]
    fn probes_do_not_inherit_the_pane_jefe_was_launched_from() {
        let Ok(plan) = MultiplexerPlan::current() else {
            return;
        };
        let command = probe_command(&plan, &["list-sessions".to_owned()]);

        for variable in NESTING_VARS_TO_SCRUB {
            let removed = command
                .get_envs()
                .any(|(key, value)| key == std::ffi::OsStr::new(variable) && value.is_none());
            assert!(removed, "{variable} is not scrubbed from probe commands");
        }
    }
}
