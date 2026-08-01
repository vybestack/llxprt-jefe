//! Qualification against the psmux binary jefe will actually use (issue #540).
//!
//! The rest of the conformance tests drive the classifier with constructed
//! outcomes, which is why they all passed while the runner was incapable of
//! qualifying a real binary: the probes collided with their own scratch
//! session, tore down their own server midway, and judged output-free verbs by
//! their empty stdout.
//!
//! Requirement 2 asks for a suite that runs against the actual runtime binary.
//! That is this file, and nothing weaker substitutes for it.

use jefe::runtime::{MultiplexerPlan, MultiplexerQualification, qualify_multiplexer_for_startup};

/// Whether this environment promises a usable psmux.
///
/// CI sets `JEFE_REQUIRE_PSMUX` on the native Windows job. Where it is set, a
/// missing binary is a failure rather than a reason to skip -- a test that
/// quietly does nothing is how a broken runner survives a green build.
fn psmux_is_required() -> bool {
    std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1")
}

#[test]
fn the_multiplexer_jefe_will_use_qualifies() {
    // Only an environment that promises psmux can be held to psmux's contract.
    // Elsewhere -- a Linux coverage run, a developer box with plain tmux -- a
    // refusal is the honest answer rather than a defect, so asserting one would
    // fail the build for the runner behaving correctly.
    if !psmux_is_required() {
        return;
    }

    let plan = match MultiplexerPlan::current() {
        Ok(plan) => plan,
        Err(error) => {
            panic!("JEFE_REQUIRE_PSMUX is set but no multiplexer could be resolved: {error}");
        }
    };

    match qualify_multiplexer_for_startup(&plan) {
        MultiplexerQualification::Qualified { .. } => {}
        MultiplexerQualification::Refused { message } => {
            panic!(
                "the binary jefe is configured to use failed its own contract.\n\
                 A refusal here means either psmux regressed or the conformance \
                 runner is wrong; the runner has been wrong before.\n\n{message}"
            );
        }
    }
}

/// Qualification must be repeatable. The runner creates and destroys sessions,
/// so a first run that leaves state behind would make a second run disagree --
/// which is exactly the collision that made the original runner refuse every
/// binary after its own setup.
#[test]
fn qualifying_twice_gives_the_same_answer() {
    if !psmux_is_required() {
        return;
    }
    let Ok(plan) = MultiplexerPlan::current() else {
        return;
    };

    let first = qualify_multiplexer_for_startup(&plan);
    let second = qualify_multiplexer_for_startup(&plan);

    let describe = |qualification: &MultiplexerQualification| match qualification {
        MultiplexerQualification::Qualified { .. } => "qualified".to_owned(),
        MultiplexerQualification::Refused { message } => format!("refused: {message}"),
    };

    assert_eq!(
        describe(&first),
        describe(&second),
        "qualification left state behind that changed the second answer"
    );
}
