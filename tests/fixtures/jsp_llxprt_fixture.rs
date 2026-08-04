use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Credential-free diagnostic for a failed fixture HTTP POST.
///
/// Each variant identifies a distinct failure class so the harness can
/// distinguish, e.g., a refused connection from a protocol rejection. No
/// variant or its Display output ever includes the bearer token, bootstrap
/// contents, or any other secret.
#[derive(Debug)]
enum PostError {
    /// The endpoint did not parse as http://host[:port]/...
    MalformedEndpoint,
    /// A header value contained CR or LF and was rejected.
    HeaderInjection,
    /// TCP connect, timeout, write, or read failure.
    Transport(String),
    /// The server responded with a non-200 status.
    UnexpectedStatus(String),
}

impl std::fmt::Display for PostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedEndpoint => f.write_str("endpoint is not http://host[:port]"),
            Self::HeaderInjection => f.write_str("header value contained CR or LF"),
            Self::Transport(detail) => write!(f, "transport error: {detail}"),
            // Only the status line prefix is retained; it never carries a secret.
            Self::UnexpectedStatus(detail) => write!(f, "unexpected response: {detail}"),
        }
    }
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if handle_probe(&args) {
        return;
    }
    let bootstrap = load_bootstrap();
    register_snapshot(&bootstrap);
    // Keep the producer lease alive. Without this the observation goes stale
    // about fifteen seconds after registration, which is fine for a proof that
    // opens the workbench immediately but not for one that drives more UI
    // first.
    heartbeat_forever(&bootstrap);
}

/// Publish a heartbeat every five seconds so the producer lease never lapses.
fn heartbeat_forever(bootstrap: &serde_json::Value) -> ! {
    let endpoint = bootstrap["endpoint"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let credential = bootstrap["publisher_credential"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let registration_id = bootstrap["registration_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let agent_id = bootstrap["agent_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let generation = bootstrap["lifecycle_generation"].as_u64().unwrap_or(1);
    let mut observed_ms: u64 = 1000;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        observed_ms = observed_ms.saturating_add(5000);
        let heartbeat = heartbeat_document(&agent_id, generation, observed_ms);
        // A failed heartbeat is not fatal: the harness asserts on rendered
        // state, and a transient failure simply shows as a stale observation.
        let _ = post(
            &endpoint,
            "heartbeat",
            &credential,
            &registration_id,
            &heartbeat,
        );
    }
}

fn heartbeat_document(agent_id: &str, generation: u64, observed_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "schema": 1,
        "kind": "heartbeat",
        "agent_id": agent_id,
        "lifecycle_generation": generation,
        "source_epoch": "fixture-epoch",
        "bridge_observed_ms": observed_ms
    })
}

fn handle_probe(args: &[String]) -> bool {
    if args.iter().any(|argument| argument == "--version") {
        write_stdout("1.0.0\n");
        return true;
    }
    if args.iter().any(|argument| argument == "--help") {
        write_stdout(
            "--prompt-interactive --profile-load --sandbox --sandbox-engine --yolo --approval-mode --continue\n",
        );
        return true;
    }
    false
}

fn load_bootstrap() -> serde_json::Value {
    let Some(bootstrap_path) = std::env::var_os("LLXPRT_JSP_BOOTSTRAP_FILE") else {
        std::process::exit(2);
    };
    let Some(bootstrap) = std::fs::read(bootstrap_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        std::process::exit(2);
    };
    bootstrap
}

/// Decide whether this instance publishes a blocked agent.
///
/// A scenario needing two differently-stated agents points
/// `JSP_FIXTURE_WAIT_TICKET` at a path. The first instance to start claims it
/// and publishes a working agent; the next finds it and publishes an agent
/// blocked on a permission request. Without the variable every instance is a
/// working agent, which is what the single-agent proofs expect.
fn claim_wait_ticket() -> bool {
    let Some(path) = std::env::var_os("JSP_FIXTURE_WAIT_TICKET") else {
        return false;
    };
    if std::path::Path::new(&path).exists() {
        return true;
    }
    let _ = std::fs::write(&path, b"claimed");
    false
}

fn activity_value(waiting: bool) -> serde_json::Value {
    serde_json::json!({ "state": if waiting { "idle" } else { "acting" } })
}

