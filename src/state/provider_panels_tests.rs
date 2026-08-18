//! Lifecycle, identity, revision, rate, and HostLocal reducer tables
//! (issue #391).
//!
//! Pure deterministic behavior only: every timestamp is injected, no I/O is
//! performed, and every failure is observable as [`PanelError`] carrying
//! `PLG-E502` without echoing the offending value.

use super::{
    AcceptSnapshot, ActivateOutcome, DeactivateOutcome, DeactivateReason, DeclareInput,
    DeclareOutcome, EventDeclaration, EventKind, HOST_LOCAL_MAX_BYTES, MODEL_SCHEMA, PanelError,
    PanelInstanceId, PanelLifecycle, ProviderPanelState, SNAPSHOT_MAX_BYTES, SubmitEvent,
    TOKEN_CAPACITY,
};
use crate::domain::{Id, TypedMap, TypedValue};
use crate::runtime::provider::protocol::{
    BodyKind, EmptyBody, HostLocal, ListBody, ListItem, PanelBody, PanelEvent, PanelSnapshot,
};
use crate::test_support::{Must, MustErr};
use crate::workbench::PanelId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn id(text: &str) -> Id {
    Id::parse(text).unwrap_or_else(|error| panic!("valid id {text:?}: {error:?}"))
}

fn owner() -> Id {
    id("vendor.pkg")
}

fn panel_type() -> Id {
    id("vendor.panel")
}

fn empty_activation() -> TypedMap {
    TypedMap::new()
}

fn declare(state: &mut ProviderPanelState, process_generation: u64) -> DeclareOutcome {
    state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id: &PanelId::from_static("main"),
            screen_instance_id: 7,
            panel_type: &panel_type(),
            activation: &empty_activation(),
            allowed_model_kinds: &[
                BodyKind::List,
                BodyKind::Detail,
                BodyKind::Form,
                BodyKind::Status,
                BodyKind::Progress,
                BodyKind::Empty,
                BodyKind::Error,
            ],
            allowed_events: &[],
            action_authority: &[],
            process_generation,
        })
        .must("declare succeeds")
}

fn activate(state: &mut ProviderPanelState, panel: PanelInstanceId) -> ActivateOutcome {
    state.activate(panel).must("activate succeeds")
}

fn empty_snapshot(panel: u64, generation: u64, rev: u64) -> PanelSnapshot {
    PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel,
        generation,
        revision: rev,
        kind: BodyKind::Empty,
        title: "t".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::Empty(EmptyBody {
            message: String::new(),
            action: None,
        }),
    }
}

fn accept(
    state: &mut ProviderPanelState,
    _panel: PanelInstanceId,
    snapshot: &PanelSnapshot,
    elapsed_ms: u64,
    payload_bytes: u64,
) -> Result<u64, PanelError> {
    state
        .accept_snapshot(AcceptSnapshot {
            owner: &owner(),
            received_process_generation: 1,
            payload_byte_count: payload_bytes,
            elapsed_ms,
            snapshot,
        })
        .map(|outcome| outcome.revision)
}

/// Accept one snapshot at the natural next revision with a small payload.
fn accept_ok(state: &mut ProviderPanelState, panel: PanelInstanceId) -> u64 {
    let generation = state.generation(panel).must("generation known");
    let rev = state.accepted_revision(panel).unwrap_or(0) + 1;
    let snap = empty_snapshot(panel.as_u64(), generation, rev);
    accept(state, panel, &snap, 0, 1).must("snapshot accepted")
}

fn first_live_panel(state: &mut ProviderPanelState) -> PanelInstanceId {
    let outcome = declare(state, 1);
    activate(state, outcome.instance);
    outcome.instance
}

// ---------------------------------------------------------------------------
// A. Lifecycle table and instance non-reuse
// ---------------------------------------------------------------------------

#[test]
fn declare_allocates_positive_non_reused_instances() {
    let mut state = ProviderPanelState::new();
    let a = declare(&mut state, 1).instance;
    let b = declare(&mut state, 1).instance;
    let c = declare(&mut state, 1).instance;
    assert!(a.as_u64() > 0);
    assert!(b.as_u64() > 0);
    assert!(c.as_u64() > 0);
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(b, c);
}

