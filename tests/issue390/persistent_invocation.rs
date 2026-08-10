//! Persistent same-process invocation lifecycle (issue #390 CW-10,
//! Remediation E).
//!
//! Proves that a persistent action invokes on the already-Ready PID with no
//! second spawn; repeated invocations use the same PID/session; progress is
//! observed before terminal; cancel sends a live cancel envelope; the
//! descriptor timeout applies; and a post-Ready crash becomes a typed health
//! failure without restart.

use std::time::{Duration, Instant};

use jefe::domain::action_registry::ActionId;
use jefe::domain::{Id, TypedMap};
use jefe::runtime::provider::outcome::{CleanupFailure, OneShotOutcome, SupervisorFailure};
use jefe::runtime::provider::persistent::{
    CandidateHealth, PersistentStartup, run_persistent_startup,
};
use jefe::runtime::provider::protocol::{
    Capability, InvokeActionPayload, InvokeContext, RequestId,
};
use jefe::runtime::provider::{PersistentInvocation, PersistentInvokeError};

use super::persistent_support::{
    EmptyEnv, Scene, candidate_pid, fast_bounds, process_budget, process_is_gone, startup_sequence,
};

/// Build a minimal `InvokeActionPayload` for testing.
fn invoke_payload(plugin_id: &str, sequence: u64) -> InvokeActionPayload {
    let invocation_id = Id::parse(&format!("{plugin_id}.{sequence}"))
        .unwrap_or_else(|err| panic!("invocation id: {err:?}"));
    InvokeActionPayload {
        invocation_id,
        action_id: ActionId::parse("vendor.alpha.run")
            .unwrap_or_else(|err| panic!("action id: {err:?}")),
        arguments: TypedMap::new(),
        context: InvokeContext {
            screen_id: Id::parse("core.dashboard")
                .unwrap_or_else(|err| panic!("screen id: {err:?}")),
            screen_instance: Id::parse("inst-1")
                .unwrap_or_else(|err| panic!("instance id: {err:?}")),
            resource_refs: TypedMap::new(),
        },
        continuation: None,
    }
}

/// Start a one-candidate supervisor and convert it into session ownership.
fn start_session_owner(
    scene: &Scene,
    mode: &str,
) -> jefe::runtime::provider::PersistentSessionOwner {
    let startup = PersistentStartup {
        candidates: vec![scene.candidate("vendor.alpha", mode, vec![Capability::Actions])],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let supervisor = match result {
        jefe::runtime::provider::persistent::PersistentStartupResult::Started {
            supervisor,
            ..
        } => supervisor,
        other @ jefe::runtime::provider::persistent::PersistentStartupResult::Failed(_) => {
            panic!("expected started, got {other:?}")
        }
    };
    supervisor.into_sessions()
}

/// Poll `invocation.progress_rx` until a progress payload arrives or the
/// deadline elapses.
fn recv_progress(
    invocation: &PersistentInvocation,
    deadline: Instant,
) -> Option<jefe::runtime::provider::protocol::ProgressPayload> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    invocation.progress_rx.recv_timeout(remaining).ok()
}

/// Wait for the invocation to finish, polling `done` to avoid blocking longer
/// than necessary.
fn wait_finish(
    invocation: PersistentInvocation,
    deadline: Instant,
) -> jefe::runtime::provider::outcome::OneShotResult {
    loop {
        if invocation.is_finished() {
            return invocation.finish();
        }
        assert!(
            Instant::now() < deadline,
            "persistent invocation did not finish before the test deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cw10_e_a_persistent_action_invokes_on_the_ready_pid_with_no_second_spawn() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke");
    let pid = candidate_pid(&scene, "vendor.alpha");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let request_id =
        RequestId::parse("h-000002").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload = invoke_payload("vendor.alpha", 1);
    let invocation = owner
        .invoke(&plugin_id, request_id, payload, Duration::from_secs(5))
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));
    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(10));
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed, got {:?}",
        result.outcome
    );
    // The original PID is still alive — no per-invocation shutdown/restart.
    assert!(
        !process_is_gone(pid),
        "the ready PID died after the first invocation (no second spawn should occur)"
    );
    // Exactly one startup entry — the initial candidate startup, not a re-spawn.
    assert_eq!(
        startup_sequence(&scene),
        vec!["vendor.alpha"],
        "no second spawn wrote a startup entry"
    );
    owner.shutdown();
}

