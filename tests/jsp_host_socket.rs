use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;

use jefe::domain::{AgentId, observation::Availability};
use jefe::jsp_host::{
    CredentialRole, JspHost, PublisherRegistry, PublisherReservation, create_bootstrap,
};
use jefe::messages::RuntimeMessage;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// Collects delivered runtime messages until `expected` have arrived or the
/// deadline passes.
///
/// The worker publishes to the delivery queue *after* it writes the HTTP
/// response, so observing `200 OK` does not mean the message is queued yet.
/// Draining immediately is a race; this polls instead of sleeping a fixed
/// amount so the test is neither flaky nor artificially slow.
///
/// A poisoned lock surfaces immediately as a panic with the real error rather
/// than spinning until the deadline as an opaque timeout.
fn drain_at_least(
    runtime: &jefe::jsp_host::JspHostRuntime,
    expected: usize,
) -> Vec<RuntimeMessage> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut collected = Vec::new();
    while collected.len() < expected && std::time::Instant::now() < deadline {
        let drained = runtime
            .drain_messages()
            .unwrap_or_else(|error| panic!("JSP delivery drain failed: {error}"));
        collected.extend(drained);
        if collected.len() < expected {
            thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    collected
}

fn reservation() -> PublisherReservation {
    PublisherReservation {
        agent_id: AgentId("agent-alex".to_string()),
        generation: 7,
        registration_id: "reg-fixture".to_string(),
        publisher_credential: TOKEN.to_string(),
        role: CredentialRole::Publisher,
    }
}

fn request(addr: SocketAddr, route: &str, token: &str, body: &[u8]) -> String {
    request_with_registration(addr, route, token, "reg-fixture", body)
}

fn request_with_registration(
    addr: SocketAddr,
    route: &str,
    token: &str,
    registration_id: &str,
    body: &[u8],
) -> String {
    // These land directly in header lines, so a value containing CR or LF
    // would smuggle an extra header and quietly change what is being tested.
    for value in [route, token, registration_id] {
        assert!(
            !value.contains(['\r', '\n']),
            "header values must not contain CR or LF"
        );
    }
    let mut stream =
        TcpStream::connect(addr).unwrap_or_else(|error| panic!("connect loopback host: {error}"));
    write!(
        stream,
        "POST {route} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nJsp-Registration-Id: {registration_id}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap_or_else(|error| panic!("write request: {error}"));
    stream
        .write_all(body)
        .unwrap_or_else(|error| panic!("write body: {error}"));
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .unwrap_or_else(|error| panic!("read response: {error}"));
    response
}

#[test]
fn real_socket_registers_authenticated_bound_snapshot_and_emits_payload() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let host = JspHost::bind(registry).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    assert!(addr.ip().is_loopback());

    let handle = thread::spawn(move || host.serve_once());
    let response = request(
        addr,
        "/jsp/1/register",
        TOKEN,
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(response.starts_with("HTTP/1.1 200"));
    let message = handle
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
    let Some(RuntimeMessage::ObservationUpdated(agent_id, generation, observation)) = message
    else {
        panic!("accepted snapshot must emit an observation message");
    };
    assert_eq!(agent_id.0, "agent-alex");
    assert_eq!(generation, 7);
    let Availability::Known(todos) = (match observation.todos {
        jefe::domain::observation::FieldState::Supported { availability, .. } => availability,
        jefe::domain::observation::FieldState::Unsupported => panic!("todos unsupported"),
    }) else {
        panic!("todos must be known");
    };
    assert_eq!(todos.items[0].text.as_str(), "Write parser");
}

#[test]
fn unknown_credential_is_401_and_wrong_binding_is_403() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let host = JspHost::bind(registry.clone()).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let unknown = thread::spawn(move || host.serve_once());
    let response = request(
        addr,
        "/jsp/1/register",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(response.starts_with("HTTP/1.1 401"));
    unknown
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));

    let host = JspHost::bind(registry).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let wrong = thread::spawn(move || host.serve_once());
    let body = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_identity_distinct.json");
    let response = request(addr, "/jsp/1/register", TOKEN, body);
    assert!(response.starts_with("HTTP/1.1 403"));
    wrong
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
}

