//! Two-instance identity isolation over the real loopback host (issue #522, X3).
//!
//! Two LLxprt instances launched from the *same* repository, branch and
//! directory must not collide, and delayed traffic from a superseded generation
//! or source epoch must not be able to update the instance that replaced it.
//!
//! These are the two failure modes that matter in practice. The instances are
//! indistinguishable by workspace, so identity has to come from the credential
//! binding rather than from anything in the payload; and a process that dies
//! slowly can still have publications in flight when its replacement is already
//! registered.
//!
//! The test drives the real host over a real loopback socket. It is
//! deterministic: no sleeps as synchronisation, no tmux, no spawned agents.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;

use jefe::domain::AgentId;
use jefe::domain::observation::{Availability, FieldState};
use jefe::jsp_host::{CredentialRole, JspHost, PublisherRegistry, PublisherReservation};
use jefe::messages::RuntimeMessage;

/// Both instances report the identical workspace, so nothing in the payload can
/// be used to tell them apart.
const SHARED_REPOSITORY: &str = "vybestack/llxprt-jefe";
const SHARED_PATH: &str = "/Users/dev/src/jefe";

const TOKEN_ONE: &str = "1111111111111111111111111111111a";
const TOKEN_TWO: &str = "2222222222222222222222222222222b";
/// The replacement for the first agent after it is relaunched.
const TOKEN_ONE_REPLACEMENT: &str = "3333333333333333333333333333333c";

fn reservation(
    agent: &str,
    generation: u64,
    registration_id: &str,
    token: &str,
) -> PublisherReservation {
    PublisherReservation {
        agent_id: AgentId(agent.to_string()),
        generation,
        registration_id: registration_id.to_string(),
        publisher_credential: token.to_string(),
        role: CredentialRole::Publisher,
    }
}

fn known_field(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "provenance": "authoritative",
        "availability": "known",
        "value": value
    })
}

/// A snapshot whose only distinguishing content is the todo text and the
/// committed message, so cross-contamination is directly observable.
fn snapshot(
    agent: &str,
    generation: u64,
    epoch: &str,
    sequence: u64,
    marker: &str,
    pid: u64,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "kind": "snapshot",
        "agent_id": agent,
        "lifecycle_generation": generation,
        "source_epoch": epoch,
        "source_sequence": sequence,
        "cursor": sequence,
        "bridge_observed_ms": 1000,
        "native_session": {
            "repository": SHARED_REPOSITORY,
            "path": SHARED_PATH,
            "agent_kind": "llxprt",
            "pid": pid,
            "display_name": "worker"
        },
        "process_binding": known_field(
            serde_json::json!({"pid": pid, "started_at_ms": 1000}),
        ),
        "native_activity": known_field(serde_json::json!({"state": "idle"})),
        "current_wait": known_field(serde_json::Value::Null),
        "current_turn": known_field(serde_json::Value::Null),
        "todos": known_field(serde_json::json!({
            "revision": 1,
            "items": [{"text": marker, "state": "in_progress"}]
        })),
        "last_displayed_assistant_message": known_field(serde_json::json!({
            "content": marker,
            "committed_ms": 1000
        })),
        "last_created_tool_call": known_field(
            serde_json::json!({"label": "Read", "phase": "succeeded"}),
        ),
        "source_terminal_state": known_field(serde_json::Value::Null),
        "source_error_state": "unsupported"
    }))
    .unwrap_or_else(|error| panic!("serialize snapshot: {error}"))
}

fn request(
    addr: SocketAddr,
    route: &str,
    token: &str,
    registration_id: &str,
    body: &[u8],
) -> String {
    // Reject header-unsafe values rather than letting them split the request.
    for value in [token, registration_id] {
        assert!(
            !value.contains('\r') && !value.contains('\n'),
            "header value must not contain CR or LF"
        );
    }
    let mut stream =
        TcpStream::connect(addr).unwrap_or_else(|error| panic!("connect to host: {error}"));
    let head = format!(
        "POST {route} HTTP/1.1\r\nhost: localhost\r\nauthorization: Bearer {token}\r\njsp-registration-id: {registration_id}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .unwrap_or_else(|error| panic!("write request head: {error}"));
    stream
        .write_all(body)
        .unwrap_or_else(|error| panic!("write request body: {error}"));
    stream
        .flush()
        .unwrap_or_else(|error| panic!("flush request: {error}"));
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .unwrap_or_else(|error| panic!("read response: {error}"));
    response
}

fn serve(
    registry: &PublisherRegistry,
    route: &str,
    token: &str,
    registration_id: &str,
    body: &[u8],
) -> (String, Option<RuntimeMessage>) {
    let host = JspHost::bind(registry.clone()).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let worker = thread::spawn(move || host.serve_once());
    let response = request(addr, route, token, registration_id, body);
    let message = worker
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
    (response, message)
}

/// Extract the agent, generation and todo marker from an observation message.
fn observed(message: Option<RuntimeMessage>) -> (String, u64, String) {
    let Some(RuntimeMessage::ObservationUpdated(agent_id, generation, observation)) = message
    else {
        panic!("an accepted snapshot must emit an observation message");
    };
    let FieldState::Supported { availability, .. } = observation.todos else {
        panic!("todos must be supported");
    };
    let Availability::Known(todos) = availability else {
        panic!("todos must be known");
    };
    let marker = todos
        .items
        .first()
        .unwrap_or_else(|| panic!("snapshot must carry one todo"))
        .text
        .as_str()
        .to_string();
    (agent_id.0, generation, marker)
}

/// Two instances in the same repository, branch and directory keep separate
/// observations; neither one's content can appear under the other's identity.
#[test]
fn two_instances_in_one_workspace_do_not_collide() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation("agent-one", 1, "reg-one", TOKEN_ONE))
        .unwrap_or_else(|error| panic!("reserve first credential: {error}"));
    registry
        .reserve(reservation("agent-two", 1, "reg-two", TOKEN_TWO))
        .unwrap_or_else(|error| panic!("reserve second credential: {error}"));

    let (response, message) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_ONE,
        "reg-one",
        &snapshot("agent-one", 1, "epoch-one", 0, "ONE todo", 1001),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let (agent, generation, marker) = observed(message);
    assert_eq!(agent, "agent-one");
    assert_eq!(generation, 1);
    assert_eq!(marker, "ONE todo");

    let (response, message) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_TWO,
        "reg-two",
        &snapshot("agent-two", 1, "epoch-two", 0, "TWO todo", 1002),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let (agent, generation, marker) = observed(message);
    assert_eq!(agent, "agent-two");
    assert_eq!(generation, 1);
    // The decisive assertion: the second instance's observation carries its own
    // content even though both instances describe an identical workspace.
    assert_eq!(marker, "TWO todo");
}