#[test]
fn cw10_e_b_repeated_invocations_use_the_same_pid_and_session() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke");
    let pid = candidate_pid(&scene, "vendor.alpha");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));

    // First invocation.
    let request_a =
        RequestId::parse("h-000010").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload_a = invoke_payload("vendor.alpha", 10);
    let invocation_a = owner
        .invoke(&plugin_id, request_a, payload_a, Duration::from_secs(5))
        .unwrap_or_else(|err| panic!("invoke A: {err:?}"));
    let result_a = wait_finish(invocation_a, Instant::now() + Duration::from_secs(10));
    assert!(
        matches!(result_a.outcome, OneShotOutcome::Completed(_)),
        "first invocation: {:?}",
        result_a.outcome
    );

    // Second invocation — same PID, same session.
    let request_b =
        RequestId::parse("h-000011").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload_b = invoke_payload("vendor.alpha", 11);
    let invocation_b = owner
        .invoke(&plugin_id, request_b, payload_b, Duration::from_secs(5))
        .unwrap_or_else(|err| panic!("invoke B: {err:?}"));
    let result_b = wait_finish(invocation_b, Instant::now() + Duration::from_secs(10));
    assert!(
        matches!(result_b.outcome, OneShotOutcome::Completed(_)),
        "second invocation: {:?}",
        result_b.outcome
    );

    // The PID did not change and is still alive after both invocations.
    assert_eq!(
        candidate_pid(&scene, "vendor.alpha"),
        pid,
        "the same PID served both invocations"
    );
    assert!(!process_is_gone(pid));
    assert_eq!(
        startup_sequence(&scene),
        vec!["vendor.alpha"],
        "no re-spawn between invocations"
    );
    owner.shutdown();
}

#[test]
fn cw10_e_c_progress_is_observed_before_the_terminal() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let request_id =
        RequestId::parse("h-000020").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload = invoke_payload("vendor.alpha", 20);
    let invocation = owner
        .invoke(&plugin_id, request_id, payload, Duration::from_secs(5))
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));

    // Collect at least one progress payload before the invocation finishes.
    let progress_deadline = Instant::now() + Duration::from_secs(5);
    let mut progress_count = 0;
    while let Some(_payload) = recv_progress(&invocation, progress_deadline) {
        progress_count += 1;
        if progress_count >= 1 {
            break;
        }
    }
    assert!(
        progress_count >= 1,
        "at least one progress payload must arrive before terminal"
    );

    // The terminal result confirms the invocation completed.
    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(10));
    assert!(
        matches!(result.outcome, OneShotOutcome::Completed(_)),
        "expected completed, got {:?}",
        result.outcome
    );
    owner.shutdown();
}

#[test]
fn cw10_e_d_cancel_sends_a_live_cancel_envelope_and_returns_cancelled() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-hang");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let request_id =
        RequestId::parse("h-000030").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload = invoke_payload("vendor.alpha", 30);
    let invocation = owner
        .invoke(&plugin_id, request_id, payload, Duration::from_secs(10))
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));

    // Wait for the fixture's first progress frame, then request cancellation.
    let progress_deadline = Instant::now() + Duration::from_secs(5);
    let got_progress = recv_progress(&invocation, progress_deadline).is_some();
    assert!(got_progress, "fixture must emit progress before cancel");

    invocation
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(5));
    assert!(
        matches!(result.outcome, OneShotOutcome::Cancelled),
        "expected cancelled, got {:?}",
        result.outcome
    );

    // The fixture recorded receipt of the cancel frame (cancel envelope was sent).
    let cancel_marker = scene.record_dir.join("vendor.alpha.cancel");
    assert!(
        cancel_marker.exists(),
        "the host cancel envelope was not received by the fixture"
    );

    owner.shutdown();
}

