//! Built-in deterministic reference adapter (Slice B).
//!
//! This module is the checked-in self-test adapter: it reads a challenge JSON
//! object from its input and produces a deterministic producer or credentials.server trace
//! that satisfies the challenge parameters. It is used by integration tests and
//! the CLI `--reference-adapter` flag to prove the challenge/response protocol
//! end-to-end without external dependencies.
//!
//! The reference adapter is NOT the same as a captured trace fixture. It
//! generates a fresh trace bound to the runner's nonce, marker, clock
//! sequence, identity, and capacity parameters on each call. A replayed trace
//! from a different nonce cannot pass because the nonce is embedded in the
//! adapter's observed output.

use serde::{Deserialize, Serialize};

use super::challenge::{AdapterKind, CredentialChallenge, RunnerChallenge};

/// Internal convenience view of the closed runner challenge.
#[derive(Debug, Clone)]
pub struct ReferenceChallenge {
    pub nonce: u64,
    pub kind: AdapterKind,
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
    pub pid: u32,
    pub started_at_ms: u64,
    pub redaction_marker: String,
    pub clock_sequence: Vec<u64>,
    pub queue_capacity: u64,
    pub queue_deadline_ms: u64,
    pub queue_operations: u64,
    pub gap_emitted_through: u64,
    pub gap_dropped_start: u64,
    pub gap_dropped_end: u64,
    pub gap_next_emitted: u64,
    pub draft_marker: String,
    pub redaction_source_handle: String,
    pub draft_source_handle: String,
    pub sink_operation_handle: String,
    pub sink_operations: Vec<String>,
    pub gap_operation_handle: String,
    pub adapter_version: String,
    pub trusted_credentials: Vec<CredentialChallenge>,
}

impl From<RunnerChallenge> for ReferenceChallenge {
    fn from(challenge: RunnerChallenge) -> Self {
        Self {
            nonce: challenge.nonce,
            kind: challenge.kind,
            agent_id: challenge.launch.identity.agent_id,
            lifecycle_generation: challenge.launch.identity.lifecycle_generation,
            source_epoch: challenge.launch.identity.source_epoch,
            pid: challenge.launch.pid,
            started_at_ms: challenge.launch.started_at_ms,
            redaction_marker: challenge.redaction.marker,
            clock_sequence: challenge.clock_sequence,
            queue_capacity: challenge.sink.capacity,
            queue_deadline_ms: challenge.sink.deadline_ms,
            queue_operations: challenge.sink.operations.len() as u64,
            gap_emitted_through: challenge.gap.emitted_through,
            gap_dropped_start: challenge.gap.dropped_start,
            gap_dropped_end: challenge.gap.dropped_end,
            gap_next_emitted: challenge.gap.next_emitted,
            draft_marker: challenge.draft.marker,
            redaction_source_handle: challenge.redaction.source_handle,
            draft_source_handle: challenge.draft.source_handle,
            sink_operation_handle: challenge.sink.operation_handle,
            sink_operations: challenge.sink.operations,
            gap_operation_handle: challenge.gap.operation_handle,
            adapter_version: challenge.adapter_version,
            trusted_credentials: challenge.trusted_credentials,
        }
    }
}

/// Run the reference adapter: parse the challenge, produce a deterministic
/// trace bound to the challenge parameters.
///
/// Returns `None` if the challenge cannot be satisfied (e.g., missing required
/// fields). The caller treats `None` as a nonzero exit.
#[must_use]
pub fn run(challenge_json: &[u8]) -> Option<Vec<u8>> {
    let challenge: RunnerChallenge = serde_json::from_slice(challenge_json).ok()?;
    if !challenge.structurally_valid() {
        return None;
    }
    let challenge = ReferenceChallenge::from(challenge);
    match challenge.kind {
        AdapterKind::Producer => {
            let trace = build_producer_trace(&challenge)?;
            serde_json::to_vec(&trace).ok()
        }
        AdapterKind::Server => {
            let transcript = build_server_transcript_from_fixture(&challenge)?;
            serde_json::to_vec(&transcript).ok()
        }
    }
}

// -----------------------------------------------------------------------
// Shared JSON helpers
// -----------------------------------------------------------------------