/// The wait payload is a closed object carrying only `reason`.
fn wait_value(waiting: bool) -> serde_json::Value {
    if waiting {
        serde_json::json!({ "reason": "permission" })
    } else {
        serde_json::Value::Null
    }
}

fn register_snapshot(bootstrap: &serde_json::Value) {
    let Some(endpoint) = bootstrap["endpoint"].as_str() else {
        std::process::exit(2);
    };
    let Some(credential) = bootstrap["publisher_credential"].as_str() else {
        std::process::exit(2);
    };
    let Some(agent_id) = bootstrap["agent_id"].as_str() else {
        std::process::exit(2);
    };
    let Some(generation) = bootstrap["lifecycle_generation"].as_u64() else {
        std::process::exit(2);
    };
    let waiting = claim_wait_ticket();
    let snapshot = serde_json::json!({
        "schema": 1,
        "kind": "snapshot",
        "agent_id": agent_id,
        "lifecycle_generation": generation,
        "source_epoch": "fixture-epoch",
        "source_sequence": 0,
        "cursor": 0,
        "bridge_observed_ms": 1000,
        "native_session": {
            "repository": "fixture/repository",
            "path": "/fixture/repository",
            "agent_kind": "llxprt",
            "pid": std::process::id(),
            "display_name": "jsp-fixture"
        },
        "process_binding": known(serde_json::json!({
            "pid": std::process::id(),
            "started_at_ms": 1000
        })),
        "native_activity": known(activity_value(waiting)),
        "current_wait": known(wait_value(waiting)),
        "current_turn": known(serde_json::json!({"elapsed_ms": 1000})),
        "todos": known(serde_json::json!({
            "revision": 1,
            "items": [{"text": "Implement issue 522", "completed": false}]
        })),
        "last_displayed_assistant_message": known(serde_json::json!({
            "content": "JSP preview is wired",
            "committed_ms": 1000
        })),
        "last_created_tool_call": known(serde_json::json!({
            "label": "run_shell",
            "phase": "executing"
        })),
        "source_terminal_state": known(serde_json::Value::Null),
        "source_error_state": {"provenance": "authoritative", "availability": "unknown"}
    });
    let Some(registration_id) = bootstrap["registration_id"].as_str() else {
        std::process::exit(2);
    };
    if let Err(error) = post(endpoint, "register", credential, registration_id, &snapshot) {
        // Print the safe, credential-free diagnostic before exiting so a
        // fixture failure is diagnosable in the harness output.
        write_stderr(&format!("JSP fixture registration failed: {error}\n"));
        std::process::exit(3);
    }
}

fn known(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "provenance": "authoritative",
        "availability": "known",
        "value": value
    })
}

fn post(
    endpoint: &str,
    route: &str,
    credential: &str,
    registration_id: &str,
    body: &serde_json::Value,
) -> Result<(), PostError> {
    let Some(authority) = endpoint
        .strip_prefix("http://")
        .and_then(|value| value.split('/').next())
    else {
        return Err(PostError::MalformedEndpoint);
    };
    // These values are interpolated straight into header lines, so refuse
    // anything that could terminate a header early and smuggle another.
    if [credential, registration_id, route]
        .iter()
        .any(|value| value.contains(['\r', '\n']))
    {
        return Err(PostError::HeaderInjection);
    }
    let body = body.to_string();
    let mut stream =
        TcpStream::connect(authority).map_err(|e| PostError::Transport(e.to_string()))?;
    // Without timeouts an unresponsive server hangs the fixture until the
    // harness kills it, which reports as an unrelated scenario timeout.
    let timeout = std::time::Duration::from_secs(5);
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| PostError::Transport(e.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| PostError::Transport(e.to_string()))?;
    write!(
        stream,
        "POST /jsp/1/{route} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {credential}\r\nJsp-Registration-Id: {registration_id}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| PostError::Transport(e.to_string()))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| PostError::Transport(e.to_string()))?;
    // Extract only the status line for the diagnostic; it never carries a
    // secret.
    let status_line = response.lines().next().unwrap_or("no response");
    if response.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        Err(PostError::UnexpectedStatus(status_line.to_string()))
    }
}

fn write_stdout(value: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(value.as_bytes());
}

fn write_stderr(value: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(value.as_bytes());
}