/// A credential that authenticates (it is known) but carries the wrong role is
/// rejected with 403, not 401. The rejection must never mutate canonical
/// state — no observation message is emitted and the publisher never
/// transitions out of the Reserved phase. (issue #522, J3)
#[test]
fn wrong_role_credential_is_403_without_mutation() {
    let registry = PublisherRegistry::default();
    // Reserve the legitimate publisher credential.
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve publisher credential: {error}"));
    // Reserve an observer credential for the same agent/generation but a
    // different role. It uses a distinct credential so it is independently
    // authenticatable.
    let observer_token = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let observer_reservation = PublisherReservation {
        agent_id: AgentId("agent-alex".to_string()),
        generation: 7,
        registration_id: "reg-fixture".to_string(),
        publisher_credential: observer_token.to_string(),
        role: CredentialRole::Observer,
    };
    registry
        .reserve(observer_reservation)
        .unwrap_or_else(|error| panic!("reserve observer credential: {error}"));

    // The observer credential authenticates (it is known) but is rejected
    // with 403 because publisher-only routes require the Publisher role.
    let host = JspHost::bind(registry.clone()).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let worker = thread::spawn(move || host.serve_once());
    let response = request_with_registration(
        addr,
        "/jsp/1/register",
        observer_token,
        "reg-fixture",
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "wrong-role credential must be 403, got: {response}"
    );
    let message = worker
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
    // No observation message may be delivered: rejection never mutates
    // canonical state.
    assert!(
        message.is_none(),
        "wrong-role rejection must not emit an observation message"
    );

    // The legitimate publisher credential for the same agent still succeeds,
    // proving the observer rejection did not mutate any canonical state.
    assert!(
        serve_request(
            &registry,
            "/jsp/1/register",
            include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
        )
        .0
        .starts_with("HTTP/1.1 200"),
        "publisher credential must still register after a wrong-role rejection"
    );
}

#[test]
fn wrong_registration_id_is_403_without_mutation() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let host = JspHost::bind(registry).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let worker = thread::spawn(move || host.serve_once());
    let response = request_with_registration(
        addr,
        "/jsp/1/register",
        TOKEN,
        "wrong-registration",
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(response.starts_with("HTTP/1.1 403"));
    let message = worker
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
    assert!(message.is_none());
}

fn serve_request(
    registry: &PublisherRegistry,
    route: &str,
    body: &[u8],
) -> (String, Option<RuntimeMessage>) {
    let host = JspHost::bind(registry.clone()).unwrap_or_else(|error| panic!("bind host: {error}"));
    let addr = host
        .local_addr()
        .unwrap_or_else(|error| panic!("host address: {error}"));
    let worker = thread::spawn(move || host.serve_once());
    let response = request(addr, route, TOKEN, body);
    let message = worker
        .join()
        .unwrap_or_else(|_| panic!("host thread panicked"))
        .unwrap_or_else(|error| panic!("serve request: {error}"));
    (response, message)
}

fn event_with_sequence(sequence: u64) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "kind": "event",
        "agent_id": "agent-alex",
        "lifecycle_generation": 7,
        "source_epoch": "epoch-001",
        "source_sequence": sequence,
        "bridge_observed_ms": 1,
        "event": {"type": "activity.changed", "state": "idle"}
    }))
    .unwrap_or_else(|error| panic!("serialize event: {error}"))
}

#[test]
fn routes_require_one_registration_and_idempotent_re_registration() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let (publish_response, publish_message) =
        serve_request(&registry, "/jsp/1/publish", &event_with_sequence(42));
    assert!(publish_response.starts_with("HTTP/1.1 409"));
    assert!(publish_message.is_none());
    let (heartbeat_response, heartbeat_message) = serve_request(
        &registry,
        "/jsp/1/heartbeat",
        include_bytes!("../dev-docs/jsp/v1/fixtures/heartbeat_full.json"),
    );
    assert!(heartbeat_response.starts_with("HTTP/1.1 409"));
    assert!(heartbeat_message.is_none());
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");
    assert!(
        serve_request(&registry, "/jsp/1/register", snapshot)
            .0
            .starts_with("HTTP/1.1 200")
    );
    // An identical re-registration (same identity triple, same epoch) is an
    // idempotent replay: it returns 200 but delivers no message so the
    // canonical state is not double-applied.
    let (replay_response, replay_message) = serve_request(&registry, "/jsp/1/register", snapshot);
    assert!(replay_response.starts_with("HTTP/1.1 200"));
    assert!(replay_message.is_none());
}