fn event_json(
    challenge: &ReferenceChallenge,
    sequence: u64,
    observed_ms: u64,
    event: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "event",
        "schema": 1,
        "agent_id": challenge.agent_id,
        "lifecycle_generation": challenge.lifecycle_generation,
        "source_epoch": challenge.source_epoch,
        "source_sequence": sequence,
        "bridge_observed_ms": observed_ms,
        "event": event,
    })
}

// -----------------------------------------------------------------------
// Producer trace builder
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProducerTraceOutput {
    schema: u64,
    kind: String,
    trace_artifact_version: String,
    adapter_version: String,
    description: String,
    challenge_nonce: u64,
    facts: Vec<serde_json::Value>,
}

fn build_producer_trace(challenge: &ReferenceChallenge) -> Option<ProducerTraceOutput> {
    let mut trace: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../dev-docs/jsp/v1/compliance/traces/producer-trace.json"
    ))
    .ok()?;
    trace["adapter_version"] = serde_json::Value::String(challenge.adapter_version.clone());
    trace["challenge_nonce"] = serde_json::Value::from(challenge.nonce);
    let facts = trace.get_mut("facts")?.as_array_mut()?;
    let mut clock_index = 0_usize;
    let mut active_clock = None;
    let mut redaction_index = None;
    for (index, fact) in facts.iter_mut().enumerate() {
        bind_producer_fixture_fact(
            fact,
            index,
            challenge,
            &mut clock_index,
            &mut active_clock,
            &mut redaction_index,
        )?;
    }
    if clock_index != challenge.clock_sequence.len() {
        return None;
    }
    facts.insert(
        redaction_index?.saturating_add(1),
        serde_json::json!({
            "fact": "draft_challenge",
            "source_handle": challenge.draft_source_handle,
        }),
    );
    serde_json::from_value(trace).ok()
}

fn bind_producer_fixture_fact(
    fact: &mut serde_json::Value,
    index: usize,
    challenge: &ReferenceChallenge,
    clock_index: &mut usize,
    active_clock: &mut Option<u64>,
    redaction_index: &mut Option<usize>,
) -> Option<()> {
    match fact.get("fact").and_then(serde_json::Value::as_str) {
        Some("clock_set") => {
            let now = *challenge.clock_sequence.get(*clock_index)?;
            *clock_index += 1;
            *active_clock = Some(now);
            fact["now_ms"] = serde_json::Value::from(now);
        }
        Some("document") => {
            let document = fact.get_mut("document")?;
            bind_document(document, challenge);
            document["bridge_observed_ms"] = serde_json::Value::from((*active_clock)?);
        }
        Some("redaction_challenge") => {
            fact["document_index"] = serde_json::Value::from(0);
            fact["forbidden_marker"] =
                serde_json::Value::String(challenge.redaction_marker.clone());
            fact["source_handle"] =
                serde_json::Value::String(challenge.redaction_source_handle.clone());
            *redaction_index = Some(index);
        }
        Some("bound_challenge") => {
            bind_document(&mut fact["at_limit"], challenge);
            bind_document(&mut fact["limit_plus_one"], challenge);
        }
        Some("nonblocking_challenge") => bind_sink_fact(fact, challenge)?,
        Some("gap_challenge") => bind_gap_fixture_fact(fact, challenge),
        _ => {}
    }
    Some(())
}

fn bind_sink_fact(fact: &mut serde_json::Value, challenge: &ReferenceChallenge) -> Option<()> {
    fact["queue_capacity"] = serde_json::Value::from(challenge.queue_capacity);
    fact["attempted"] = serde_json::Value::from(challenge.queue_operations);
    fact["accepted"] = serde_json::Value::from(challenge.queue_capacity);
    fact["elapsed_ms"] = serde_json::Value::from(1);
    fact["deadline_ms"] = serde_json::Value::from(challenge.queue_deadline_ms);
    fact["operation_handle"] = serde_json::Value::String(challenge.sink_operation_handle.clone());
    fact["operation_handles"] = serde_json::to_value(&challenge.sink_operations).ok()?;
    Some(())
}