#[test]
fn declared_and_preallocated_panels_share_one_process_identity_authority() {
    let mut state = ProviderPanelState::new();
    let preallocated = PanelInstanceId::next();
    let ordinary = declare(&mut state, 1).instance;

    assert_ne!(ordinary, preallocated);
}

#[test]
fn declared_lifecycle_is_declared_before_activation() {
    let mut state = ProviderPanelState::new();
    let panel = declare(&mut state, 1).instance;
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Declared)
    );
}

#[test]
fn activate_returns_activate_effect_with_generation_one() {
    let mut state = ProviderPanelState::new();
    let panel = declare(&mut state, 1).instance;
    let outcome = activate(&mut state, panel);
    assert_eq!(outcome.effect.generation, 1, "generation begins at 1");
    assert_eq!(outcome.effect.panel_instance, panel);
    assert_eq!(outcome.effect.screen_instance, 7);
    assert_eq!(outcome.effect.panel_type, panel_type());
    assert!(outcome.effect.prior_host_local.is_none());
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Activating)
    );
}

#[test]
fn accept_snapshot_transitions_to_active_and_reports_revision_one() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let revision = accept_ok(&mut state, panel);
    assert_eq!(revision, 1);
    assert_eq!(state.lifecycle(panel), Some(super::PanelLifecycle::Active));
    assert!(!state.is_stale(panel).must("expected value"));
}

#[test]
fn retry_from_active_allocates_a_fresh_incrementing_generation() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let outcome = state.retry(panel).must("retry from active");
    assert_eq!(outcome.effect.generation, 2);
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Activating)
    );
}

#[test]
fn retry_rejects_activating_and_suspended_panels_without_losing_host_local() {
    let mut state = ProviderPanelState::new();
    let panel = declare(&mut state, 1).instance;
    activate(&mut state, panel);
    assert!(matches!(state.retry(panel), Err(PanelError::InvalidLifecycle)));

    accept_ok(&mut state, panel);
    let host_local = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: Some(id("selection")),
        form_draft: None,
    };
    state
        .update_host_local(panel, host_local.clone())
        .must("host local");
    state.suspend(panel).must("suspend");

    assert!(matches!(state.retry(panel), Err(PanelError::InvalidLifecycle)));
    assert_eq!(state.host_local(panel), Some(&host_local));
}

// ---------------------------------------------------------------------------
// B. Generation and revision exactness
// ---------------------------------------------------------------------------

#[test]
fn revision_increases_exactly_one_per_accepted_snapshot() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    assert_eq!(accept_ok(&mut state, panel), 1);
    assert_eq!(accept_ok(&mut state, panel), 2);
    assert_eq!(accept_ok(&mut state, panel), 3);
}

#[test]
fn retry_resets_revision_to_one_for_the_new_generation() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    accept_ok(&mut state, panel);
    state.retry(panel).must("retry");
    assert_eq!(state.accepted_revision(panel), None);
    assert_eq!(accept_ok(&mut state, panel), 1);
}

#[test]
fn wrong_generation_snapshot_is_rejected_without_mutation() {
    let mut panels = ProviderPanelState::new();
    let panel = first_live_panel(&mut panels);
    let stale = empty_snapshot(panel.as_u64(), 99, 1);
    let error = accept(&mut panels, panel, &stale, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::GenerationMismatch));
    assert_eq!(panels.accepted_revision(panel), None);
}

#[test]
fn wrong_revision_snapshot_is_rejected_without_mutation() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let gap = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        9,
    );
    let error = accept(&mut state, panel, &gap, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::RevisionMismatch));
    assert_eq!(state.accepted_revision(panel), Some(1));
}

#[test]
fn duplicate_revision_snapshot_is_rejected() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let dup = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        1,
    );
    let error = accept(&mut state, panel, &dup, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::RevisionMismatch));
}

#[test]
fn wrong_owner_is_rejected_without_mutation() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let snapshot = empty_snapshot(panel.as_u64(), 1, 1);
    let error = state
        .accept_snapshot(AcceptSnapshot {
            owner: &id("other.pkg"),
            received_process_generation: 1,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot: &snapshot,
        })
        .must_err("expected failure");
    assert!(matches!(error, PanelError::OwnerMismatch));
}