#[test]
fn cw10_e_e_descriptor_timeout_applies_to_the_invocation() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-hang");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let request_id =
        RequestId::parse("h-000040").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload = invoke_payload("vendor.alpha", 40);
    // One-second invocation timeout. The fixture emits one progress then hangs;
    // the invocation must time out and return a typed InvocationTimeout failure.
    let invocation = owner
        .invoke(&plugin_id, request_id, payload, Duration::from_secs(1))
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));

    let start = Instant::now();
    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(5));
    let elapsed = start.elapsed();
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::InvocationTimeout)
        ),
        "expected invocation timeout, got {:?}",
        result.outcome
    );
    // The timeout must have fired near the 1-second bound, not the default 60 s.
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed <= Duration::from_secs(3),
        "timeout fired at {elapsed:?}, expected ~1s"
    );
    owner.shutdown();
}

#[test]
fn cw10_e_f_post_ready_crash_during_invocation_becomes_typed_health_failure() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-then-crash");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let request_id =
        RequestId::parse("h-000050").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload = invoke_payload("vendor.alpha", 50);
    let invocation = owner
        .invoke(&plugin_id, request_id, payload, Duration::from_secs(5))
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));

    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(10));
    // The invocation fails: the provider crashed mid-invocation.
    assert!(
        matches!(
            result.outcome,
            OneShotOutcome::Failed(SupervisorFailure::Crashed { .. } | SupervisorFailure::Io(_))
        ),
        "expected crashed/io failure after provider exit, got {:?}",
        result.outcome
    );

    // Health reports the candidate exited (not Ready): no auto-restart.
    let health = owner.health();
    assert_eq!(health.len(), 1);
    match &health[0].health {
        CandidateHealth::Exited { .. } => {}
        other => panic!("expected Exited health after crash, got {other:?}"),
    }

    // The session still accepts commands but the invocation fails because the
    // provider process has exited. No auto-restart occurs.
    let request_b =
        RequestId::parse("h-000051").unwrap_or_else(|err| panic!("request id: {err:?}"));
    let payload_b = invoke_payload("vendor.alpha", 51);
    let second = owner.invoke(&plugin_id, request_b, payload_b, Duration::from_secs(2));
    match second {
        Ok(invocation_b) => {
            let result_b = wait_finish(invocation_b, Instant::now() + Duration::from_secs(5));
            assert!(
                matches!(
                    result_b.outcome,
                    OneShotOutcome::Failed(
                        SupervisorFailure::Crashed { .. } | SupervisorFailure::Io(_)
                    )
                ),
                "expected crashed/io failure on second invocation, got {:?}",
                result_b.outcome
            );
        }
        Err(PersistentInvokeError::SessionGone) => {}
        Err(other) => panic!("unexpected second invocation error: {other:?}"),
    }
    owner.shutdown();
}

#[test]
fn cw10_e_g_health_remains_observable_during_a_live_invocation() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-hang");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let invocation = owner
        .invoke(
            &plugin_id,
            RequestId::parse("h-000060").unwrap_or_else(|err| panic!("request id: {err:?}")),
            invoke_payload("vendor.alpha", 60),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));
    assert!(recv_progress(&invocation, Instant::now() + Duration::from_secs(5)).is_some());

    let health = owner.health();
    assert_eq!(health.len(), 1);
    assert!(matches!(health[0].health, CandidateHealth::Ready { .. }));

    invocation
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = wait_finish(invocation, Instant::now() + Duration::from_secs(5));
    owner.shutdown();
}