fn snapshot_with_epoch(epoch: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "kind": "snapshot",
        "agent_id": "agent-alex",
        "lifecycle_generation": 7,
        "source_epoch": epoch,
        "source_sequence": 42,
        "cursor": 41,
        "bridge_observed_ms": 1000,
        "native_session": {
            "repository": "vybestack/llxprt-jefe",
            "path": "/Users/dev/src/jefe",
            "agent_kind": "llxprt",
            "pid": 12345,
            "display_name": "main-worker"
        },
        "process_binding": known_field(serde_json::json!({"pid": 12345, "started_at_ms": 1000})),
        "native_activity": known_field(serde_json::json!({"state": "idle"})),
        "current_wait": known_field(serde_json::Value::Null),
        "current_turn": known_field(serde_json::json!({"elapsed_ms": 12000})),
        "todos": known_field(serde_json::json!({
            "revision": 3,
            "items": [{"text": "Write parser", "completed": false}]
        })),
        "last_displayed_assistant_message": known_field(serde_json::json!({
            "content": "Done.",
            "committed_ms": 1000
        })),
        "last_created_tool_call": known_field(serde_json::json!({"label": "Read", "phase": "succeeded"})),
        "source_terminal_state": known_field(serde_json::Value::Null),
        "source_error_state": "unsupported"
    }))
    .unwrap_or_else(|error| panic!("serialize snapshot: {error}"))
}

fn known_field(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "provenance": "authoritative",
        "availability": "known",
        "value": value
    })
}

/// A different source_epoch for the same agent/generation is a genuine conflict
/// and must return 409 even though the publisher is already registered.
#[test]
fn re_registration_with_different_epoch_is_409() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");
    assert!(
        serve_request(&registry, "/jsp/1/register", snapshot)
            .0
            .starts_with("HTTP/1.1 200")
    );
    let different_epoch = snapshot_with_epoch("epoch-999");
    let (conflict_response, conflict_message) =
        serve_request(&registry, "/jsp/1/register", &different_epoch);
    assert!(conflict_response.starts_with("HTTP/1.1 409"));
    assert!(conflict_message.is_none());
}

/// After an idempotent re-registration, publish/heartbeat rules still hold: the
/// stream identity is the original epoch, so events bound to that epoch are
/// accepted and events with a different epoch are rejected.
#[test]
fn idempotent_re_registration_preserves_sequence_rules() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");
    assert!(
        serve_request(&registry, "/jsp/1/register", snapshot)
            .0
            .starts_with("HTTP/1.1 200")
    );
    // Idempotent replay: same snapshot, same epoch -> 200, no message.
    let (replay_response, replay_message) = serve_request(&registry, "/jsp/1/register", snapshot);
    assert!(replay_response.starts_with("HTTP/1.1 200"));
    assert!(replay_message.is_none());

    // After the replay the stream is still live at cursor 41, so event 42
    // applies and event 43 (gap) is rejected.
    let (ok_response, ok_message) =
        serve_request(&registry, "/jsp/1/publish", &event_with_sequence(42));
    assert!(ok_response.starts_with("HTTP/1.1 200"));
    assert!(ok_message.is_some());
    let (gap_response, _gap_message) =
        serve_request(&registry, "/jsp/1/publish", &event_with_sequence(44));
    assert!(gap_response.starts_with("HTTP/1.1 400"));
}

/// Full real-socket idempotent replay: the producer registers, the 200 is
/// acknowledged, and an identical re-registration returns 200 again without
/// delivering a duplicate observation message. The publisher registry uses
/// internal `Arc<Mutex<...>>` sharing, so `serve_request` clones — which share
/// the same underlying state — preserve registration across separate host
/// binds on the real loopback socket.
#[test]
fn real_socket_idempotent_re_registration_does_not_double_apply() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");

    // First registration through the real loopback socket: 200 + message.
    let (first_response, first_message) = serve_request(&registry, "/jsp/1/register", snapshot);
    assert!(first_response.starts_with("HTTP/1.1 200"));
    assert!(
        first_message.is_some(),
        "first registration must emit an observation message"
    );

    // Identical re-registration through a second real socket: 200, no message.
    let (replay_response, replay_message) = serve_request(&registry, "/jsp/1/register", snapshot);
    assert!(replay_response.starts_with("HTTP/1.1 200"));
    assert!(
        replay_message.is_none(),
        "idempotent replay must not double-apply or deliver a duplicate message"
    );
}

