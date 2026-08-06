//! Behavioral coverage for the multiplexer server-health probe classifier.
//!
//! The classifier ran untested in production until issue #664: a `Replaced`
//! verdict was emitted for a server whose creation discriminator was 133
//! seconds *older* than the server it supposedly replaced. These tests pin the
//! classification of every observation the probe can produce, starting with
//! the `Replaced` branch and the conflicting-identity guard that now bounds it.

use super::server_health::{
    ServerIdentity, ServerInstanceToken, ServerLivenessEvidence, ServerLivenessObservation,
    parse_server_identity_output,
};
use super::server_health_io::{
    classify_observation, classify_resolved_identity, resolve_observed_identity,
};
use crate::domain::{ProcessIdentity, ServerProcessIdentity};
use crate::runtime::MultiplexerVersion;

fn identity(pid: u32, started_at: u64) -> ServerIdentity {
    ServerIdentity::new(
        ServerProcessIdentity::new(pid, started_at),
        MultiplexerVersion::new(3, 3, 7),
    )
}

fn identity_without_start(pid: u32) -> ServerIdentity {
    ServerIdentity::new(
        ServerProcessIdentity::from_pid(pid),
        MultiplexerVersion::new(3, 3, 7),
    )
}

/// A different PID whose creation discriminator is strictly newer than the
/// pinned server's is a genuine restart, so it still classifies as `Replaced`.
#[test]
fn strictly_newer_identity_is_replaced() {
    let prior = identity(656, 134_304_226_880_092_839);
    let current = identity(19948, 134_304_228_211_297_590);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Replaced(current)
    );
}

/// The exact production observation from issue #664: the "replacement" server
/// (pid 19948) started 133 seconds *before* the server it supposedly replaced
/// (pid 656). A replacement cannot predate the thing it replaced, so this is
/// reported as a conflicting identity rather than accepted as a restart.
#[test]
fn older_identity_is_conflicting_rather_than_replaced() {
    let prior = identity(656, 134_304_228_211_297_590);
    let current = identity(19948, 134_304_226_880_092_839);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::ConflictingIdentity(current)
    );
}

/// Two servers created in the same tick are not ordered, so neither can be
/// shown to have replaced the other. "Strictly newer" excludes equality.
#[test]
fn equally_aged_identity_is_conflicting() {
    let prior = identity(656, 134_304_228_211_297_590);
    let current = identity(19948, 134_304_228_211_297_590);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::ConflictingIdentity(current)
    );
}

/// A missing creation discriminator on the pinned server makes the ordering
/// unverifiable, not conflicting. The guard fails open to the pre-existing
/// `Replaced` behavior so weaker evidence never manufactures a new failure.
#[test]
fn unverifiable_prior_start_falls_open_to_replaced() {
    let prior = identity_without_start(656);
    let current = identity(19948, 134_304_226_880_092_839);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Replaced(current)
    );
}

/// A missing creation discriminator on the observed server is equally
/// unverifiable and equally fails open.
#[test]
fn unverifiable_current_start_falls_open_to_replaced() {
    let prior = identity(656, 134_304_228_211_297_590);
    let current = identity_without_start(19948);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Replaced(current)
    );
}

/// The same server answering again is healthy, and the guard does not disturb
/// that: identical PID and creation discriminator never reach the ordering
/// check.
#[test]
fn same_identity_is_healthy() {
    let prior = identity(656, 134_304_228_211_297_590);
    let current = identity(656, 134_304_228_211_297_590);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Healthy(Some(current))
    );
}

/// With nothing pinned there is no prior to order against, so the first
/// observation of a server is simply healthy.
#[test]
fn first_observation_without_prior_is_healthy() {
    let current = identity(656, 134_304_228_211_297_590);

    assert_eq!(
        classify_resolved_identity(None, &current),
        ServerLivenessObservation::Healthy(Some(current))
    );
}

/// A successful probe whose payload does not carry a parseable identity tells
/// us nothing about the server, so the observation is unavailable rather than
/// a loss.
#[test]
fn unparseable_probe_output_is_unavailable() {
    let evidence = ServerLivenessEvidence::command_succeeded("not an identity", "");

    assert_eq!(
        classify_observation(None, &evidence),
        ServerLivenessObservation::Unavailable
    );
}

/// A probe that fails with a recognised "no server" diagnostic is a genuine
/// loss of the pinned server.
#[test]
fn no_server_stderr_is_gone() {
    let prior = identity(656, 134_304_228_211_297_590);
    let evidence = ServerLivenessEvidence::command_failed("no server running on jefe-abc");

    assert_eq!(
        classify_observation(Some(&prior), &evidence),
        ServerLivenessObservation::Gone
    );
}

/// A probe that fails for an unrecognised reason is not evidence of loss.
#[test]
fn unrecognised_failure_is_unavailable() {
    let prior = identity(656, 134_304_228_211_297_590);
    let evidence = ServerLivenessEvidence::command_failed("permission denied");

    assert_eq!(
        classify_observation(Some(&prior), &evidence),
        ServerLivenessObservation::Unavailable
    );
}

