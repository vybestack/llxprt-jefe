//! Behavioral coverage for the multiplexer server-health probe classifier.
//!
//! The classifier ran untested in production until issue #664: a `Replaced`
//! verdict was emitted for a server whose creation discriminator was 133
//! seconds *older* than the server it supposedly replaced. These tests pin the
//! classification of every observation the probe can produce, starting with
//! the `Replaced` branch and the conflicting-identity guard that now bounds it.

use super::server_health::{ServerIdentity, ServerLivenessEvidence, ServerLivenessObservation};
use super::server_health_io::{classify_observation, classify_resolved_identity};
use crate::domain::ServerProcessIdentity;
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