#[test]
fn process_generation_mismatch_is_rejected() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let snapshot = empty_snapshot(panel.as_u64(), 1, 1);
    let error = state
        .accept_snapshot(AcceptSnapshot {
            owner: &owner(),
            received_process_generation: 2,
            payload_byte_count: 1,
            elapsed_ms: 0,
            snapshot: &snapshot,
        })
        .must_err("expected failure");
    assert!(matches!(error, PanelError::ProcessGenerationMismatch));
}

#[test]
fn unknown_model_schema_is_rejected_as_model_mismatch() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let mut snapshot = empty_snapshot(panel.as_u64(), 1, 1);
    snapshot.model_schema = 2;
    let error = accept(&mut state, panel, &snapshot, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::ModelMismatch));
}

// ---------------------------------------------------------------------------
// C. Atomic replacement and stale model
// ---------------------------------------------------------------------------

#[test]
fn invalid_snapshot_marks_failed_and_stale_without_partial_apply() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let first = accept_ok(&mut state, panel);
    let prior_snapshot = state
        .accepted_snapshot(panel)
        .must("accepted snapshot present")
        .clone();
    // Oversize payload: a well-formed (correlation-valid) candidate that fails
    // model validation after consuming a token.
    let oversize = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        first + 1,
    );
    let error = accept(&mut state, panel, &oversize, 0, SNAPSHOT_MAX_BYTES + 1)
        .must_err("expected failure");
    assert!(matches!(error, PanelError::SnapshotInvalid));
    assert_eq!(state.lifecycle(panel), Some(super::PanelLifecycle::Failed));
    assert!(
        state.is_stale(panel).must("expected value"),
        "retained model is stale"
    );
    assert_eq!(
        state.accepted_snapshot(panel),
        Some(&prior_snapshot),
        "previous complete model retained byte-equivalent"
    );
}

#[test]
fn runtime_failure_marks_live_owner_panels_failed_and_stale() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let _ = accept_ok(&mut state, panel);

    assert_eq!(state.fail_runtime_owner(&owner()), 1);
    assert_eq!(state.lifecycle(panel), Some(super::PanelLifecycle::Failed));
    assert_eq!(state.is_stale(panel), Some(true));
}

#[test]
fn recovered_snapshot_restores_active_and_clears_stale() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let first = accept_ok(&mut state, panel);
    let oversize = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        first + 1,
    );
    let _ = accept(&mut state, panel, &oversize, 0, SNAPSHOT_MAX_BYTES + 1);
    // The failed revision is retried at the same revision and accepted.
    assert_eq!(accept_ok(&mut state, panel), first + 1);
    assert_eq!(state.lifecycle(panel), Some(super::PanelLifecycle::Active));
    assert!(!state.is_stale(panel).must("expected value"));
}

#[test]
fn snapshot_payload_at_limit_is_accepted_and_one_over_is_rejected() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let generation = state.generation(panel).must("expected value");
    let at_limit = empty_snapshot(panel.as_u64(), generation, 1);
    let rev = accept(&mut state, panel, &at_limit, 0, SNAPSHOT_MAX_BYTES).must("at-limit accepted");
    assert_eq!(rev, 1);
    let over = empty_snapshot(panel.as_u64(), generation, 2);
    let error =
        accept(&mut state, panel, &over, 0, SNAPSHOT_MAX_BYTES + 1).must_err("expected failure");
    assert!(matches!(error, PanelError::SnapshotInvalid));
}

// ---------------------------------------------------------------------------
// D. Token bucket: initial burst, refill, fraction, clock regression
// ---------------------------------------------------------------------------

#[test]
fn token_bucket_allows_initial_capacity_then_rejects_one_more() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let generation = state.generation(panel).must("expected value");
    // All at the same injected instant: no refill.
    for rev in 1..=TOKEN_CAPACITY {
        let snapshot = empty_snapshot(panel.as_u64(), generation, rev);
        accept(&mut state, panel, &snapshot, 100, 1).must("within burst");
    }
    let over = empty_snapshot(panel.as_u64(), generation, TOKEN_CAPACITY + 1);
    let error = accept(&mut state, panel, &over, 100, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::RateLimited));
}

