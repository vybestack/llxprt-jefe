//! Measured pane-command budget (issue #540 slice S4/V4).
//!
//! jefe sized its prompt limits against tmux 3.7b on macOS, where the
//! multiplexer itself refuses a pane command over ~16,340 bytes. That number
//! describes neither the multiplexer nor the platform jefe now runs on.
//!
//! Measured against psmux (fork build 1a8b6d5) on Windows:
//!
//! | path                        | largest delivered | smallest dropped |
//! |-----------------------------|-------------------|------------------|
//! | pwsh (jefe's launch chain)  | 32,649            | 32,658           |
//! | cmd.exe                     | 8,164             | 8,172            |
//!
//! Two things follow. psmux imposes no limit of its own -- the boundary tracks
//! the shell's command-line ceiling, not the multiplexer. And the failure is
//! silent: psmux exits 0, the session is created, and the command never runs.
//! A budget that is merely documented cannot catch that, so it is declared with
//! its provenance and asserted against the prompt limits that depend on it.

use jefe::runtime::pane_command_budget;
// Every assertion naming a specific source is Windows-only, because that is
// where the budget was measured.
#[cfg(windows)]
use jefe::runtime::BudgetSource;
use jefe::runtime::{LocalPlatform, MultiplexerIsolation, MultiplexerPlan};
use std::ffi::OsString;
use std::path::PathBuf;

/// The native plan, so the budget under test is the one this host measures.
fn native_plan() -> MultiplexerPlan {
    #[cfg(windows)]
    let (platform, executable, isolation) = (
        LocalPlatform::Windows,
        PathBuf::from("psmux.exe"),
        MultiplexerIsolation::Namespace("jefe-budget-probe".to_owned()),
    );
    #[cfg(not(windows))]
    let (platform, executable, isolation) = (
        LocalPlatform::Unix,
        PathBuf::from("tmux"),
        MultiplexerIsolation::Socket(PathBuf::from("/tmp/jefe-budget-probe.sock")),
    );

    match MultiplexerPlan::for_platform(platform, executable, isolation) {
        Ok(plan) => plan,
        Err(error) => panic!("the native plan must be constructible: {error}"),
    }
}

/// The failure this guards is the one the evidence above records: psmux exits
/// 0, creates the session, and never runs a command that overran the shell's
/// ceiling. A budget nothing checks cannot catch that, so an over-budget pane
/// command must be refused here rather than discovered as a pane that silently
/// does nothing.
#[test]
fn an_over_budget_pane_command_is_refused_rather_than_silently_dropped() {
    let budget = pane_command_budget();
    let oversized = OsString::from("x".repeat(budget.bytes + 1));

    let Err(error) = native_plan().pane_command_args(
        &OsString::from("agent"),
        std::slice::from_ref(&oversized),
        &[],
    ) else {
        panic!(
            "a pane command over the {}-byte budget must be refused, not built",
            budget.bytes
        );
    };

    let rendered = error.to_string();
    assert!(
        rendered.contains("pane-command"),
        "the refusal must name the gate that produced it: {rendered}",
    );
    assert!(
        rendered.contains(&budget.bytes.to_string()),
        "the refusal must state the budget it exceeded: {rendered}",
    );
    assert!(
        rendered.contains("remediation:"),
        "the refusal must tell the user what to do: {rendered}",
    );
}

/// The check must bound the command actually issued, so a launch that fits
/// keeps working unchanged on every platform.
#[test]
fn a_pane_command_within_the_budget_is_still_built() {
    let budget = pane_command_budget();
    let sized = OsString::from("x".repeat(budget.bytes / 2));

    let built = native_plan().pane_command_args(
        &OsString::from("agent"),
        std::slice::from_ref(&sized),
        &[],
    );

    assert!(
        built.is_ok(),
        "a command inside the budget must still build: {:?}",
        built.err().map(|error| error.to_string()),
    );
}

/// A budget with no recorded provenance is the failure being corrected: a
/// number nobody can re-derive, attributed to the wrong component.
#[test]
fn the_budget_records_where_it_came_from() {
    let budget = pane_command_budget();

    assert!(budget.bytes > 0);
    assert!(
        !budget.measured_on.is_empty(),
        "the budget must say what it was measured against",
    );
    assert!(
        !budget.evidence.is_empty(),
        "the budget must carry the observation that produced it",
    );
}

/// The constraint is the shell's command line, not the multiplexer. Attributing
/// it to the multiplexer is what produced a tmux number on a psmux system.
///
/// Windows-only, because that is where the claim was measured and where it
/// holds: psmux imposes no pane-command limit of its own. tmux does, so on
/// other platforms attributing the budget to the multiplexer is the correct
/// answer rather than the inherited mistake.
#[cfg(windows)]
#[test]
fn the_budget_is_not_attributed_to_the_multiplexer() {
    let budget = pane_command_budget();

    assert_ne!(
        budget.source,
        BudgetSource::MultiplexerPaneCommand,
        "psmux imposes no pane-command limit; measurement showed the boundary \
         tracking the shell's ceiling instead",
    );
}

/// On Windows the launch chain runs through PowerShell, whose ceiling is the
/// CreateProcess command-line limit -- roughly twice the tmux figure jefe was
/// sized against, and reached silently.
#[cfg(windows)]
#[test]
fn the_windows_budget_reflects_the_measured_powershell_ceiling() {
    let budget = pane_command_budget();

    assert_eq!(budget.source, BudgetSource::WindowsCreateProcess);
    assert!(
        budget.bytes <= 32_649,
        "the budget must not exceed the largest command observed to arrive \
         intact (32,649), got {}",
        budget.bytes,
    );
    assert!(
        budget.bytes >= 16_000,
        "sizing Windows to tmux's ~16 KB would keep the inherited number, \
         got {}",
        budget.bytes,
    );
}

/// The cmd.exe ceiling is far below the pwsh one and below the current
/// compaction threshold. Recording it keeps the difference visible rather than
/// leaving it to be rediscovered by a silent non-launch.
#[test]
fn the_narrower_shell_ceiling_is_recorded() {
    let budget = pane_command_budget();

    assert!(
        budget.evidence.contains("8,1") || budget.evidence.contains("8164"),
        "the cmd.exe boundary belongs in the evidence: {}",
        budget.evidence,
    );
}