/// A probe that could not be spawned at all says nothing about the server.
#[test]
fn spawn_failure_is_unavailable() {
    let prior = identity(656, 134_304_228_211_297_590);
    let evidence = ServerLivenessEvidence::spawn_failed();

    assert_eq!(
        classify_observation(Some(&prior), &evidence),
        ServerLivenessObservation::Unavailable
    );
}

/// A successful probe naming a live process resolves that process's real
/// creation discriminator rather than the parser's placeholder, so the
/// identity jefe pins is the one the operating system reports.
#[cfg(windows)]
#[test]
fn live_pid_resolves_to_the_observed_process_identity() {
    let pid = std::process::id();
    let stdout = format!("|{pid}|3.3.7");
    let evidence = ServerLivenessEvidence::command_succeeded(&stdout, "");

    match classify_observation(None, &evidence) {
        ServerLivenessObservation::Healthy(Some(observed)) => {
            assert_eq!(observed.process.pid(), pid);
            assert!(observed.process.started_at().is_some());
        }
        other => panic!("expected a resolved healthy identity, got {other:?}"),
    }
}

/// Exit-empty remediation is applied only for identities jefe has accepted as
/// the current server. A conflicting identity is not accepted, so it must not
/// reconfigure the server.
#[cfg(windows)]
#[test]
fn conflicting_identity_is_not_an_exit_empty_target() {
    let current = identity(19948, 134_304_226_880_092_839);

    assert_eq!(
        super::server_health_io::exit_empty_target(
            &ServerLivenessObservation::ConflictingIdentity(current)
        ),
        None
    );
}

/// Accepted identities — a healthy server and a genuine replacement — remain
/// exit-empty targets.
#[cfg(windows)]
#[test]
fn accepted_identities_are_exit_empty_targets() {
    let current = identity(19948, 134_304_226_880_092_839);

    assert_eq!(
        super::server_health_io::exit_empty_target(&ServerLivenessObservation::Replaced(
            current.clone()
        )),
        Some(current.clone())
    );
    assert_eq!(
        super::server_health_io::exit_empty_target(&ServerLivenessObservation::Healthy(Some(
            current.clone()
        ))),
        Some(current)
    );
}

/// Observations that carry no accepted identity never trigger remediation.
#[cfg(windows)]
#[test]
fn identity_free_observations_are_not_exit_empty_targets() {
    assert_eq!(
        super::server_health_io::exit_empty_target(&ServerLivenessObservation::Gone),
        None
    );
    assert_eq!(
        super::server_health_io::exit_empty_target(&ServerLivenessObservation::Unavailable),
        None
    );
    assert_eq!(
        super::server_health_io::exit_empty_target(&ServerLivenessObservation::Healthy(None)),
        None
    );
}

// --- Namespace-token classification (issue #668) -------------------------
//
// psmux runs one server *per session*, so a namespace can hold several live
// servers and `display-message` answers with whichever one replied. The
// `#{server_instance}` token identifies the namespace itself, and until #668
// the I/O path parsed it and then dropped it, leaving every verdict to the
// weaker pid comparison. These tests drive the production composition —
// parse the probe answer, merge the operating system's view of the answering
// process, classify — so the token is proven to survive that merge and to
// decide the verdict once it does.

/// One namespace's token, and a second, different namespace's token.
const NAMESPACE_A: &str = "883b25f5379f199a";
const NAMESPACE_B: &str = "f3cb9da032325298";

/// The multiplexer version every probe answer below reports.
const VERSION: &str = "3.3.7";

/// Creation discriminators from the issue #664 production observation: 133
/// seconds apart, `OLDER` belonging to the process that answered second.
const OLDER: u64 = 134_304_226_880_092_839;
const NEWER: u64 = 134_304_228_211_297_590;

/// Render a probe answer the way the multiplexer does. An empty `token`
/// reproduces a multiplexer predating psmux#509.
fn answer(token: &str, pid: u32) -> String {
    format!("{token}|{pid}|{VERSION}")
}

/// Build an identity exactly as the production path does: the multiplexer's
/// parsed answer merged with the operating system's view of the process that
/// answered. Each side owns half the evidence, so both halves must survive.
fn observed(token: &str, pid: u32, started_at: u64) -> ServerIdentity {
    let stdout = answer(token, pid);
    let Some(parsed) = parse_server_identity_output(&stdout) else {
        panic!("a server identity probe answer must parse, got {stdout:?}")
    };
    let process = ProcessIdentity::new(parsed.process.pid(), started_at);
    resolve_observed_identity(parsed, process)
}

/// Resolving an answer against the operating system keeps the namespace token
/// the multiplexer reported and adopts the creation discriminator only the
/// operating system can supply.
#[test]
fn resolving_an_answer_keeps_the_namespace_token_and_takes_the_real_start() {
    let resolved = observed(NAMESPACE_A, 656, NEWER);

    assert_eq!(
        resolved.instance.as_ref().map(ServerInstanceToken::as_str),
        Some(NAMESPACE_A)
    );
    assert_eq!(resolved.process, ServerProcessIdentity::new(656, NEWER));
    assert_eq!(resolved.multiplexer, MultiplexerVersion::new(3, 3, 7));
}