#[test]
fn token_bucket_refills_over_time_and_carries_fractional_credit() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let generation = state.generation(panel).must("expected value");
    // Exhaust the initial burst.
    for rev in 1..=TOKEN_CAPACITY {
        let snapshot = empty_snapshot(panel.as_u64(), generation, rev);
        accept(&mut state, panel, &snapshot, 0, 1).must("within burst");
    }
    // 20 tokens/s => after 100ms exactly 2 tokens refilled (fractional credit
    // carried, no rounding loss).
    let after_refill = TOKEN_CAPACITY + 1;
    let snapshot = empty_snapshot(panel.as_u64(), generation, after_refill);
    accept(&mut state, panel, &snapshot, 100, 1).must("first refilled token");
    let snapshot = empty_snapshot(panel.as_u64(), generation, after_refill + 1);
    accept(&mut state, panel, &snapshot, 100, 1).must("second refilled token");
    let snapshot = empty_snapshot(panel.as_u64(), generation, after_refill + 2);
    let error = accept(&mut state, panel, &snapshot, 100, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::RateLimited));
}

#[test]
fn clock_regression_is_invalid() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let generation = state.generation(panel).must("expected value");
    let first = empty_snapshot(panel.as_u64(), generation, 1);
    accept(&mut state, panel, &first, 500, 1).must("first");
    let regressed = empty_snapshot(panel.as_u64(), generation, 2);
    let error = accept(&mut state, panel, &regressed, 499, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::ClockRegression));
}

// ---------------------------------------------------------------------------
// E. Suspend / resume / dispose / late snapshot
// ---------------------------------------------------------------------------

#[test]
fn suspend_from_active_returns_deactivate_reason_suspend() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let effect = state.suspend(panel).must("suspend");
    assert_eq!(effect.reason, DeactivateReason::Suspend);
    assert_eq!(effect.panel_instance, panel);
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Suspended)
    );
    assert!(
        state.accepted_snapshot(panel).is_none(),
        "model dropped on suspend"
    );
}

#[test]
fn suspend_from_failed_is_allowed() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let first = accept_ok(&mut state, panel);
    let oversize = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        first + 1,
    );
    let _ = accept(&mut state, panel, &oversize, 0, SNAPSHOT_MAX_BYTES + 1);
    let effect = state.suspend(panel).must("suspend from failed");
    assert_eq!(effect.reason, DeactivateReason::Suspend);
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Suspended)
    );
}

#[test]
fn resume_uses_fresh_generation_and_carries_prior_host_local() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    state
        .update_host_local(
            panel,
            HostLocal {
                focus_target: Some(id("vendor.f")),
                scroll_offset: 3,
                selected_id: None,
                form_draft: None,
            },
        )
        .must("host-local update");
    let gen_before = state.generation(panel).must("expected value");
    state.suspend(panel).must("suspend");
    let outcome = state.resume(panel).must("resume");
    assert_eq!(outcome.effect.generation, gen_before + 1);
    let prior = outcome
        .effect
        .prior_host_local
        .as_ref()
        .must("prior host-local carried on resume");
    assert_eq!(prior.scroll_offset, 3);
    assert_eq!(
        state.accepted_revision(panel),
        None,
        "revision reset on resume"
    );
}

#[test]
fn resume_only_from_suspended() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let error = state.resume(panel).must_err("expected failure");
    assert!(matches!(error, PanelError::InvalidLifecycle));
}

#[test]
fn dispose_returns_deactivate_reason_dispose_and_invalidates_instance() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let outcome = state.dispose(panel).must("dispose");
    let DeactivateOutcome::Sent(effect) = outcome else {
        panic!("dispose from active sends a deactivate");
    };
    assert_eq!(effect.reason, DeactivateReason::Dispose);
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Disposed)
    );
}

#[test]
fn replace_returns_deactivate_reason_replace() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    let outcome = state.replace(panel).must("replace");
    let DeactivateOutcome::Sent(effect) = outcome else {
        panic!("replace from active sends a deactivate");
    };
    assert_eq!(effect.reason, DeactivateReason::Replace);
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Disposed)
    );
}