#[test]
fn rejected_gap_and_parse_error_forward_health_mutations() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");
    assert!(
        serve_request(&registry, "/jsp/1/register", snapshot)
            .0
            .starts_with("HTTP/1.1 200")
    );
    let (gap_response, gap_message) =
        serve_request(&registry, "/jsp/1/publish", &event_with_sequence(43));
    assert!(gap_response.starts_with("HTTP/1.1 400"));
    let Some(RuntimeMessage::ObservationUpdated(_, 7, gap_observation)) = gap_message else {
        panic!("gap must forward stale observation");
    };
    assert_eq!(
        gap_observation.health,
        jefe::domain::observation::ObservationHealth::Stale
    );
    let (parse_response, parse_message) =
        serve_request(&registry, "/jsp/1/publish", br#"{"not":"jsp"}"#);
    assert!(parse_response.starts_with("HTTP/1.1 400"));
    let Some(RuntimeMessage::ObservationUpdated(_, 7, parse_observation)) = parse_message else {
        panic!("parse failure must forward protocol-error observation");
    };
    assert_eq!(
        parse_observation.health,
        jefe::domain::observation::ObservationHealth::ProtocolError
    );
}

#[test]
fn producer_lease_tick_marks_live_observation_stale_once() {
    let registry = PublisherRegistry::default();
    registry
        .reserve(reservation())
        .unwrap_or_else(|error| panic!("reserve credential: {error}"));
    let snapshot = include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json");
    assert!(
        serve_request(&registry, "/jsp/1/register", snapshot)
            .0
            .starts_with("HTTP/1.1 200")
    );
    let messages = registry
        .tick(std::time::Instant::now() + std::time::Duration::from_secs(16))
        .unwrap_or_else(|error| panic!("lease tick: {error}"));
    assert!(matches!(
        messages.as_slice(),
        [RuntimeMessage::ObservationUpdated(_, 7, observation)]
            if observation.health == jefe::domain::observation::ObservationHealth::Stale
    ));
    assert!(
        registry
            .tick(std::time::Instant::now() + std::time::Duration::from_secs(17))
            .unwrap_or_else(|error| panic!("second lease tick: {error}"))
            .is_empty()
    );
}
#[test]
fn bootstrap_is_owner_only_contains_no_argv_secret_and_cleans_up() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let material = create_bootstrap(
        temp.path(),
        "127.0.0.1:49152"
            .parse()
            .unwrap_or_else(|error| panic!("address: {error}")),
        &reservation(),
    )
    .unwrap_or_else(|error| panic!("bootstrap: {error}"));
    let content = std::fs::read_to_string(material.path())
        .unwrap_or_else(|error| panic!("read bootstrap: {error}"));
    let document: serde_json::Value =
        serde_json::from_str(&content).unwrap_or_else(|error| panic!("parse bootstrap: {error}"));
    assert_eq!(
        document,
        serde_json::json!({
            "schema": 1,
            "protocol": "jsp/1",
            "endpoint": "http://127.0.0.1:49152/jsp/1",
            "registration_id": "reg-fixture",
            "publisher_credential": TOKEN,

            "agent_id": "agent-alex",
            "lifecycle_generation": 7,
        })
    );
    assert!(!format!("{:?}", material.path()).contains(TOKEN));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(material.path())
            .unwrap_or_else(|error| panic!("metadata: {error}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    material
        .cleanup()
        .unwrap_or_else(|error| panic!("cleanup: {error}"));
    assert!(!material.path().exists());
}

#[cfg(unix)]
#[test]
fn stale_cleanup_skips_symlink_candidates() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime_dir = temp.path().join("jsp");
    std::fs::create_dir(&runtime_dir).unwrap_or_else(|error| panic!("runtime dir: {error}"));
    let outside = temp.path().join("outside-secret");
    std::fs::write(&outside, "preserve").unwrap_or_else(|error| panic!("outside file: {error}"));
    symlink(&outside, runtime_dir.join("jsp-agent-alex-1.json"))
        .unwrap_or_else(|error| panic!("bootstrap symlink: {error}"));
    let runtime = jefe::jsp_host::JspHostRuntime::start(runtime_dir)
        .unwrap_or_else(|error| panic!("start runtime: {error}"));
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap_or_else(|error| panic!("outside read: {error}")),
        "preserve"
    );
    drop(runtime);
}
#[test]
fn launch_environment_is_local_llxprt_only_and_never_contains_token() {
    use jefe::domain::agent_definition::{AgentLaunchPlan, AgentTypeId, RemoteTarget, Target};
    use jefe::jsp_host::{BOOTSTRAP_ENV, authorize_launch_environment};

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let material = create_bootstrap(
        temp.path(),
        "127.0.0.1:49152"
            .parse()
            .unwrap_or_else(|error| panic!("address: {error}")),
        &reservation(),
    )
    .unwrap_or_else(|error| panic!("bootstrap: {error}"));
    let mut local = AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("type id: {error}")),
        target: Target::Local {
            canonical_cwd: temp.path().to_path_buf(),
        },
        ..AgentLaunchPlan::default()
    };
    assert!(authorize_launch_environment(&mut local, &material));
    assert!(
        local
            .env
            .iter()
            .any(|(name, value)| { name == BOOTSTRAP_ENV && value == material.path().as_os_str() })
    );
    assert!(!local.argv.iter().any(|argument| argument == TOKEN));
    assert!(!local.env.iter().any(|(_, value)| value == TOKEN));

    let mut remote = local;
    remote.env.clear();
    remote.target = Target::Remote(RemoteTarget::default());
    assert!(!authorize_launch_environment(&mut remote, &material));
    assert!(remote.env.is_empty());
}