/// An instance cannot publish under another instance's identity even with a
/// valid credential of its own: the binding is what decides, not the payload.
#[test]
fn one_instance_cannot_publish_as_the_other() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation("agent-one", 1, "reg-one", TOKEN_ONE))
        .unwrap_or_else(|error| panic!("reserve first credential: {error}"));
    registry
        .reserve(reservation("agent-two", 1, "reg-two", TOKEN_TWO))
        .unwrap_or_else(|error| panic!("reserve second credential: {error}"));

    // The second instance's credential, claiming to be the first instance.
    let (response, message) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_TWO,
        "reg-two",
        &snapshot("agent-one", 1, "epoch-two", 0, "spoofed", 1002),
    );
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "claiming another agent must be forbidden, got {response}"
    );
    assert!(
        message.is_none(),
        "a rejected publication must not mutate canonical state"
    );
}

/// Delayed traffic from a superseded generation cannot update the replacement.
#[test]
fn stale_generation_traffic_cannot_update_the_replacement() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation("agent-one", 1, "reg-one", TOKEN_ONE))
        .unwrap_or_else(|error| panic!("reserve original credential: {error}"));

    let (response, _) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_ONE,
        "reg-one",
        &snapshot("agent-one", 1, "epoch-one", 0, "original", 1001),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    // The agent is relaunched: the previous generation is revoked and a new
    // credential is reserved for the replacement.
    registry
        .revoke(&AgentId("agent-one".to_string()), 1)
        .unwrap_or_else(|error| panic!("revoke superseded credential: {error}"));
    registry
        .reserve(reservation(
            "agent-one",
            2,
            "reg-one-replacement",
            TOKEN_ONE_REPLACEMENT,
        ))
        .unwrap_or_else(|error| panic!("reserve replacement credential: {error}"));

    let (response, message) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_ONE_REPLACEMENT,
        "reg-one-replacement",
        &snapshot("agent-one", 2, "epoch-two", 0, "replacement", 1003),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let (_, generation, marker) = observed(message);
    assert_eq!(generation, 2);
    assert_eq!(marker, "replacement");

    // The dying process finally gets its publication out. Its credential is
    // revoked, so it cannot reach the replacement's state.
    let (response, message) = serve(
        &registry,
        "/jsp/1/publish",
        TOKEN_ONE,
        "reg-one",
        &snapshot("agent-one", 1, "epoch-one", 1, "stale", 1001),
    );
    assert!(
        response.starts_with("HTTP/1.1 401"),
        "a revoked credential must be unauthorized, got {response}"
    );
    assert!(
        message.is_none(),
        "stale traffic must not mutate the replacement's state"
    );
}

/// Even holding the replacement's live credential, traffic that declares the
/// superseded generation is refused rather than applied.
#[test]
fn superseded_generation_is_refused_on_the_live_credential() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation(
            "agent-one",
            2,
            "reg-one-replacement",
            TOKEN_ONE_REPLACEMENT,
        ))
        .unwrap_or_else(|error| panic!("reserve replacement credential: {error}"));

    let (response, message) = serve(
        &registry,
        "/jsp/1/register",
        TOKEN_ONE_REPLACEMENT,
        "reg-one-replacement",
        // Declares generation 1 while the reservation is generation 2.
        &snapshot("agent-one", 1, "epoch-one", 0, "stale generation", 1001),
    );
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "a superseded generation must be forbidden, got {response}"
    );
    assert!(
        message.is_none(),
        "a rejected publication must not mutate canonical state"
    );
}