#[test]
fn late_snapshot_cannot_mutate_a_disposed_panel() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    state.dispose(panel).must("dispose");
    let snapshot = empty_snapshot(panel.as_u64(), 1, 1);
    let error = accept(&mut state, panel, &snapshot, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::Disposed));
    assert_eq!(
        state.lifecycle(panel),
        Some(super::PanelLifecycle::Disposed)
    );
}

#[test]
fn late_snapshot_to_suspended_panel_is_rejected() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    accept_ok(&mut state, panel);
    state.suspend(panel).must("suspend");
    let snapshot = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        2,
    );
    let error = accept(&mut state, panel, &snapshot, 0, 1).must_err("expected failure");
    assert!(matches!(error, PanelError::InvalidLifecycle));
}

// ---------------------------------------------------------------------------
// F. HostLocal update bounds and atomicity (N / N+1)
// ---------------------------------------------------------------------------

#[test]
fn host_local_update_only_on_live_or_suspended_instance() {
    let mut state = ProviderPanelState::new();
    let panel = declare(&mut state, 1).instance;
    let host = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: None,
    };
    let error = state
        .update_host_local(panel, host.clone())
        .must_err("expected failure");
    assert!(matches!(error, PanelError::InvalidLifecycle));
}

#[test]
fn host_local_at_limit_is_accepted_and_one_byte_over_is_rejected() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    // Find an N/N+1 boundary around the canonical byte limit. The canonical
    // form of an empty HostLocal is a fixed small object; grow a form-draft
    // field value until the limit is approached exactly.
    let base = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: None,
    };
    let base_size = super::host_local_canonical_bytes(&base);
    assert!(base_size <= HOST_LOCAL_MAX_BYTES);
    state.update_host_local(panel, base).must("empty accepted");
    assert_eq!(
        base_size,
        "{\"scroll_offset\":0}".len(),
        "absent optional host-local fields are omitted"
    );

    // The canonical size grows by exactly one byte per filler character
    // (each `x` is an unescaped ASCII byte), so a single probe fixes the
    // overhead and the exact limit-filling length.
    let mut probe_values = TypedMap::new();
    probe_values.insert(id("vendor.f"), TypedValue::String("x".to_string()));
    let probe = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: Some(probe_values),
    };
    let overhead = super::host_local_canonical_bytes(&probe) - 1;
    let fill_len = HOST_LOCAL_MAX_BYTES - overhead;
    let mut at_values = TypedMap::new();
    at_values.insert(id("vendor.f"), TypedValue::String("x".repeat(fill_len)));
    let at_limit = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: Some(at_values),
    };
    assert_eq!(
        super::host_local_canonical_bytes(&at_limit),
        HOST_LOCAL_MAX_BYTES,
        "fixture lands exactly at the limit"
    );
    state
        .update_host_local(panel, at_limit.clone())
        .must("at-limit accepted");

    let mut over_values = TypedMap::new();
    over_values.insert(id("vendor.f"), TypedValue::String("x".repeat(fill_len + 1)));
    let over = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: Some(over_values),
    };
    let error = state
        .update_host_local(panel, over.clone())
        .must_err("expected failure");
    assert!(matches!(error, PanelError::HostLocalTooLarge));
    // The rejected update did not mutate the retained host-local.
    assert_eq!(state.host_local(panel), Some(&at_limit));
}

#[test]
fn authoritative_snapshot_selection_cannot_exceed_the_host_local_bound() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let mut probe_values = TypedMap::new();
    probe_values.insert(id("vendor.f"), TypedValue::String("x".to_owned()));
    let probe = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: Some(probe_values),
    };
    let fill_len = HOST_LOCAL_MAX_BYTES - super::host_local_canonical_bytes(&probe) + 1;
    let mut values = TypedMap::new();
    values.insert(id("vendor.f"), TypedValue::String("x".repeat(fill_len)));
    let retained = HostLocal {
        focus_target: None,
        scroll_offset: 0,
        selected_id: None,
        form_draft: Some(values),
    };
    assert_eq!(super::host_local_canonical_bytes(&retained), HOST_LOCAL_MAX_BYTES);
    state
        .update_host_local(panel, retained.clone())
        .must("at-limit host local accepted");
    let generation = state.generation(panel).must("generation known");
    let mut snapshot = list_snapshot(panel.as_u64(), generation, 1, &["vendor.selection"]);
    let PanelBody::List(list) = &mut snapshot.body else {
        panic!("list fixture");
    };
    list.selected_id = Some(id("vendor.selection"));

    let result = accept(&mut state, panel, &snapshot, 0, 1);

    assert_eq!(result, Err(PanelError::HostLocalTooLarge));
    assert_eq!(state.host_local(panel), Some(&retained));
    assert!(state.accepted_snapshot(panel).is_none());
    assert_eq!(state.lifecycle(panel), Some(PanelLifecycle::Failed));
}

