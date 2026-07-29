//! Canonical SSE observation and fake-clock lease checks.

use crate::domain::observation::ObservationIdentity;

use super::dto::{
    HealthValueWire, LeaseRequestWire, ResponseBodyWire, ResponseKindWire, ServerRequestWire,
    ServerResponseWire, StreamItemWire, TypedDocument,
};
use super::profile::ProfileError;
use super::reducer::ReferenceReducer;
use super::server_profile::{
    ServerState, activity_value, finding_at, native_state_equal_except_health,
};

pub(super) struct LeaseInteraction {
    pub request: ServerRequestWire,
    pub identity: ObservationIdentity,
    pub response: Option<ServerResponseWire>,
    pub index: usize,
}

pub(super) fn check_lease(
    interaction: LeaseInteraction,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    if state.registered.as_ref() != Some(&interaction.identity) {
        lease_finding(findings, interaction.index, "lease binding mismatch");
        return;
    }
    let Some(lease) = parse_lease(interaction.request, interaction.index, findings) else {
        return;
    };
    let (Some(mut clock), Some(observed)) = (state.clock, state.last_heartbeat_ms) else {
        lease_finding(
            findings,
            interaction.index,
            "lease challenge occurred before heartbeat",
        );
        return;
    };
    let projection = state.reducer.projection();
    let expected_activity = activity_value(projection.activity);
    let response_valid = interaction.response.as_ref().is_some_and(|response| {
        response.status == 200
            && response.kind == ResponseKindWire::ObservationHealthStale
            && matches!(&response.body, Some(ResponseBodyWire::Health(health))
                if health.observation_health == HealthValueWire::Stale
                    && health.native_activity == expected_activity
                    && (!state.strict_challenge
                        || (health.activity_availability == Some(activity_availability(projection.activity))
                            && health.activity_provenance == Some(projection.activity_provenance))))
    });
    if clock.set_ms(lease.now_ms).is_ok()
        && clock.lease_expired(observed, lease.lease_ms)
        && response_valid
    {
        state.reducer.mark_observation_stale();
        state.clock = Some(clock);
        state.proved |= ServerState::LEASE;
    } else {
        lease_finding(
            findings,
            interaction.index,
            "fake-clock lease or canonical activity response failed",
        );
    }
}

fn activity_availability(
    activity: super::projection::ActivityProjection,
) -> super::projection::AvailabilityProjection {
    match activity {
        super::projection::ActivityProjection::Idle
        | super::projection::ActivityProjection::Thinking
        | super::projection::ActivityProjection::Acting => {
            super::projection::AvailabilityProjection::Known
        }
        super::projection::ActivityProjection::Unsupported => {
            super::projection::AvailabilityProjection::Unsupported
        }
        super::projection::ActivityProjection::Unknown => {
            super::projection::AvailabilityProjection::Unknown
        }
        super::projection::ActivityProjection::Degraded => {
            super::projection::AvailabilityProjection::Degraded
        }
    }
}

fn parse_lease(
    request: ServerRequestWire,
    index: usize,
    findings: &mut Vec<ProfileError>,
) -> Option<LeaseRequestWire> {
    let Some(raw) = request.body else {
        lease_finding(findings, index, "lease challenge is missing");
        return None;
    };
    let Ok(lease) = serde_json::from_str(raw.get()) else {
        lease_finding(findings, index, "lease challenge shape rejected");
        return None;
    };
    Some(lease)
}

fn lease_finding(findings: &mut Vec<ProfileError>, index: usize, detail: &str) {
    finding_at(findings, "observation_health", index, "lease", detail);
}

pub(super) struct StreamInteraction {
    pub request: ServerRequestWire,
    pub identity: ObservationIdentity,
    pub response: Option<ServerResponseWire>,
    pub stream: Option<Vec<StreamItemWire>>,
    pub index: usize,
}

pub(super) fn check_stream(
    interaction: StreamInteraction,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let index = interaction.index;
    let Some((mut observed, tail)) = stream_start(interaction, state, findings) else {
        return;
    };
    let tail_required = state.strict_challenge
        && observed.projection().last_sequence != state.reducer.projection().last_sequence;
    let snapshot_only = tail.is_empty();
    if !apply_stream_tail(tail, index, &mut observed, tail_required, findings)
        || !native_state_equal_except_health(&observed.projection(), &state.reducer.projection())
    {
        stream_finding(
            findings,
            index,
            "tail",
            "SSE tail differs from accepted canonical reducer state",
        );
        return;
    }
    if state
        .pending_observations
        .iter()
        .any(|expected| !native_state_equal_except_health(expected, &state.reducer.projection()))
    {
        finding_at(
            findings,
            "rejection_no_mutation",
            index,
            "snapshot",
            "subsequent observation differs from pre-rejection state",
        );
        return;
    }
    state.last_observed_projection = Some(state.reducer.projection());
    state.pending_observations.clear();
    state.proved |= ServerState::STREAM;
    if state.strict_challenge {
        state.proved |= if snapshot_only {
            ServerState::STREAM_SNAPSHOT_ONLY
        } else {
            ServerState::STREAM_TAIL
        };
    } else {
        state.proved |= ServerState::STREAM_TAIL | ServerState::STREAM_SNAPSHOT_ONLY;
    }
}