fn read_bootstrap(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|error| panic!("read generated bootstrap: {error}")),
    )
    .unwrap_or_else(|error| panic!("parse generated bootstrap: {error}"))
}

fn bootstrap_string(bootstrap: &serde_json::Value, field: &str) -> String {
    bootstrap[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} must be a string"))
        .to_owned()
}

fn publish_repeated_snapshots(
    runtime: &jefe::jsp_host::JspHostRuntime,
    credential: &str,
    registration_id: &str,
) {
    for _ in 0..100 {
        let response = request_with_registration(
            runtime.endpoint(),
            "/jsp/1/publish",
            credential,
            registration_id,
            include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
        );
        assert!(response.starts_with("HTTP/1.1 200"));
        // Yield briefly so the single-threaded host worker can accept the next
        // connection. Under parallel test contention the accept backlog can
        // overflow without this, producing a spurious broken-pipe failure.
        thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn production_host_generates_unique_credentials_delivers_and_revokes() {
    use jefe::domain::agent_definition::{AgentLaunchPlan, AgentTypeId, Target};
    use jefe::jsp_host::JspHostRuntime;

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let runtime = JspHostRuntime::start(temp.path().join("jsp"))
        .unwrap_or_else(|error| panic!("start JSP runtime: {error}"));
    let agent_id = AgentId("agent-alex".to_owned());
    let plan = AgentLaunchPlan {
        type_id: AgentTypeId::parse("core.llxprt")
            .unwrap_or_else(|error| panic!("type id: {error}")),
        target: Target::Local {
            canonical_cwd: temp.path().to_path_buf(),
        },
        ..AgentLaunchPlan::default()
    };
    let coordinator = runtime.coordinator();
    let prepared = coordinator
        .prepare_launch(&agent_id, 7, &plan)
        .unwrap_or_else(|error| panic!("prepare launch: {error}"))
        .unwrap_or_else(|| panic!("local LLxprt launch must be instrumented"));
    let bootstrap_path = prepared.bootstrap_path().to_path_buf();
    let bootstrap = read_bootstrap(&bootstrap_path);
    let credential = bootstrap_string(&bootstrap, "publisher_credential");
    let registration_id = bootstrap_string(&bootstrap, "registration_id");
    assert!(credential.starts_with("pub-"));
    assert_eq!(bootstrap["lifecycle_generation"], 7);
    prepared.commit();

    let response = request_with_registration(
        runtime.endpoint(),
        "/jsp/1/register",
        &credential,
        &registration_id,
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(matches!(
        drain_at_least(&runtime, 1).as_slice(),
        [RuntimeMessage::ObservationUpdated(delivered_agent, 7, _)] if delivered_agent == &agent_id
    ));
    publish_repeated_snapshots(&runtime, &credential, &registration_id);
    // Repeated snapshots coalesce into exactly one delivered update.
    assert_eq!(drain_at_least(&runtime, 1).len(), 1);

    coordinator
        .revoke(&agent_id)
        .unwrap_or_else(|error| panic!("revoke launch: {error}"));
    assert!(!bootstrap_path.exists());
    let response = request_with_registration(
        runtime.endpoint(),
        "/jsp/1/register",
        &credential,
        &registration_id,
        include_bytes!("../dev-docs/jsp/v1/fixtures/snapshot_full.json"),
    );
    assert!(response.starts_with("HTTP/1.1 401"));

    let replacement = coordinator
        .prepare_launch(&agent_id, 8, &plan)
        .unwrap_or_else(|error| panic!("prepare replacement: {error}"))
        .unwrap_or_else(|| panic!("replacement must be instrumented"));
    let replacement_json = read_bootstrap(replacement.bootstrap_path());
    assert_ne!(replacement_json["publisher_credential"], credential);
    assert_eq!(replacement_json["lifecycle_generation"], 8);
}
