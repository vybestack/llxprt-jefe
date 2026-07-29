//! Normalized server request and canonical reducer interaction checks.

use super::harness::FakeClock;
use crate::domain::observation::ObservationIdentity;
use crate::jsp::v1::JspCode;

use super::dto::{
    DocumentWire, MethodWire, RejectionReasonWire, ResponseBodyWire, ResponseKindWire, RoleWire,
    RouteWire, ServerInteractionWire, ServerRequestWire, ServerResponseWire, TypedDocument,
};
use super::profile::ProfileError;
use super::projection::NormalizedProjection;
use super::reducer::ReducerError;
use super::server_profile::{
    ServerState, check_forbidden, finding_at, native_state_equal_except_health, pending_unchanged,
    reject_unchanged, rejection_reason, response_matches,
};
use super::server_profile_stream::{
    LeaseInteraction, StreamInteraction, check_lease, check_stream,
};

pub(super) fn check_interaction(
    interaction: ServerInteractionWire,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let request = interaction.request;
    let Some(identity) = request_identity(&request, index, findings) else {
        return;
    };
    if route_immediate_observation(
        &request,
        &identity,
        interaction.response.as_ref(),
        index,
        state,
        findings,
    ) {
        return;
    }
    let authorization = authorize(&request, &identity, state);
    let role = match authorization {
        Authorization::Authorized(role) => role,
        Authorization::Unknown | Authorization::BindingMismatch if !state.strict_challenge => {
            check_forbidden(interaction.response, index, request.route, state, findings);
            return;
        }
        Authorization::Unknown => {
            check_auth_rejection(
                interaction.response,
                401,
                ResponseKindWire::UnknownAuthentication,
                index,
                state,
                findings,
            );
            return;
        }
        Authorization::BindingMismatch => {
            check_auth_rejection(
                interaction.response,
                403,
                ResponseKindWire::ForbiddenBinding,
                index,
                state,
                findings,
            );
            return;
        }
    };
    dispatch(
        AuthorizedInteraction {
            request,
            identity,
            response: interaction.response,
            stream: interaction.stream,
            index,
            role,
        },
        state,
        findings,
    );
}
fn request_identity(
    request: &ServerRequestWire,
    index: usize,
    findings: &mut Vec<ProfileError>,
) -> Option<ObservationIdentity> {
    if let Ok(identity) = request.identity() {
        Some(identity)
    } else {
        finding_at(
            findings,
            "request_identity",
            index,
            "request",
            "identity validation failed",
        );
        None
    }
}

fn route_immediate_observation(
    request: &ServerRequestWire,
    identity: &ObservationIdentity,
    response: Option<&ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) -> bool {
    if state.strict_challenge
        && !state.pending_observations.is_empty()
        && request.route != RouteWire::ObservationDigest
    {
        finding_at(
            findings,
            "immediate_post_observation",
            index,
            "request",
            "rejection observation was not immediate",
        );
        return true;
    }
    if request.route != RouteWire::ObservationDigest {
        return false;
    }
    check_observation_digest_ref(request, identity, response, index, state, findings);
    true
}

struct AuthorizedInteraction {
    request: ServerRequestWire,
    identity: ObservationIdentity,
    response: Option<ServerResponseWire>,
    stream: Option<Vec<super::dto::StreamItemWire>>,
    index: usize,
    role: RoleWire,
}

fn dispatch(
    interaction: AuthorizedInteraction,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let AuthorizedInteraction {
        request,
        identity,
        response,
        stream,
        index,
        role,
    } = interaction;
    match (request.route, role, request.method) {
        (RouteWire::Register, RoleWire::Publisher, MethodWire::Post) => {
            check_register(request, identity, response, index, state, findings);
        }
        (RouteWire::Publish, RoleWire::Publisher, MethodWire::Post) => {
            check_publish(request, identity, response, index, state, findings);
        }
        (RouteWire::Heartbeat, RoleWire::Publisher, MethodWire::Post) => {
            check_heartbeat(request, identity, response, index, state, findings);
        }
        (RouteWire::Observe, RoleWire::Observer, MethodWire::Get) => check_stream(
            StreamInteraction {
                request,
                identity,
                response,
                stream,
                index,
            },
            state,
            findings,
        ),
        (RouteWire::LeaseExpired, RoleWire::Server, MethodWire::Post) => check_lease(
            LeaseInteraction {
                request,
                identity,
                response,
                index,
            },
            state,
            findings,
        ),
        (RouteWire::ObservationDigest, _, _) => finding_at(
            findings,
            "method_role_route",
            index,
            "request",
            "observation digest must be handled before dispatch",
        ),
        (RouteWire::Publish | RouteWire::Control, RoleWire::Observer, MethodWire::Post)
        | (RouteWire::Observe, RoleWire::Publisher, MethodWire::Get) => {
            check_forbidden(response, index, request.route, state, findings);
        }
        _ => finding_at(
            findings,
            "method_role_route",
            index,
            "request",
            "route/method/trusted-role combination is invalid",
        ),
    }
}