fn bind_gap_fixture_fact(fact: &mut serde_json::Value, challenge: &ReferenceChallenge) {
    fact["operation_handle"] = serde_json::Value::String(challenge.gap_operation_handle.clone());
    fact["emitted_through"] = serde_json::Value::from(challenge.gap_emitted_through);
    fact["dropped_start"] = serde_json::Value::from(challenge.gap_dropped_start);
    fact["dropped_end"] = serde_json::Value::from(challenge.gap_dropped_end);
    fact["next_emitted"] = serde_json::Value::from(challenge.gap_next_emitted);
    fact["next_publication"] = event_json(
        challenge,
        challenge.gap_next_emitted,
        2_500,
        serde_json::json!({"type": "activity.changed", "state": "idle"}),
    );
}

fn bind_document(document: &mut serde_json::Value, challenge: &ReferenceChallenge) {
    if document.get("agent_id").is_some() {
        document["agent_id"] = serde_json::Value::String(challenge.agent_id.clone());
        document["lifecycle_generation"] = serde_json::Value::from(challenge.lifecycle_generation);
        document["source_epoch"] = serde_json::Value::String(challenge.source_epoch.clone());
    }
    if document.get("process_binding").is_some() {
        document["process_binding"]["value"]["pid"] = serde_json::Value::from(challenge.pid);
        document["process_binding"]["value"]["started_at_ms"] =
            serde_json::Value::from(challenge.started_at_ms);
    }
}

struct ServerCredentials<'a> {
    main: &'a CredentialChallenge,
    observer: &'a CredentialChallenge,
    server: &'a CredentialChallenge,
    stale_generation: &'a CredentialChallenge,
    stale_epoch: &'a CredentialChallenge,
    unrelated: &'a CredentialChallenge,
}

impl<'a> ServerCredentials<'a> {
    fn from_challenge(challenge: &'a ReferenceChallenge) -> Option<Self> {
        let main = challenge_credential(challenge, super::challenge::ChallengeRole::Publisher, 0)?;
        let observer =
            challenge_credential(challenge, super::challenge::ChallengeRole::Observer, 0)?;
        let server = challenge_credential(challenge, super::challenge::ChallengeRole::Server, 0)?;
        let stale_generation = challenge.trusted_credentials.iter().find(|credential| {
            credential.role == super::challenge::ChallengeRole::Publisher
                && credential.identity.lifecycle_generation != challenge.lifecycle_generation
                && credential.identity.agent_id == challenge.agent_id
        })?;
        let stale_epoch = challenge.trusted_credentials.iter().find(|credential| {
            credential.role == super::challenge::ChallengeRole::Publisher
                && credential.identity.source_epoch != challenge.source_epoch
                && credential.identity.agent_id == challenge.agent_id
        })?;
        let unrelated = challenge.trusted_credentials.iter().find(|credential| {
            credential.role == super::challenge::ChallengeRole::Publisher
                && credential.identity.agent_id != challenge.agent_id
        })?;
        Some(Self {
            main,
            observer,
            server,
            stale_generation,
            stale_epoch,
            unrelated,
        })
    }

    fn select(&self, name: &str) -> &'a CredentialChallenge {
        if name == "observer_attempts_publish"
            || name == "observer_attempts_control"
            || name.starts_with("observe_")
        {
            self.observer
        } else if name == "publisher_observes_unrelated" {
            self.main
        } else if name.contains("lease") {
            self.server
        } else if name.contains("unrelated") {
            self.unrelated
        } else if name.contains("stale_generation") {
            self.stale_generation
        } else if name.contains("stale_epoch") {
            self.stale_epoch
        } else {
            self.main
        }
    }
}

fn build_server_transcript_from_fixture(
    challenge: &ReferenceChallenge,
) -> Option<ServerTranscriptOutput> {
    let mut transcript: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../dev-docs/jsp/v1/compliance/traces/server-transcript.json"
    ))
    .ok()?;
    transcript["server_version"] = serde_json::Value::String(challenge.adapter_version.clone());
    transcript["challenge_nonce"] = serde_json::Value::from(challenge.nonce);
    let interactions = transcript.get_mut("interactions")?.as_array_mut()?;
    let credentials = ServerCredentials::from_challenge(challenge)?;
    let mut transformed = Vec::new();
    for interaction in std::mem::take(interactions) {
        transform_server_interaction(interaction, challenge, &credentials, &mut transformed)?;
    }
    transcript["interactions"] = serde_json::Value::Array(transformed);
    serde_json::from_value(transcript).ok()
}