/// A multiplexer predating psmux#509 renders the token as empty text. That
/// blank field stays absent rather than becoming a token, so such a server is
/// still classified on process identity alone.
#[test]
fn resolving_a_tokenless_answer_yields_no_namespace_token() {
    let resolved = observed("", 656, NEWER);

    assert_eq!(resolved.instance, None);
    assert_eq!(resolved.process, ServerProcessIdentity::new(656, NEWER));
}

/// The same namespace answering from a different per-session server is not a
/// replacement: adding a session to a live namespace changes which pid
/// replies but nothing about the namespace (issue #540).
#[test]
fn the_same_namespace_token_at_a_different_pid_is_healthy() {
    let prior = observed(NAMESPACE_A, 9008, OLDER);
    let current = observed(NAMESPACE_A, 3832, NEWER);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Healthy(Some(current))
    );
}

/// The issue #664 production shape, now carrying a token: the server that
/// answered is 133 seconds *older* than the pinned one. Within one namespace
/// that is an ordinary sibling server, so the token settles it as healthy and
/// the monotonicity guard is never reached — the guard does not mask the
/// token rule.
#[test]
fn the_same_namespace_token_at_an_older_sibling_server_is_healthy() {
    let prior = observed(NAMESPACE_A, 656, NEWER);
    let current = observed(NAMESPACE_A, 19948, OLDER);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Healthy(Some(current))
    );
}

/// A different namespace token means the namespace itself changed. When the
/// answering process is strictly newer the restart is orderable, so it is a
/// genuine replacement even though the operating system reused the pid.
#[test]
fn a_different_namespace_token_at_a_reused_pid_is_replaced() {
    let prior = observed(NAMESPACE_A, 656, OLDER);
    let current = observed(NAMESPACE_B, 656, NEWER);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::Replaced(current)
    );
}

/// One process cannot belong to two namespaces. A different token on an
/// identical process is contradictory rather than a restart, and the #664
/// guard refuses it because nothing was created after anything else.
#[test]
fn a_different_namespace_token_on_an_identical_process_is_conflicting() {
    let prior = observed(NAMESPACE_A, 656, NEWER);
    let current = observed(NAMESPACE_B, 656, NEWER);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::ConflictingIdentity(current)
    );
}

/// A token on the pinned side only is not decisive in either direction: with
/// nothing to compare it against, the verdict falls back to process identity.
#[test]
fn a_token_on_the_prior_only_falls_back_to_the_process_identity() {
    let prior = observed(NAMESPACE_A, 656, NEWER);

    let same_process = observed("", 656, NEWER);
    assert_eq!(
        classify_resolved_identity(Some(&prior), &same_process),
        ServerLivenessObservation::Healthy(Some(same_process))
    );

    let newer_process = observed("", 19948, NEWER + 1);
    assert_eq!(
        classify_resolved_identity(Some(&prior), &newer_process),
        ServerLivenessObservation::Replaced(newer_process)
    );
}

/// The mirror case: a token appearing on the fresh answer only cannot be
/// matched against the pinned identity, so process identity decides again.
#[test]
fn a_token_on_the_current_only_falls_back_to_the_process_identity() {
    let prior = observed("", 656, NEWER);

    let same_process = observed(NAMESPACE_A, 656, NEWER);
    assert_eq!(
        classify_resolved_identity(Some(&prior), &same_process),
        ServerLivenessObservation::Healthy(Some(same_process))
    );

    let newer_process = observed(NAMESPACE_A, 19948, NEWER + 1);
    assert_eq!(
        classify_resolved_identity(Some(&prior), &newer_process),
        ServerLivenessObservation::Replaced(newer_process)
    );
}

/// The issue #664 guard survives the token path going live: a genuinely
/// different namespace whose server predates the pinned one is still refused,
/// so the token rule does not promote a non-monotonic answer to `Replaced`.
#[test]
fn a_non_monotonic_namespace_change_is_still_refused() {
    let prior = observed(NAMESPACE_A, 656, NEWER);
    let current = observed(NAMESPACE_B, 19948, OLDER);

    assert_eq!(
        classify_resolved_identity(Some(&prior), &current),
        ServerLivenessObservation::ConflictingIdentity(current)
    );
}

/// End to end on the platform where the defect was observed: a probe answer
/// carrying a namespace token reaches the pinned identity with that token
/// intact, alongside the operating system's creation discriminator.
#[cfg(windows)]
#[test]
fn a_probed_namespace_token_reaches_the_pinned_identity() {
    let pid = std::process::id();
    let stdout = answer(NAMESPACE_B, pid);
    let evidence = ServerLivenessEvidence::command_succeeded(&stdout, "");

    match classify_observation(None, &evidence) {
        ServerLivenessObservation::Healthy(Some(observed)) => {
            assert_eq!(
                observed.instance.as_ref().map(ServerInstanceToken::as_str),
                Some(NAMESPACE_B)
            );
            assert_eq!(observed.process.pid(), pid);
            assert!(observed.process.started_at().is_some());
        }
        other => panic!("expected a resolved healthy identity, got {other:?}"),
    }
}