/// The outcome of authenticating a server request against the trusted
/// credential table. See [`authorize`] for the authentication policy that
/// produces each variant.
enum Authorization {
    /// The credential handle and principal handle matched a trusted
    /// credential bound to the request identity, granting the credential's
    /// role.
    Authorized(RoleWire),
    /// No trusted credential matched the supplied handles.
    Unknown,
    /// A trusted credential matched the handles but its identity binding
    /// disagreed with the request identity (a genuinely unrelated principal).
    BindingMismatch,
}

/// Authenticate by credential handle and principal handle, returning the
/// granted role. A credential bound to a different **agent_id** is rejected
/// (forbidden) because it represents a genuinely unrelated principal.
///
/// A stale generation or epoch for the **same** agent is still authenticated:
/// the credential is valid for that agent, and the reducer-level
/// [`check_stale_identity`] handles the 409 rejection. This separation is
/// critical so a publisher with a stale identity receives a typed rejection
/// rather than a role-mismatch forbidden.
fn authorize(
    request: &ServerRequestWire,
    identity: &ObservationIdentity,
    state: &ServerState,
) -> Authorization {
    let Some(credential) = state.credentials.iter().find(|credential| {
        credential.credential_handle == request.credential_handle
            && credential.principal_handle == request.principal_handle
    }) else {
        return Authorization::Unknown;
    };
    if credential.binding.as_ref() != Some(identity) {
        if !state.strict_challenge
            && credential
                .binding
                .as_ref()
                .is_some_and(|binding| binding.agent_id == identity.agent_id)
        {
            return Authorization::Authorized(credential.role);
        }
        return Authorization::BindingMismatch;
    }
    Authorization::Authorized(credential.role)
}

fn check_auth_rejection(
    response: Option<ServerResponseWire>,
    status: u16,
    kind: ResponseKindWire,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let before = state.reducer.projection();
    if response_matches(response.as_ref(), status, kind) {
        if kind == ResponseKindWire::ForbiddenBinding {
            state.proved |= ServerState::PUBLISHER_OBSERVE;
        } else if kind == ResponseKindWire::UnknownAuthentication {
            state.proved |= ServerState::UNKNOWN_AUTH;
        }
        pending_unchanged(state, before);
    } else {
        finding_at(
            findings,
            "authentication",
            index,
            "request",
            "authentication response mismatch",
        );
    }
}

fn check_observation_digest_ref(
    request: &ServerRequestWire,
    identity: &ObservationIdentity,
    response: Option<&ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let authorized_server = matches!(
        authorize(request, identity, state),
        Authorization::Authorized(RoleWire::Server)
    );
    let observed: Option<NormalizedProjection> = request
        .body
        .as_ref()
        .and_then(|body| serde_json::from_str(body.get()).ok());
    let expected = state.pending_observations.first();
    let projection_matches = observed
        .as_ref()
        .zip(expected)
        .is_some_and(|(observed, expected)| {
            native_state_equal_except_health(observed, expected)
                || state
                    .last_observed_projection
                    .as_ref()
                    .is_some_and(|projection| {
                        native_state_equal_except_health(observed, projection)
                            && native_state_equal_except_health(expected, projection)
                    })
        });
    if authorized_server
        && request.method == MethodWire::Post
        && projection_matches
        && response_matches(response, 200, ResponseKindWire::CanonicalObservation)
    {
        state.pending_observations.clear();
    } else {
        let detail = if !authorized_server {
            "digest principal is not the trusted server"
        } else if !projection_matches {
            "digest projection differs from rejected-operation observation"
        } else {
            "digest method or response mismatch"
        };
        finding_at(
            findings,
            "immediate_post_observation",
            index,
            "digest",
            detail,
        );
    }
}

fn check_register(
    request: ServerRequestWire,
    identity: ObservationIdentity,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    // A duplicate registration (the stream is already bound) is a distinct
    // invariant from a malformed or identity-mismatching first registration.
    // Split them so a transcript author can tell which contract was violated.
    if state.registered.is_some() {
        finding_at(
            findings,
            "duplicate_registration",
            index,
            "request",
            "registration occurred after the stream was already bound",
        );
        return;
    }
    let response_valid = response.as_ref().is_some_and(|response| {
        response.status == 201
            && response.kind == ResponseKindWire::Registered
            && matches!(&response.body, Some(ResponseBodyWire::Binding(binding)) if binding.identity().as_ref() == Ok(&identity))
    });
    if request.body.is_some() || !response_valid {
        finding_at(
            findings,
            "registration_identity",
            index,
            "request",
            "registration binding invariant failed",
        );
        return;
    }
    state.registered = Some(identity);
    state.proved |= ServerState::REGISTER;
}