fn stream_start(
    interaction: StreamInteraction,
    state: &ServerState,
    findings: &mut Vec<ProfileError>,
) -> Option<(ReferenceReducer, Vec<StreamItemWire>)> {
    if interaction.request.body.is_some()
        || interaction.response.is_some()
        || state.registered.as_ref() != Some(&interaction.identity)
    {
        stream_finding(
            findings,
            interaction.index,
            "stream",
            "observe request contract failed",
        );
        return None;
    }
    let (snapshot, items) = parse_stream_items(interaction.stream, interaction.index, findings)?;
    let mut observed = ReferenceReducer::new();
    observed.apply_snapshot(&snapshot);
    let snapshot_projection = observed.projection();
    let snapshot_is_current =
        native_state_equal_except_health(&snapshot_projection, &state.reducer.projection());
    let snapshot_was_accepted = state
        .accepted_states
        .iter()
        .any(|accepted| native_state_equal_except_health(&snapshot_projection, accepted));
    if snapshot.identity != interaction.identity
        || (!snapshot_is_current && (!state.strict_challenge || !snapshot_was_accepted))
    {
        stream_finding(
            findings,
            interaction.index,
            "snapshot",
            "SSE snapshot is not an accepted atomic cursor state",
        );
        return None;
    }
    if state.strict_challenge && snapshot_is_current && !items.is_empty() {
        stream_finding(
            findings,
            interaction.index,
            "tail",
            "state-changing tail must start from an earlier accepted cursor",
        );
        return None;
    }
    Some((observed, items))
}

fn parse_stream_items(
    stream: Option<Vec<StreamItemWire>>,
    index: usize,
    findings: &mut Vec<ProfileError>,
) -> Option<(Box<crate::jsp::Snapshot>, Vec<StreamItemWire>)> {
    let Some(mut items) = stream else {
        stream_finding(findings, index, "stream", "SSE stream is missing");
        return None;
    };
    if items.is_empty() {
        stream_finding(findings, index, "stream", "SSE stream is empty");
        return None;
    }
    let first = items.remove(0);
    if first.kind != super::dto::DocumentKindWire::Snapshot {
        stream_finding(
            findings,
            index,
            "snapshot",
            "first SSE item is not snapshot",
        );
        return None;
    }
    let Ok(TypedDocument::Snapshot(snapshot)) = first.document.into_typed() else {
        stream_finding(
            findings,
            index,
            "snapshot",
            "SSE snapshot parser rejected document",
        );
        return None;
    };
    Some((snapshot, items))
}

fn apply_stream_tail(
    items: Vec<StreamItemWire>,
    interaction_index: usize,
    observed: &mut ReferenceReducer,
    state_change_required: bool,
    findings: &mut Vec<ProfileError>,
) -> bool {
    if items.len() > 64 {
        stream_finding(
            findings,
            interaction_index,
            "tail",
            "SSE tail exceeds bound",
        );
        return false;
    }
    let mut changed = false;
    for (tail_index, item) in items.into_iter().enumerate() {
        let valid = match item.kind {
            super::dto::DocumentKindWire::Event => {
                let before = observed.projection();
                let applied = matches!(item.document.into_typed(), Ok(TypedDocument::Event(event)) if observed.apply_event(&event).is_ok());
                changed |= applied && observed.projection() != before;
                applied
            }
            super::dto::DocumentKindWire::Heartbeat => {
                matches!(item.document.into_typed(), Ok(TypedDocument::Heartbeat(heartbeat)) if observed.apply_heartbeat(&heartbeat).is_ok())
            }
            super::dto::DocumentKindWire::Snapshot => false,
        };
        if !valid {
            stream_finding(
                findings,
                interaction_index,
                "tail",
                &format!("tail event index {tail_index} is invalid"),
            );
            return false;
        }
    }
    if state_change_required && !changed {
        stream_finding(
            findings,
            interaction_index,
            "tail",
            "SSE tail does not contain a real state-changing contiguous event",
        );
        return false;
    }
    true
}

fn stream_finding(findings: &mut Vec<ProfileError>, index: usize, kind: &str, detail: &str) {
    finding_at(findings, "canonical_snapshot_first", index, kind, detail);
}