fn transform_server_interaction(
    mut interaction: serde_json::Value,
    challenge: &ReferenceChallenge,
    credentials: &ServerCredentials<'_>,
    transformed: &mut Vec<serde_json::Value>,
) -> Option<()> {
    let name = interaction.get("name")?.as_str()?.to_string();
    bind_server_interaction(&mut interaction, credentials.select(&name));
    if name == "publisher_observes_unrelated" {
        bind_forbidden_unrelated_observe(&mut interaction, credentials.unrelated);
    }
    if matches!(
        name.as_str(),
        "observe_stream_snapshot_first" | "observe_after_bound_rejection"
    ) {
        retain_snapshot_only(&mut interaction);
    }
    let projection = rejection_projection(&interaction, &name, transformed, challenge);
    transformed.push(interaction.clone());
    if name == "publish_event_1" {
        transformed.push(atomic_tail_interaction(
            challenge,
            credentials.observer,
            &interaction,
        )?);
        transformed.push(unknown_auth_interaction(challenge, &interaction)?);
        let canonical = canonical_projection(transformed, challenge)?;
        transformed.push(observation_digest_interaction(
            challenge,
            credentials.server,
            canonical,
        ));
    }
    if let Some(projection) = projection {
        transformed.push(observation_digest_interaction(
            challenge,
            credentials.server,
            projection,
        ));
    }
    Some(())
}

fn atomic_tail_interaction(
    challenge: &ReferenceChallenge,
    observer: &CredentialChallenge,
    event_interaction: &serde_json::Value,
) -> Option<serde_json::Value> {
    let snapshot = serde_json::from_slice::<serde_json::Value>(include_bytes!(
        "../../../../dev-docs/jsp/v1/compliance/traces/server-transcript.json"
    ))
    .ok()?["interactions"][1]["request"]["body"]
        .clone();
    let mut snapshot = snapshot;
    bind_server_identity(&mut snapshot, &observer.identity);
    let mut event = event_interaction.get("request")?.get("body")?.clone();
    bind_server_identity(&mut event, &observer.identity);
    Some(serde_json::json!({
        "name": "runner_owned_atomic_tail",
        "request": {
            "route": "/jsp/1/observe",
            "credential_handle": observer.credential_handle,
            "principal_handle": observer.principal_handle,
            "method": "GET",
            "agent_id": challenge.agent_id,
            "lifecycle_generation": challenge.lifecycle_generation,
            "source_epoch": challenge.source_epoch
        },
        "stream": [
            {"kind": "snapshot", "document": snapshot},
            {"kind": "event", "document": event}
        ],
        "assert": "atomic earlier cursor plus state-changing contiguous tail"
    }))
}

fn unknown_auth_interaction(
    challenge: &ReferenceChallenge,
    event_interaction: &serde_json::Value,
) -> Option<serde_json::Value> {
    let body = event_interaction.get("request")?.get("body")?.clone();
    Some(serde_json::json!({
        "name": "runner_owned_unknown_authentication",
        "request": {
            "route": "/jsp/1/publish",
            "credential_handle": format!("unknown-credential-{}", challenge.nonce),
            "principal_handle": format!("unknown-principal-{}", challenge.nonce),
            "method": "POST",
            "agent_id": challenge.agent_id,
            "lifecycle_generation": challenge.lifecycle_generation,
            "source_epoch": challenge.source_epoch,
            "body": body
        },
        "response": {"status": 401, "kind": "unknown_authentication"},
        "assert": "unknown authentication is distinct from trusted wrong role"
    }))
}

fn bind_forbidden_unrelated_observe(
    interaction: &mut serde_json::Value,
    unrelated: &CredentialChallenge,
) {
    bind_server_identity(&mut interaction["request"], &unrelated.identity);
    interaction["response"] = serde_json::json!({"status": 403, "kind": "forbidden_binding"});
}