fn check_publish(
    request: ServerRequestWire,
    identity: ObservationIdentity,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    if state.registered.as_ref() != Some(&identity) {
        check_stale_identity(&identity, response, index, state, findings);
        return;
    }
    let Some(raw) = request.body else {
        finding_at(
            findings,
            "publish_body",
            index,
            "document",
            "publish document is missing",
        );
        return;
    };
    match DocumentWire::from_raw(raw).into_typed() {
        Ok(TypedDocument::Snapshot(snapshot)) => {
            check_snapshot(*snapshot, identity, response, index, state, findings);
        }
        Ok(TypedDocument::Event(event)) => {
            if event.identity == identity {
                check_event(event, response, index, state, findings);
            } else {
                reject_unchanged(
                    state,
                    findings,
                    index,
                    "document_identity",
                    "event identity differs from request binding",
                );
            }
        }
        Ok(TypedDocument::Heartbeat(_)) => finding_at(
            findings,
            "publish_body",
            index,
            "heartbeat",
            "heartbeat must use heartbeat route",
        ),
        Err(error) => check_parser_rejection(error, response.as_ref(), index, state, findings),
    }
}

fn check_snapshot(
    snapshot: crate::jsp::Snapshot,
    identity: ObservationIdentity,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    if snapshot.identity != identity {
        reject_unchanged(
            state,
            findings,
            index,
            "document_identity",
            "snapshot identity differs from request binding",
        );
        return;
    }
    let fresh = state.reducer.fresh_snapshot_required(&identity);
    let expected = if fresh {
        ResponseKindWire::FreshSnapshotAccepted
    } else {
        ResponseKindWire::Accepted
    };
    if !response_matches(response.as_ref(), 200, expected) {
        finding_at(
            findings,
            "snapshot_status",
            index,
            "snapshot",
            "snapshot response mismatch",
        );
        return;
    }
    state.reducer.apply_snapshot(&snapshot);
    state.accepted_states.push(state.reducer.projection());
    state.proved |= ServerState::SNAPSHOT;
    if fresh {
        // A fresh snapshot atomically replaces the canonical state, so all
        // pre-rejection pending observations from the gap/stale era are
        // superseded. Without this, a subsequent observe stream would
        // compare post-snapshot state against stale pre-rejection snapshots.
        state.pending_observations.clear();
        state.proved |= ServerState::GAP_FRESH;
    }
}
struct NoopCheck<'a> {
    response: Option<&'a ServerResponseWire>,
    kind: ResponseKindWire,
    index: usize,
    before: NormalizedProjection,
    bit: u32,
}

struct AcceptedEvent<'a> {
    sequence: u64,
    last: u64,
    response: Option<&'a ServerResponseWire>,
    index: usize,
    before: NormalizedProjection,
}

fn check_event(
    event: crate::domain::observation::EventRecord,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let before = state.reducer.projection();
    let last = before.last_sequence;
    match state.reducer.apply_event(&event) {
        Ok(()) => check_accepted_event(
            AcceptedEvent {
                sequence: event.source_sequence,
                last,
                response: response.as_ref(),
                index,
                before,
            },
            state,
            findings,
        ),
        Err(error) => {
            check_rejected_event(error, response.as_ref(), index, before, state, findings);
        }
    }
}

fn check_accepted_event(
    event: AcceptedEvent<'_>,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    if event.sequence == event.last.saturating_add(1) {
        if response_matches(event.response, 200, ResponseKindWire::Accepted) {
            state.accepted_states.push(state.reducer.projection());
            state.proved |= ServerState::EVENT;
        } else {
            finding_at(
                findings,
                "contiguous_event",
                event.index,
                "event",
                "accepted event response mismatch",
            );
        }
        return;
    }
    let (kind, bit) = if event.sequence == event.last {
        (ResponseKindWire::DuplicateNoop, ServerState::DUPLICATE)
    } else {
        (ResponseKindWire::OutOfOrderNoop, ServerState::OUT_OF_ORDER)
    };
    check_noop(
        NoopCheck {
            response: event.response,
            kind,
            index: event.index,
            before: event.before,
            bit,
        },
        state,
        findings,
    );
}