#[test]
fn cw10_e_h_shutdown_interrupts_a_live_invocation_and_reaps_the_candidate() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-hang");
    let pid = candidate_pid(&scene, "vendor.alpha");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let invocation = owner
        .invoke(
            &plugin_id,
            RequestId::parse("h-000061").unwrap_or_else(|err| panic!("request id: {err:?}")),
            invoke_payload("vendor.alpha", 61),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|err| panic!("invoke: {err:?}"));
    assert!(recv_progress(&invocation, Instant::now() + Duration::from_secs(5)).is_some());

    owner.shutdown();
    let result = wait_finish(invocation, Instant::now() + Duration::from_secs(5));
    assert!(matches!(result.outcome, OneShotOutcome::Cancelled));
    assert!(process_is_gone(pid));
}

#[test]
fn cw10_e_i_persistent_outbound_invocation_queue_is_bounded_to_64() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-invoke-hang");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let active = owner
        .invoke(
            &plugin_id,
            RequestId::parse("h-000100").unwrap_or_else(|err| panic!("request id: {err:?}")),
            invoke_payload("vendor.alpha", 100),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|err| panic!("active invoke: {err:?}"));
    assert!(recv_progress(&active, Instant::now() + Duration::from_secs(5)).is_some());

    let mut queued = Vec::new();
    for sequence in 101_u64..=164 {
        let request_id = RequestId::new_host(sequence)
            .unwrap_or_else(|err| panic!("request id {sequence}: {err:?}"));
        let invocation = owner
            .invoke(
                &plugin_id,
                request_id,
                invoke_payload("vendor.alpha", sequence),
                Duration::from_secs(10),
            )
            .unwrap_or_else(|err| panic!("queued invoke {sequence}: {err:?}"));
        queued.push(invocation);
    }
    let overflow = owner.invoke(
        &plugin_id,
        RequestId::parse("h-000165").unwrap_or_else(|err| panic!("request id: {err:?}")),
        invoke_payload("vendor.alpha", 165),
        Duration::from_secs(10),
    );
    assert!(matches!(overflow, Err(PersistentInvokeError::QueueFull)));

    owner.shutdown();
    drop(queued);
    let _ = wait_finish(active, Instant::now() + Duration::from_secs(5));
}

#[test]
fn cw10_e_j_late_terminal_after_cancel_is_diagnostic_not_the_next_result() {
    let _budget = process_budget();
    let scene = Scene::new();
    let mut owner = start_session_owner(&scene, "persistent-cancel-then-terminal");
    let plugin_id = Id::parse("vendor.alpha").unwrap_or_else(|err| panic!("plugin id: {err:?}"));
    let first = owner
        .invoke(
            &plugin_id,
            RequestId::parse("h-000200").unwrap_or_else(|err| panic!("request id: {err:?}")),
            invoke_payload("vendor.alpha", 200),
            Duration::from_secs(5),
        )
        .unwrap_or_else(|err| panic!("first invoke: {err:?}"));
    assert!(recv_progress(&first, Instant::now() + Duration::from_secs(5)).is_some());
    first
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let first_result = wait_finish(first, Instant::now() + Duration::from_secs(5));
    assert!(matches!(first_result.outcome, OneShotOutcome::Cancelled));
    assert!(
        matches!(
            first_result.cleanup_failure,
            Some(CleanupFailure::PostTerminal(_))
        ),
        "the accepted cancellation must retain the later byte as a protocol diagnostic"
    );

    let second = owner.invoke(
        &plugin_id,
        RequestId::parse("h-000201").unwrap_or_else(|err| panic!("request id: {err:?}")),
        invoke_payload("vendor.alpha", 201),
        Duration::from_secs(2),
    );
    assert!(
        matches!(second, Err(PersistentInvokeError::SessionGone)),
        "a generation made unhealthy by late terminal bytes must reject the next invocation"
    );
    owner.shutdown();
}