#[test]
fn host_local_canonical_bytes_is_pure_and_deterministic() {
    let a = HostLocal {
        focus_target: Some(id("vendor.f")),
        scroll_offset: 12,
        selected_id: Some(id("vendor.s")),
        form_draft: None,
    };
    let b = a.clone();
    assert_eq!(
        super::host_local_canonical_bytes(&a),
        super::host_local_canonical_bytes(&b)
    );
    assert!(super::host_local_canonical_bytes(&a) > 0);
}

// ---------------------------------------------------------------------------
// G. Errors are redaction-safe and carry PLG-E502 semantics
// ---------------------------------------------------------------------------

#[test]
fn panel_error_display_carries_plg_code_and_no_values() {
    let display = PanelError::OwnerMismatch.to_string();
    assert!(
        display.contains("PLG-E502"),
        "error must carry PLG-E502: {display}"
    );
    assert!(
        !display.contains("vendor.pkg"),
        "error must not echo the offending value: {display}"
    );
}

#[test]
fn accept_outcome_revision_matches_accepted_revision() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let revision = accept_ok(&mut state, panel);
    assert_eq!(state.accepted_revision(panel), Some(revision));
}

// ---------------------------------------------------------------------------
// H. Snapshot N/N+1 body-count boundary
// ---------------------------------------------------------------------------

#[test]
fn snapshot_body_count_boundary_accepts_incremental_revisions() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let generation = state.generation(panel).must("expected value");
    // Two consecutive revisions of the same list body; the second adds an item.
    let one = list_snapshot(panel.as_u64(), generation, 1, &["vendor.a"]);
    let two = list_snapshot(panel.as_u64(), generation, 2, &["vendor.a", "vendor.b"]);
    assert_eq!(
        accept(&mut state, panel, &one, 0, 1).must("expected value"),
        1
    );
    assert_eq!(
        accept(&mut state, panel, &two, 0, 1).must("expected value"),
        2
    );
}

fn list_snapshot(panel: u64, generation: u64, rev: u64, item_ids: &[&str]) -> PanelSnapshot {
    let items: Vec<ListItem> = item_ids
        .iter()
        .map(|item_id| ListItem {
            id: id(item_id),
            label: (*item_id).to_string(),
            description: None,
            status: None,
            actions: vec![],
        })
        .collect();
    PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel,
        generation,
        revision: rev,
        kind: BodyKind::List,
        title: "list".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![],
        body: PanelBody::List(ListBody {
            items,
            selected_id: None,
            next_page_token: None,
        }),
    }
}

// ---------------------------------------------------------------------------
// I. Retry event from failed emits a fresh Activate (cross-check with events)
// ---------------------------------------------------------------------------

#[test]
fn retry_event_from_failed_emits_fresh_activate() {
    let mut state = ProviderPanelState::new();
    let panel = first_live_panel(&mut state);
    let first = accept_ok(&mut state, panel);
    let oversize = empty_snapshot(
        panel.as_u64(),
        state.generation(panel).must("expected value"),
        first + 1,
    );
    let _ = accept(&mut state, panel, &oversize, 0, SNAPSHOT_MAX_BYTES + 1);
    let generation = state.generation(panel).must("expected value");
    let outcome = state
        .submit_event(SubmitEvent {
            panel,
            owner: &owner(),
            received_process_generation: 1,
            generation,
            revision: first,
            event: PanelEvent::Retry,
            allowed_events: &[EventDeclaration {
                kind: EventKind::Retry,
                arguments: vec![],
            }],
        })
        .must("retry event processed");
    assert!(
        matches!(outcome, super::EventOutcome::Activate(_)),
        "retry from failed emits Activate"
    );
    assert_eq!(
        state.generation(panel).must("expected value"),
        generation + 1
    );
}