fn check_rejected_event(
    error: ReducerError,
    response: Option<&ServerResponseWire>,
    index: usize,
    before: NormalizedProjection,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let (reason, invariant, detail) = match error {
        ReducerError::Gap { expected, actual } => (
            RejectionReasonWire::SequenceGapFreshSnapshotRequired,
            "gap_no_mutation",
            format!("expected cursor {expected} actual {actual}"),
        ),
        ReducerError::FreshSnapshotRequired => (
            RejectionReasonWire::FreshSnapshotRequired,
            "gap_fresh_snapshot",
            "event was not rejected unchanged pending fresh snapshot".to_string(),
        ),
        error => {
            finding_at(findings, "event_transition", index, "event", error.code());
            return;
        }
    };
    let unchanged = native_state_equal_except_health(&before, &state.reducer.projection());
    if response_matches(
        response,
        409,
        ResponseKindWire::GapRejectedFreshStreamRequired,
    ) && rejection_reason(response) == Some(reason)
        && unchanged
    {
        pending_unchanged(state, before);
    } else {
        finding_at(findings, invariant, index, "event", &detail);
    }
}

fn check_noop(check: NoopCheck<'_>, state: &mut ServerState, findings: &mut Vec<ProfileError>) {
    if response_matches(check.response, 200, check.kind)
        && state.reducer.projection() == check.before
    {
        state.proved |= check.bit;
        pending_unchanged(state, check.before);
    } else {
        finding_at(
            findings,
            "no_op_mutation",
            check.index,
            "event",
            "duplicate/lower operation changed canonical state or response",
        );
    }
}

fn check_stale_identity(
    request: &ObservationIdentity,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let before = state.reducer.projection();
    let Some(bound) = &state.registered else {
        finding_at(
            findings,
            "registration_identity",
            index,
            "request",
            "publish occurred before registration",
        );
        return;
    };
    let (kind, reason, bit) = if request.agent_id != bound.agent_id {
        (
            ResponseKindWire::UnrelatedAgentRejected,
            RejectionReasonWire::UnrelatedAgent,
            ServerState::UNRELATED,
        )
    } else if request.lifecycle_generation != bound.lifecycle_generation {
        (
            ResponseKindWire::StaleGenerationRejected,
            RejectionReasonWire::StaleLifecycleGeneration,
            ServerState::STALE_GENERATION,
        )
    } else {
        (
            ResponseKindWire::StaleEpochRejected,
            RejectionReasonWire::StaleSourceEpoch,
            ServerState::STALE_EPOCH,
        )
    };
    if response_matches(response.as_ref(), 409, kind)
        && rejection_reason(response.as_ref()) == Some(reason)
        && state.reducer.projection() == before
    {
        state.proved |= bit;
        pending_unchanged(state, before);
    } else {
        finding_at(
            findings,
            "stale_identity_no_mutation",
            index,
            "request",
            "identity class/response/no-mutation invariant failed",
        );
    }
}

fn check_heartbeat(
    request: ServerRequestWire,
    identity: ObservationIdentity,
    response: Option<ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let Some(raw) = request.body else {
        finding_at(
            findings,
            "heartbeat_lease",
            index,
            "heartbeat",
            "heartbeat document is missing",
        );
        return;
    };
    let Ok(TypedDocument::Heartbeat(heartbeat)) = DocumentWire::from_raw(raw).into_typed() else {
        finding_at(
            findings,
            "heartbeat_lease",
            index,
            "heartbeat",
            "heartbeat parser rejected document",
        );
        return;
    };
    // Validate monotonicity before touching the reducer: a non-monotonic
    // heartbeat must never mutate reducer health, even on a probe.
    if state
        .last_heartbeat_ms
        .is_some_and(|last| heartbeat.bridge_observed_ms <= last)
    {
        finding_at(
            findings,
            "heartbeat_lease",
            index,
            "heartbeat",
            "heartbeat time is not monotonic",
        );
        return;
    }
    let before = state.reducer.projection();
    if heartbeat.identity != identity
        || state.registered.as_ref() != Some(&identity)
        || !response_matches(response.as_ref(), 200, ResponseKindWire::Accepted)
        || state.reducer.apply_heartbeat(&heartbeat).is_err()
        || !native_state_equal_except_health(&before, &state.reducer.projection())
    {
        finding_at(
            findings,
            "heartbeat_lease",
            index,
            "heartbeat",
            "heartbeat binding/status/state invariant failed",
        );
        return;
    }
    state.last_heartbeat_ms = Some(heartbeat.bridge_observed_ms);
    state.clock = Some(FakeClock::new(heartbeat.bridge_observed_ms));
    state.proved |= ServerState::HEARTBEAT;
}
fn check_parser_rejection(
    error: crate::jsp::v1::JspError,
    response: Option<&ServerResponseWire>,
    index: usize,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let before = state.reducer.projection();
    if error.code() == JspCode::EBound
        && response_matches(response, 413, ResponseKindWire::BoundExceeded)
        && rejection_reason(response) == Some(RejectionReasonWire::PayloadExceedsBound)
    {
        state.proved |= ServerState::BOUND;
        pending_unchanged(state, before);
    } else {
        finding_at(findings, "bounds", index, "document", error.code().as_str());
    }
}
