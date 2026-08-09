use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use jefe::domain::Id;
use jefe::domain::effects::{
    Correlation, CorrelationId, EffectFamily, ProviderRequestKey, SemanticKey,
};
use jefe::runtime::provider::protocol::ProgressPayload;

use super::{
    ActiveSession, SessionStart, TerminalSource, drain_session_progress, fill_session_slots,
    forward_exact_cancels,
};

fn id(value: &str) -> Id {
    match Id::parse(value) {
        Ok(parsed) => parsed,
        Err(error) => panic!("test id {value:?} must be valid: {error}"),
    }
}

fn key(generation: u64) -> ProviderRequestKey {
    ProviderRequestKey {
        owner: id("core.workbench"),
        action_id: id("vendor.provider.run"),
        generation,
    }
}

fn correlation(generation: u64) -> Correlation {
    Correlation {
        correlation_id: CorrelationId::new(generation),
        owner: id("core.workbench"),
        screen_generation: 1,
        activation_generation: 1,
        semantic_key: SemanticKey::new(EffectFamily::Provider, "invoke"),
    }
}

fn session(
    generation: u64,
) -> (
    ActiveSession,
    mpsc::Sender<ProgressPayload>,
    Arc<AtomicBool>,
) {
    let (progress_tx, progress_rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let (_result_tx, result_rx) = mpsc::channel();
    (
        ActiveSession {
            key: key(generation),
            correlation: correlation(generation),
            progress_rx,
            cancel: cancel.clone(),
            terminal: TerminalSource::Persistent { done, result_rx },
        },
        progress_tx,
        cancel,
    )
}

fn progress(sequence: u16, message: &str) -> ProgressPayload {
    ProgressPayload {
        sequence,
        message: message.to_owned(),
        completed: Some(u64::from(sequence)),
        total: Some(4),
    }
}

#[test]
fn bounded_admission_preserves_fifo_and_refills_each_freed_slot() {
    let mut active = Vec::new();
    let mut deferred = (1_u8..=18).collect::<VecDeque<_>>();
    let mut started = Vec::new();

    fill_session_slots(
        &mut active,
        &mut deferred,
        16,
        |item| {
            started.push(item);
            SessionStart::<u8, u8, ()>::Started(item)
        },
        |()| {},
    );

    assert_eq!(active, (1_u8..=16).collect::<Vec<_>>());
    assert_eq!(deferred, VecDeque::from([17, 18]));
    active.remove(0);

    fill_session_slots(
        &mut active,
        &mut deferred,
        16,
        |item| {
            started.push(item);
            SessionStart::<u8, u8, ()>::Started(item)
        },
        |()| {},
    );

    assert_eq!(active.len(), 16);
    assert_eq!(active.last(), Some(&17));
    assert_eq!(deferred, VecDeque::from([18]));
    assert_eq!(started, (1_u8..=17).collect::<Vec<_>>());
}

#[test]
fn a_deferred_head_remains_owned_and_cannot_be_overtaken() {
    let mut active = Vec::new();
    let mut deferred = VecDeque::from([1_u8, 2]);
    let mut attempted = Vec::new();

    fill_session_slots(
        &mut active,
        &mut deferred,
        16,
        |item| {
            attempted.push(item);
            SessionStart::<u8, u8, ()>::Deferred(item)
        },
        |()| {},
    );

    assert!(active.is_empty());
    assert_eq!(deferred, VecDeque::from([1, 2]));
    assert_eq!(attempted, vec![1]);
}

#[test]
fn failed_admission_consumes_no_slot_and_later_work_can_fill_capacity() {
    let mut active = Vec::new();
    let mut deferred = VecDeque::from([1_u8, 2]);
    let mut failures = Vec::new();

    fill_session_slots(
        &mut active,
        &mut deferred,
        1,
        |item| {
            if item == 1 {
                SessionStart::Failed(item)
            } else {
                SessionStart::Started(item)
            }
        },
        |failure| failures.push(failure),
    );

    assert_eq!(failures, vec![1]);
    assert_eq!(active, vec![2]);
    assert!(deferred.is_empty());
}

#[test]
fn cancellation_routes_only_to_the_exact_request_generation() {
    let (first, _first_progress, first_cancel) = session(1);
    let (second, _second_progress, second_cancel) = session(2);
    let active = vec![first, second];

    forward_exact_cancels(&active, [key(2)]);

    assert!(!first_cancel.load(Ordering::SeqCst));
    assert!(second_cancel.load(Ordering::SeqCst));
}

#[test]
fn progress_from_concurrent_sessions_keeps_identity_and_dispatch_order() {
    let (first, first_tx, _first_cancel) = session(1);
    let (second, second_tx, _second_cancel) = session(2);
    assert!(first_tx.send(progress(1, "first-1")).is_ok());
    assert!(first_tx.send(progress(2, "first-2")).is_ok());
    assert!(second_tx.send(progress(1, "second-1")).is_ok());
    let active = vec![first, second];
    let mut delivered = Vec::new();

    drain_session_progress(&active, |_correlation, key, payload| {
        delivered.push((key.generation, payload.sequence, payload.message));
    });

    assert_eq!(
        delivered,
        vec![
            (1, 1, "first-1".to_owned()),
            (1, 2, "first-2".to_owned()),
            (2, 1, "second-1".to_owned()),
        ]
    );
}