#[test]
fn snapshot_kind_not_declared_by_manifest_fails_without_applying_model() {
    let mut state = ProviderPanelState::default();
    let declared = state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id: &PanelId::from_static("main"),
            screen_instance_id: 7,
            panel_type: &panel_type(),
            activation: &empty_activation(),
            allowed_model_kinds: &[BodyKind::List],
            allowed_events: &[],
            action_authority: &[],
            process_generation: 1,
        })
        .must("declare succeeds");
    let activated = state.activate(declared.instance).must("activate succeeds");
    let snapshot = empty_snapshot(declared.instance.as_u64(), activated.effect.generation, 1);

    let result = state.accept_snapshot(AcceptSnapshot {
        owner: &owner(),
        received_process_generation: 1,
        snapshot: &snapshot,
        payload_byte_count: 1,
        elapsed_ms: 0,
    });

    assert_eq!(result, Err(PanelError::SnapshotInvalid));
    assert_eq!(
        state.lifecycle(declared.instance),
        Some(PanelLifecycle::Failed)
    );
    assert!(state.accepted_snapshot(declared.instance).is_none());
}

#[test]
fn snapshot_body_kind_must_match_its_declared_kind_before_commit() {
    let mut state = ProviderPanelState::default();
    let declared = state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id: &PanelId::from_static("main"),
            screen_instance_id: 7,
            panel_type: &panel_type(),
            activation: &empty_activation(),
            allowed_model_kinds: &[BodyKind::List],
            allowed_events: &[],
            action_authority: &[],
            process_generation: 1,
        })
        .must("declare succeeds");
    let activated = state.activate(declared.instance).must("activate succeeds");
    let mut snapshot = empty_snapshot(declared.instance.as_u64(), activated.effect.generation, 1);
    snapshot.kind = BodyKind::List;

    let result = state.accept_snapshot(AcceptSnapshot {
        owner: &owner(),
        received_process_generation: 1,
        snapshot: &snapshot,
        payload_byte_count: 1,
        elapsed_ms: 0,
    });

    assert_eq!(result, Err(PanelError::SnapshotInvalid));
    assert!(state.accepted_snapshot(declared.instance).is_none());
}

// ---------------------------------------------------------------------------
// J. Snapshot affordance action-id authority
// ---------------------------------------------------------------------------

fn declare_with_authority(
    state: &mut ProviderPanelState,
    authority: &[&str],
) -> DeclareOutcome {
    let action_authority = authority
        .iter()
        .map(|id| {
            crate::domain::action_registry::ActionId::parse(id)
                .unwrap_or_else(|error| panic!("action id {id:?}: {error:?}"))
        })
        .collect::<Vec<_>>();
    state
        .declare(DeclareInput {
            owner: &owner(),
            panel_id: &PanelId::from_static("main"),
            screen_instance_id: 7,
            panel_type: &panel_type(),
            activation: &empty_activation(),
            allowed_model_kinds: &[BodyKind::Empty],
            allowed_events: &[],
            action_authority: &action_authority,
            process_generation: 1,
        })
        .must("declare succeeds")
}

fn snapshot_with_affordance(
    panel: u64,
    generation: u64,
    action_id: &str,
    enabled: bool,
    reason: Option<&str>,
) -> PanelSnapshot {
    use crate::runtime::provider::protocol::Affordance;
    PanelSnapshot {
        model_schema: MODEL_SCHEMA,
        panel_instance_id: panel,
        generation,
        revision: 1,
        kind: BodyKind::Empty,
        title: "t".to_string(),
        description: None,
        loading: false,
        action_affordances: vec![Affordance {
            id: id("affordance"),
            label: "Action".to_owned(),
            action_id: crate::domain::action_registry::ActionId::parse(action_id)
                .unwrap_or_else(|error| panic!("action id: {error:?}")),
            arguments: None,
            enabled,
            unavailable_reason: reason.map(ToOwned::to_owned),
        }],
        body: PanelBody::Empty(EmptyBody {
            message: "m".to_owned(),
            action: None,
        }),
    }
}

include!("provider_panels_rejection_tests.rs");