fn rejection_projection(
    interaction: &serde_json::Value,
    name: &str,
    transformed: &[serde_json::Value],
    challenge: &ReferenceChallenge,
) -> Option<serde_json::Value> {
    let kind = interaction.get("response")?.get("kind")?.as_str()?;
    if !is_rejection_kind(kind) {
        return None;
    }
    if name == "publisher_observes_unrelated" {
        return canonical_projection(transformed, challenge).map(|mut projection| {
            projection["last_sequence"] = serde_json::Value::from(1_u64);
            projection["observation_health"] = serde_json::Value::String("live".to_string());
            projection
        });
    }
    if matches!(name, "observer_attempts_control" | "publish_over_bound") {
        return latest_observation_projection(transformed)
            .or_else(|| canonical_projection(transformed, challenge));
    }
    canonical_projection(transformed, challenge)
}

fn is_rejection_kind(kind: &str) -> bool {
    matches!(
        kind,
        "duplicate_noop"
            | "out_of_order_noop"
            | "gap_rejected_fresh_stream_required"
            | "unrelated_agent_rejected"
            | "stale_generation_rejected"
            | "stale_epoch_rejected"
            | "forbidden_role"
            | "forbidden_binding"
            | "bound_exceeded"
    )
}

fn challenge_credential(
    challenge: &ReferenceChallenge,
    role: super::challenge::ChallengeRole,
    skip: usize,
) -> Option<&CredentialChallenge> {
    challenge
        .trusted_credentials
        .iter()
        .filter(|credential| credential.role == role)
        .nth(skip)
}

fn bind_server_interaction(interaction: &mut serde_json::Value, credential: &CredentialChallenge) {
    let request = &mut interaction["request"];
    request["credential_handle"] = serde_json::Value::String(credential.credential_handle.clone());
    request["principal_handle"] = serde_json::Value::String(credential.principal_handle.clone());
    bind_server_identity(request, &credential.identity);
    if request
        .get("body")
        .is_some_and(serde_json::Value::is_object)
    {
        bind_server_identity(&mut request["body"], &credential.identity);
    }
    if interaction.get("stream").is_some() {
        if let Some(items) = interaction["stream"].as_array_mut() {
            for item in items {
                bind_server_identity(&mut item["document"], &credential.identity);
            }
        }
    }
    if interaction
        .get("response")
        .and_then(|response| response.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("registered")
    {
        bind_server_identity(&mut interaction["response"]["body"], &credential.identity);
    }
    if interaction
        .get("response")
        .and_then(|response| response.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("observation_health_stale")
    {
        interaction["response"]["body"]["activity_availability"] =
            serde_json::Value::String("known".to_string());

        interaction["response"]["body"]["activity_provenance"] =
            serde_json::Value::String("authoritative".to_string());
    }
}

fn retain_snapshot_only(interaction: &mut serde_json::Value) {
    if let Some(stream) = interaction
        .get_mut("stream")
        .and_then(serde_json::Value::as_array_mut)
    {
        stream.retain(|item| {
            item.get("kind").and_then(serde_json::Value::as_str) == Some("snapshot")
        });
    }
}

fn bind_server_identity(
    value: &mut serde_json::Value,
    identity: &super::challenge::IdentityChallenge,
) {
    if value.get("agent_id").is_some() {
        value["agent_id"] = serde_json::Value::String(identity.agent_id.clone());
        value["lifecycle_generation"] = serde_json::Value::from(identity.lifecycle_generation);
        value["source_epoch"] = serde_json::Value::String(identity.source_epoch.clone());
    }
}

fn latest_observation_projection(interactions: &[serde_json::Value]) -> Option<serde_json::Value> {
    interactions.iter().rev().find_map(|interaction| {
        let route = interaction.get("request")?.get("route")?.as_str()?;
        if route != "/jsp/1/observe" {
            return None;
        }
        let stream = interaction.get("stream")?.as_array()?;
        let snapshot = stream.first()?.get("document")?;
        let bytes = serde_json::to_vec(snapshot).ok()?;
        let snapshot = crate::jsp::v1::parse_snapshot(&bytes).ok()?;
        let mut reducer = super::reducer::ReferenceReducer::new();
        reducer.apply_snapshot(&snapshot);
        serde_json::to_value(reducer.projection()).ok()
    })
}

fn canonical_projection(
    interactions: &[serde_json::Value],
    challenge: &ReferenceChallenge,
) -> Option<serde_json::Value> {
    let mut reducer = super::reducer::ReferenceReducer::new();
    for interaction in interactions {
        let route = interaction
            .get("request")
            .and_then(|request| request.get("route"))
            .and_then(serde_json::Value::as_str);
        if route == Some("/jsp/1/internal/lease_expired") {
            reducer.mark_observation_stale();
            continue;
        }
        if !matches!(route, Some("/jsp/1/publish" | "/jsp/1/heartbeat")) {
            continue;
        }
        let Some(body) = interaction
            .get("request")
            .and_then(|request| request.get("body"))
        else {
            continue;
        };
        if body.is_null() {
            continue;
        }
        let bytes = serde_json::to_vec(body).ok()?;
        if let Ok(snapshot) = crate::jsp::v1::parse_snapshot(&bytes) {
            reducer.apply_snapshot(&snapshot);
        } else if let Ok(event) = crate::jsp::v1::parse_event(&bytes) {
            let _ = reducer.apply_event(&event);
        } else if let Ok(heartbeat) = crate::jsp::v1::parse_heartbeat(&bytes) {
            let _ = reducer.apply_heartbeat(&heartbeat);
        }
    }
    let projection = reducer.projection();
    if projection.agent_id != challenge.agent_id {
        return None;
    }
    serde_json::to_value(projection).ok()
}

fn observation_digest_interaction(
    challenge: &ReferenceChallenge,
    server: &CredentialChallenge,
    projection: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "name": "runner_owned_observation_digest",
        "request": {
            "route": "/jsp/1/internal/observation_digest",
            "credential_handle": server.credential_handle,
            "principal_handle": server.principal_handle,
            "method": "POST",
            "agent_id": server.identity.agent_id,
            "lifecycle_generation": server.identity.lifecycle_generation,
            "source_epoch": server.identity.source_epoch,
            "body": projection,
        },
        "response": {"status": 200, "kind": "canonical_observation"},
        "assert": format!("trusted digest for runner nonce {}", challenge.nonce),
    })
}

