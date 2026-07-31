use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if handle_probe(&args) {
        return;
    }
    let bootstrap = load_bootstrap();
    register_snapshot(&bootstrap);
    loop {
        std::thread::park_timeout(Duration::from_secs(60));
    }
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
        "native_activity": known(serde_json::json!({"state": "acting"})),
        "current_wait": known(serde_json::Value::Null),
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
    if !post(endpoint, "register", credential, registration_id, &snapshot) {
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
) -> bool {
    let Some(authority) = endpoint
        .strip_prefix("http://")
        .and_then(|value| value.split('/').next())
    else {
        return false;
    };
    let body = body.to_string();
    let Ok(mut stream) = TcpStream::connect(authority) else {
        return false;
    };
    if write!(
        stream,
        "POST /jsp/1/{route} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {credential}\r\nJsp-Registration-Id: {registration_id}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .is_err()
    {
        return false;
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).is_ok() && response.starts_with("HTTP/1.1 200")
}

fn write_stdout(value: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(value.as_bytes());
}
