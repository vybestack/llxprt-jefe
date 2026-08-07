//! Scheduled provider work must survive transient contention (issue #390).
//!
//! The worker drains the queue, then needs the context lock to look up the
//! descriptor. Losing the item when that lock is momentarily held turns a
//! 50-millisecond scheduling hiccup into a request that never runs and reports
//! "provider stream closed unexpectedly", which is not what happened.

use super::*;

fn id(value: &str) -> crate::domain::Id {
    match crate::domain::Id::parse(value) {
        Ok(parsed) => parsed,
        Err(error) => panic!("id fixture {value:?} must parse: {error}"),
    }
}

fn work_item(generation: u64) -> ProviderWorkItem {
    ProviderWorkItem {
        invocation: crate::domain::effects::ProviderInvocation {
            key: crate::domain::effects::ProviderRequestKey {
                owner: id("host"),
                action_id: id("vendor.pkg.run"),
                generation,
            },
            arguments: crate::domain::TypedMap::new(),
            context_screen: id("core.dashboard"),
            context_instance: id("instance-1"),
            context_refs: crate::domain::TypedMap::new(),
            continuation: None,
        },
        correlation: Correlation {
            correlation_id: crate::domain::effects::CorrelationId::new(generation),
            owner: id("host"),
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: crate::domain::effects::SemanticKey::new(
                crate::domain::effects::EffectFamily::Provider,
                "test",
            ),
        },
    }
}

#[test]
fn deferred_work_returns_to_the_queue_in_its_original_order() {
    let handle = ProviderEffectHandle::new();
    handle.schedule(work_item(1));
    handle.schedule(work_item(2));
    handle.schedule(work_item(3));

    let drained = handle.drain();
    assert_eq!(drained.len(), 3);
    assert!(!handle.is_dirty());

    // The worker could not reach the coordinator, so the batch goes back.
    handle.defer_all(drained);

    assert!(
        handle.is_dirty(),
        "deferred work must make the queue dirty again so the worker retries"
    );
    let generations: Vec<u64> = handle
        .drain()
        .into_iter()
        .map(|item| item.invocation.key.generation)
        .collect();
    assert_eq!(
        generations,
        vec![1, 2, 3],
        "deferring must preserve dispatch order, not reverse it"
    );
}

#[test]
fn deferred_work_precedes_newly_scheduled_work() {
    let handle = ProviderEffectHandle::new();
    handle.schedule(work_item(1));
    let drained = handle.drain();
    handle.schedule(work_item(2));
    handle.defer_all(drained);

    let generations: Vec<u64> = handle
        .drain()
        .into_iter()
        .map(|item| item.invocation.key.generation)
        .collect();
    assert_eq!(
        generations,
        vec![1, 2],
        "work that was already dispatched must not be overtaken by later work"
    );
}