// -----------------------------------------------------------------------
// Server transcript builder
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerTranscriptOutput {
    schema: u64,
    kind: String,
    transcript_artifact_version: String,
    server_version: String,
    description: String,
    challenge_nonce: u64,
    interactions: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_reference_adapter_produces_valid_json() {
        let challenge = RunnerChallenge::reference(AdapterKind::Producer, 42);
        let challenge = serde_json::to_vec(&challenge)
            .unwrap_or_else(|error| panic!("serialize challenge: {error}"));
        let output =
            run(&challenge).unwrap_or_else(|| panic!("producer trace must produce output"));
        assert!(!output.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_slice(&output).unwrap_or_else(|error| panic!("valid json: {error}"));
        assert_eq!(parsed["challenge_nonce"], 42);
        assert_eq!(parsed["schema"], 1);
        assert!(parsed["facts"].as_array().is_some_and(|f| !f.is_empty()));
    }

    #[test]
    fn server_reference_adapter_produces_valid_json() {
        let challenge = RunnerChallenge::reference(AdapterKind::Server, 99);
        let challenge = serde_json::to_vec(&challenge)
            .unwrap_or_else(|error| panic!("serialize challenge: {error}"));
        let output = run(&challenge)
            .unwrap_or_else(|| panic!("credentials.server transcript must produce output"));
        assert!(!output.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_slice(&output).unwrap_or_else(|error| panic!("valid json: {error}"));
        assert_eq!(parsed["challenge_nonce"], 99);
        assert_eq!(parsed["schema"], 1);
        assert!(
            parsed["interactions"]
                .as_array()
                .is_some_and(|i| !i.is_empty())
        );
    }

    #[test]
    fn missing_agent_id_returns_none() {
        let mut challenge = RunnerChallenge::reference(AdapterKind::Producer, 1);
        challenge.launch.identity.agent_id.clear();
        let challenge = serde_json::to_vec(&challenge)
            .unwrap_or_else(|error| panic!("serialize challenge: {error}"));
        assert!(run(&challenge).is_none());
    }
}
