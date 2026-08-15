//! Cross-platform action-provider fixture binary (issue #390 CW-10, Slice C1).
//!
//! A minimal provider speaking the closed JSONL protocol on stdin/stdout. It is
//! driven by `argv[1]` (scenario mode) and, for the `record` mode, `argv[2]`
//! (an observation directory). The supervisor's focused integration tests drive
//! it through the real supervisor to prove the exact lifecycle transcript, the
//! queue overflow, the staged shutdown/reap, the secret redaction across every
//! provider-owned surface, and the isolated environment.
//!
//! Modes:
//! - `happy` (default): hello-ack, ready, 3 progress, Navigate outcome.
//! - `progress-256`: ready, 256 progress events, then a Refresh outcome.
//! - `error`: ready, an `error` terminal.
//! - `never-ready`: hello-ack then hang (handshake timeout / stage-B reap).
//! - `crash-after-ready`: ready then exit 1 (crash before a terminal).
//! - `bad-order`: after invoke-action, emit an out-of-order handshake message.
//! - `generation-drift`: hello-ack with the wrong generation (protocol fault).
//! - `hang-shutdown`: full lifecycle then ignore shutdown (stage-B/C reap).
//! - `secret-stderr`: echo a received Configure secret to stderr (redaction).
//! - `secret-navigate`/`secret-refresh`/`secret-panel`/`secret-migrated`: echo
//!   the Configure secret into the named outcome surface (redaction).
//! - `secret-confirm`: echo the secret into a request-host-confirmation outcome.
//! - `secret-error`: echo the secret into an error terminal (message + path).
//! - `record`: full lifecycle plus observation files for CW10-14.
//! - `duplicate-terminal`: two `outcome` messages (post-terminal PLG-E502).
//! - `descendant-hang`: spawn an in-group hanging child that keeps the
//!   inherited pipes open, then hang on shutdown (bounded group reap).
//! - `descendant-hang-child`: a hanging descendant spawned by `descendant-hang`.
//! - `ack-wrong-kind`/`ack-missing`/`ack-eof-before`/`ack-data-after`: strict
//!   shutdown-ack lifecycle-failure evidence.
//! - `persistent-ready`: a persistent candidate that handshakes to `ready`,
//!   records its plugin id into a shared startup-sequence file (argv[2]), then
//!   enters the post-ready loop reading `invoke-action`/`cancel`/`shutdown`.
//!   Used by the CW10-03 ordered two-provider startup and the CW10-04
//!   rollback/no-restart/shutdown tests.
//! - `persistent-invoke`: like `persistent-ready` but each `invoke-action`
//!   emits three progress events and a Navigate outcome (repeated same-PID
//!   invocation, progress-before-terminal evidence).
//! - `persistent-invoke-hang`: like `persistent-invoke` but each
//!   `invoke-action` emits one progress and never a terminal (cancel/timeout
//!   evidence).
//! - `persistent-timeout-then-terminal`: emit a terminal just after the host's
//!   invocation timeout (late-output generation-retirement evidence).
//! - `persistent-invoke-then-crash`: after the first `invoke-action` emits one
//!   progress then exits 1 (post-Ready crash during invocation evidence).
//! - `persistent-hello-hang`: read `hello` then hang (hello-ack timeout).
//! - `persistent-ready-hang`: `hello-ack` then hang before `ready` (ready
//!   timeout).
//! - `persistent-crash-after-ack`: `hello-ack` then exit 1 (configure-write /
//!   ready-eof failure).
//! - `persistent-protocol`: `hello-ack` with a drifted generation (protocol
//!   fault).
//! - `persistent-undeclared-cap`: `ready` reporting a capability the host did
//!   not declare (capability-subset rejection).
//! - `persistent-ready-then-exit`: reach `ready` then exit 0 on its own (no
//!   auto-restart evidence).
//! - `persistent-secret-protocol`: echo a received Configure secret as an
//!   invalid `ready` capability so the parse fault carries the secret verbatim
//!   (persistent startup-failure redaction proof).
//! - `persistent-illegal-bytes`: reach `ready` then emit an unsolicited protocol
//!   frame while alive (health `ProtocolFault` fail-fast proof).
//! - `persistent-descendant-hang`: reach `ready`, spawn an escaping in-its-own-
//!   group descendant that holds the inherited pipes, then acknowledge and exit
//!   (DrainTimeout cleanup-failure proof; Unix only).
//! - `persistent-ack-wrong-kind`/`persistent-ack-missing`/`persistent-ack-eof-
//!   before`/`persistent-ack-data-after`: strict persistent shutdown-ack
//!   lifecycle-failure evidence while still being killed/reaped.
//! - `persistent-escape-child`: a lingering descendant spawned by
//!   `persistent-descendant-hang` (Unix only).
//!
//! Test-only sidecar control: the product composition spawns providers with no
//! argv, so a staged copy of this fixture selects its behavior from a
//! `<executable>.control` file next to the copied executable — one
//! `key=value` line per setting (`mode`, `record_dir`, `spawn_marker`). The
//! sidecar is consulted only when argv supplies no mode, so every test that
//! drives the fixture through explicit argv (all of issue #390) is unaffected.
//! `spawn_marker` names a file touched the instant the fixture starts: the
//! fail-if-spawned trap for providers the host must never start.

#[path = "provider_fixture_control.rs"]
mod control;

use std::io::Write;
use std::process::ExitCode;
use std::process::Stdio;

use control::resolve_invocation;
use serde_json::Value;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

fn run() -> Result<(), u8> {
    let (mode, record_dir) = resolve_invocation();

    // A hanging descendant that keeps the inherited pipes open. Spawned by the
    // descendant-hang mode; it never speaks the protocol.
    if mode == "descendant-hang-child" {
        hang_forever();
    }

    // An escaping descendant that holds the inherited pipes for a bounded time
    // in its own process group, so it survives the leader's group reap. Spawned
    // by the persistent-descendant-hang mode (DrainTimeout cleanup evidence).
    if mode == "persistent-escape-child" {
        std::thread::sleep(std::time::Duration::from_secs(5));
        return Ok(());
    }

    let hello = next_line()?;
    let generation = parse_generation(&hello);

    // Persistent candidate lifecycle (issue #390 CW-10, Slice C2). A persistent
    // candidate handshakes only to `ready` and then waits for `shutdown`; it
    // never reads an `invoke-action`.
    if mode.starts_with("persistent-") {
        return run_persistent(&mode, &hello, generation, record_dir.as_deref());
    }

    if mode == "generation-drift" {
        emit(&frame(
            "hello-ack",
            generation + 1,
            r#"{"provider_name":"x","protocol":1}"#,
        ));
        hang_forever();
    }
    emit(&frame(
        "hello-ack",
        generation,
        r#"{"provider_name":"fixture","protocol":1}"#,
    ));

    if mode.starts_with("migration-") {
        return run_migration(&mode, generation);
    }

    let configure_line = next_line()?;
    if mode == "record" {
        record_observations(record_dir.as_deref(), &configure_line);
    }
    if mode == "secret-stderr" {
        echo_secret_to_stderr(&configure_line);
    }
    if mode == "never-ready" {
        hang_forever();
    }
    emit(&frame(
        "ready",
        generation,
        r#"{"capabilities":["actions"]}"#,
    ));

    let _invoke_line = next_line()?;
    if mode == "crash-after-ready" {
        std::process::exit(1);
    }

    // Secret-bearing outcome modes echo the resolved Configure secret into the
    // provider-owned observation surface so the supervisor's redaction is proven.
    let secret = extract_first_secret_value(&configure_line);
    emit_terminal_scenario(&mode, generation, secret.as_deref())?;

    // A descendant that holds the inherited stdout/stderr open, so the
    // supervisor must kill the process group to close the pipes and reap.
    if mode == "descendant-hang" {
        spawn_hanging_descendant();
    }

    let _shutdown = next_line()?;

    emit_ack_scenario(&mode, generation)?;
    Ok(())
}

fn run_migration(mode: &str, generation: u64) -> Result<(), u8> {
    if mode == "migration-timeout" {
        hang_forever();
    }
    if mode == "migration-eof" {
        return Ok(());
    }
    let request = next_line()?;
    if mode == "migration-malformed" {
        emit("{not-json");
        return Ok(());
    }
    let parsed: Value = serde_json::from_str(&request).map_err(|_| 2)?;
    let request_id = parsed.get("request_id").and_then(Value::as_str).ok_or(2)?;
    let payload = parsed.get("payload").and_then(Value::as_object).ok_or(2)?;
    let mut response = serde_json::json!({
        "from_version": payload.get("from_version").ok_or(2)?,
        "to_version": payload.get("to_version").ok_or(2)?,
        "config": payload.get("config").ok_or(2)?,
        "draft_token": payload.get("draft_token").ok_or(2)?,
        "target_config": payload.get("config").ok_or(2)?,
        "notes": ["fixture migration"]
    });
    update_migration_response(mode, &mut response);
    let response_id = if mode == "migration-wrong-request" {
        "h-999999"
    } else {
        request_id
    };
    let response_generation = if mode == "migration-wrong-generation" {
        generation.saturating_add(1)
    } else {
        generation
    };
    emit(&frame_with_request(
        "migrated-config",
        response_id,
        response_generation,
        &response.to_string(),
    ));
    let shutdown = next_line()?;
    if frame_type(&shutdown).as_deref() != Some("shutdown") {
        return Err(2);
    }
    emit(&frame("shutdown-ack", generation, "{}"));
    Ok(())
}

fn update_migration_response(mode: &str, response: &mut Value) {
    match mode {
        "migration-wrong-source-version" => response["from_version"] = serde_json::json!(99),
        "migration-wrong-target-version" => response["to_version"] = serde_json::json!(99),
        "migration-wrong-source-config" => {
            response["config"] = serde_json::json!({"unexpected": true});
        }
        "migration-wrong-draft-token" => response["draft_token"] = serde_json::json!(99),
        _ => {}
    }
}

/// Read one trimmed JSONL line from stdin (EOF or read error fails the run).
fn next_line() -> Result<String, u8> {
    let stdin = std::io::stdin();
    let mut buf = String::new();
    let read = stdin.read_line(&mut buf).map_err(|_| 2)?;
    if read == 0 {
        return Err(2);
    }
    let line = buf.trim_end_matches(['\n', '\r']).to_owned();
    if line.is_empty() {
        return Err(2);
    }
    Ok(line)
}

/// Parse one host-sent line as the JSON object fields this fixture observes.
///
/// The fixture deliberately does not link the production protocol decoder: the
/// host decoder has focused tests of its own, while linking it here pulls the
/// whole application into every spawned fixture under coverage instrumentation.
fn parse_host_line(line: &str) -> Option<Value> {
    serde_json::from_str(line).ok()
}

/// Extract the fixed generation from the host `hello` frame.
fn parse_generation(line: &str) -> u64 {
    parse_host_line(line)
        .and_then(|parsed| parsed.get("generation")?.as_u64())
        .unwrap_or(1)
}

/// Emit the post-`invoke-action` scenario (progress, terminal, or fault).
fn emit_terminal_scenario(mode: &str, generation: u64, secret: Option<&str>) -> Result<(), u8> {
    if mode == "secret-error" {
        emit(&frame("error", generation, &secret_error(secret)));
        return Ok(());
    }
    if let Some(outcome) = secret_outcome_for(mode, secret) {
        emit(&frame("outcome", generation, &outcome));
        return Ok(());
    }
    match mode {
        "bad-order" => {
            emit(&frame(
                "hello-ack",
                generation,
                r#"{"provider_name":"x","protocol":1}"#,
            ));
            hang_forever();
        }
        "progress-256" => emit_progress_256(generation),
        // Emit a few progress frames then hang forever: the invocation is live
        // but never reaches a terminal. Used to prove live progress delivery
        // (S16) and that a host cancel is observed while the invocation is
        // live rather than after the invocation deadline (S17).
        "progress-then-hang" => {
            for seq in 1..=3u16 {
                emit(&progress_frame(generation, seq));
            }
            hang_forever();
        }
        "error" => emit(&frame(
            "error",
            generation,
            r#"{"code":"PLG-EX","message":"boom","retryable":false,"field_errors":[]}"#,
        )),
        "duplicate-terminal" => emit_duplicate_terminal(generation),
        "hang-shutdown" => {
            emit(&frame(
                "outcome",
                generation,
                r#"{"kind":"navigate","route_id":"r.home","activation":{}}"#,
            ));
            let _shutdown = next_line()?;
            hang_forever();
        }
        _ => emit_happy_terminal(generation),
    }
    Ok(())
}

/// Emit the 256-progress then refresh outcome terminal.
fn emit_progress_256(generation: u64) {
    for seq in 1..=256u16 {
        emit(&progress_frame(generation, seq));
    }
    emit(&frame(
        "outcome",
        generation,
        r#"{"kind":"refresh","resource_ref":{}}"#,
    ));
}

/// Emit two consecutive outcome messages (post-terminal PLG-E502 evidence).
fn emit_duplicate_terminal(generation: u64) {
    emit(&frame(
        "outcome",
        generation,
        r#"{"kind":"navigate","route_id":"r.home","activation":{}}"#,
    ));
    emit(&frame(
        "outcome",
        generation,
        r#"{"kind":"navigate","route_id":"r.again","activation":{}}"#,
    ));
}

/// Emit the default happy terminal: 3 progress then a Navigate outcome.
fn emit_happy_terminal(generation: u64) {
    for seq in 1..=3u16 {
        emit(&progress_frame(generation, seq));
    }
    emit(&frame(
        "outcome",
        generation,
        r#"{"kind":"navigate","route_id":"r.home","activation":{}}"#,
    ));
}

/// Resolve a secret-bearing outcome payload for the given mode, or `None` if
/// the mode does not carry a secret.
fn secret_outcome_for(mode: &str, secret: Option<&str>) -> Option<String> {
    match mode {
        "secret-navigate" => Some(secret_outcome_navigate(secret)),
        "secret-refresh" => Some(secret_outcome_refresh(secret)),
        "secret-panel" => Some(secret_outcome_replace_panel(secret)),
        "secret-migrated" => Some(secret_outcome_migrated_config(secret)),
        "secret-confirm" => Some(secret_outcome_confirmation(secret)),
        _ => None,
    }
}

/// Emit the shutdown-ack scenario: a valid ack by default, or a strict lifecycle
/// failure mode. `descendant-hang` hangs (no ack) so the supervisor must kill
/// the process group that the lingering descendant shares.
fn emit_ack_scenario(mode: &str, generation: u64) -> Result<(), u8> {
    match mode {
        "ack-missing" | "descendant-hang" => hang_forever(),
        "ack-wrong-kind" => {
            emit(&progress_frame(generation, 1));
            Ok(())
        }
        "ack-eof-before" => {
            // Close stdout (exit) without acknowledging.
            Ok(())
        }
        "ack-data-after" => {
            emit(&frame("shutdown-ack", generation, "{}"));
            emit(&frame(
                "outcome",
                generation,
                r#"{"kind":"notice","severity":"info","message":"stray"}"#,
            ));
            Ok(())
        }
        _ => {
            emit(&frame("shutdown-ack", generation, "{}"));
            Ok(())
        }
    }
}

fn emit(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(line.as_bytes());
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

fn frame_with_request(kind: &str, request_id: &str, generation: u64, payload: &str) -> String {
    format!(
        "{{\"protocol\":1,\"type\":\"{kind}\",\"request_id\":\"{request_id}\",\"generation\":{generation},\"payload\":{payload}}}"
    )
}

fn frame(kind: &str, generation: u64, payload: &str) -> String {
    format!(
        "{{\"protocol\":1,\"type\":\"{kind}\",\"request_id\":\"p-000001\",\"generation\":{generation},\"payload\":{payload}}}"
    )
}

fn progress_frame(generation: u64, sequence: u16) -> String {
    let seq = sequence;
    frame(
        "progress",
        generation,
        &format!(r#"{{"sequence":{seq},"message":"step","completed":{seq},"total":256}}"#),
    )
}

/// A typed-map fragment carrying the secret under a valid field id.
fn secret_typed_map(secret: Option<&str>) -> String {
    let value = secret.unwrap_or("");
    format!(
        r#"{{"vendor.pkg.echoed":{{"type":"string","value":{json}}}}}"#,
        json = json_string(value)
    )
}

/// A minimal continuation-schema field declaration carrying the secret as its
/// text default.
fn secret_field(secret: Option<&str>) -> String {
    let value = secret.unwrap_or("");
    format!(
        r#"{{"id":"vendor.pkg.token","label":"Token","type":"string","required":false,"default":{json},"restart":"none"}}"#,
        json = json_string(value)
    )
}

fn secret_outcome_navigate(secret: Option<&str>) -> String {
    format!(
        r#"{{"kind":"navigate","route_id":"vendor.pkg.route","activation":{map}}}"#,
        map = secret_typed_map(secret)
    )
}

fn secret_outcome_refresh(secret: Option<&str>) -> String {
    format!(
        r#"{{"kind":"refresh","resource_ref":{map}}}"#,
        map = secret_typed_map(secret)
    )
}

fn secret_outcome_replace_panel(secret: Option<&str>) -> String {
    format!(
        r#"{{"kind":"replace-panel","panel_instance_id":"vendor.pkg.panel","snapshot":{map}}}"#,
        map = secret_typed_map(secret)
    )
}

fn secret_outcome_migrated_config(secret: Option<&str>) -> String {
    format!(
        r#"{{"kind":"migrated-config","migration":{map}}}"#,
        map = secret_typed_map(secret)
    )
}

fn secret_outcome_confirmation(secret: Option<&str>) -> String {
    let text = json_string(secret.unwrap_or(""));
    let field = secret_field(secret);
    let t = text.as_str();
    format!(
        r#"{{"kind":"request-host-confirmation","confirmation_id":"vendor.pkg.confirm","title":{t},"body":{t},"confirm_label":{t},"destructive":false,"continuation_schema":[{field}]}}"#
    )
}

fn secret_error(secret: Option<&str>) -> String {
    let text = json_string(secret.unwrap_or(""));
    let msg = text.as_str();
    let path = text.as_str();
    format!(
        r#"{{"code":"PLG-EX","message":{msg},"retryable":false,"field_errors":[{{"path":{path},"message":{msg}}}]}}"#
    )
}

/// Escape a string for embedding inside a hand-written JSON literal.
fn json_string(value: &str) -> String {
    use std::fmt::Write;
    let mut out = String::from('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Prove redaction: echo any value found under configure.payload.secrets to
/// stderr verbatim. The supervisor must scrub it from retained stderr.
fn echo_secret_to_stderr(configure_line: &str) {
    if let Some(value) = extract_first_secret_value(configure_line) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(value.as_bytes());
        let _ = stderr.write_all(b"\n");
    }
}

fn extract_first_secret_value(configure_line: &str) -> Option<String> {
    let idx = configure_line.find("\"secrets\":")?;
    let fragment = &configure_line[idx..];
    let colon = fragment.find(':')?;
    let after = &fragment[colon + 1..];
    let quote = after.find('"')?;
    let value_start = quote + 1;
    let rest = &after[value_start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Spawn a descendant in the same process group that inherits the provider's
/// pipes and hangs, so the supervisor must kill the group to close them.
fn spawn_hanging_descendant() {
    let exe = std::env::current_exe().unwrap_or_else(|error| {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "fixture: cannot resolve current exe: {error}");
        std::process::exit(3);
    });
    let _ = std::process::Command::new(exe)
        .arg("descendant-hang-child")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();
}

/// Write CW10-14 observation files (argv/env/cwd/configure). Best-effort.
fn record_observations(dir: Option<&str>, configure_line: &str) {
    let Some(dir) = dir else { return };
    let path = std::path::Path::new(dir);
    let _ = std::fs::create_dir_all(path);
    let argv = std::env::args().collect::<Vec<_>>().join("\n");
    let _ = std::fs::write(path.join("argv.txt"), argv);
    let mut env_pairs: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    env_pairs.sort();
    let _ = std::fs::write(path.join("env.txt"), env_pairs.join("\n"));
    if let Ok(cwd) = std::env::current_dir() {
        let _ = std::fs::write(path.join("cwd.txt"), cwd.to_string_lossy().as_bytes());
    }
    // The Configure payload the provider received (may contain a secret value).
    let _ = std::fs::write(path.join("configure.json"), configure_line);
}

// ---- Persistent candidate lifecycle (issue #390 CW-10, Slice C2)

/// The persistent candidate handshake. It reaches `ready`, records its plugin
/// id into a shared startup-sequence file (when a record directory is known,
/// from argv or the control sidecar) so the ordered-startup test can observe
/// the deterministic plugin-id start order, then enters the post-ready loop:
/// it reads `invoke-action` (emitting progress and a terminal outcome),
/// `cancel` (recording receipt), and `shutdown` (acknowledging and exiting).
/// Failure modes hang or exit at the named handshake phase before reaching
/// the post-ready loop.
fn run_persistent(
    mode: &str,
    hello: &str,
    generation: u64,
    record_dir: Option<&str>,
) -> Result<(), u8> {
    let plugin_id = parse_plugin_id(hello);
    if let Some(dir) = record_dir {
        append_startup_sequence(dir, &plugin_id);
        write_pid_file(dir, &plugin_id);
    }

    if mode == "persistent-hello-hang" {
        // Never send hello-ack: the host times out awaiting hello-ack.
        hang_forever();
    }
    if mode == "persistent-protocol" {
        // hello-ack with a drifted generation: a closed-protocol fault.
        emit(&frame(
            "hello-ack",
            generation + 1,
            r#"{"provider_name":"x","protocol":1}"#,
        ));
        hang_forever();
    }

    emit(&frame(
        "hello-ack",
        generation,
        r#"{"provider_name":"fixture","protocol":1}"#,
    ));

    if mode == "persistent-crash-after-ack" {
        // Exit immediately so the host's configure write or ready read fails.
        std::process::exit(1);
    }

    let configure_line = next_line()?;

    if mode == "persistent-ready-hang" {
        // configure received, but never ready: the host times out awaiting ready.
        hang_forever();
    }

    emit(&frame(
        "ready",
        generation,
        &persistent_ready_payload(mode, &configure_line),
    ));

    if mode == "persistent-secret-protocol" {
        // The secret-bearing ready capability triggered a host parse fault that
        // carries the secret verbatim; hang so the host's bounded read resolves.
        hang_forever();
    }

    if mode == "persistent-illegal-bytes" {
        // Emit an unsolicited protocol frame after ready: a host health probe
        // must mark this candidate a protocol fault while it is still alive.
        emit(&progress_frame(generation, 1));
        hang_forever();
    }

    if mode == "persistent-ready-then-exit" {
        // Reached ready; then exit on its own so the host observes a ready
        // process that exited (no auto-restart).
        std::thread::sleep(std::time::Duration::from_millis(150));
        std::process::exit(0);
    }

    persistent_ready_loop(mode, generation, &plugin_id, record_dir)
}

/// Read requests after a persistent candidate reaches Ready.
fn persistent_ready_loop(
    mode: &str,
    generation: u64,
    plugin_id: &str,
    record_dir: Option<&str>,
) -> Result<(), u8> {
    if mode == "persistent-descendant-hang" {
        spawn_escaping_descendant(record_dir, plugin_id);
    }
    loop {
        let line = next_line()?;
        match frame_type(&line).as_deref() {
            Some("invoke-action") => {
                emit_persistent_invocation(mode, generation, record_dir, plugin_id);
            }
            Some("cancel") => {
                record_cancel_received(record_dir, plugin_id);
                if mode == "persistent-cancel-then-terminal" {
                    emit(&frame(
                        "outcome",
                        generation,
                        r#"{"kind":"navigate","route_id":"r.home","activation":{}}"#,
                    ));
                }
            }
            Some("shutdown") => {
                emit_persistent_ack(mode, generation)?;
                return Ok(());
            }
            _ => {}
        }
    }
}

/// The `ready` payload for a persistent mode. `persistent-secret-protocol`
/// echoes the resolved Configure secret as an invalid capability so the host
/// parse fault carries the secret verbatim (redaction proof); every other mode
/// reports its declared/undeclared capabilities.
fn persistent_ready_payload(mode: &str, configure_line: &str) -> String {
    if mode == "persistent-secret-protocol" {
        let secret = extract_first_secret_value(configure_line);
        return match secret {
            Some(value) => format!(r#"{{"capabilities":[{}]}}"#, json_string(&value)),
            None => r#"{"capabilities":["actions"]}"#.to_owned(),
        };
    }
    persistent_capabilities(mode)
}

/// Extract the `"type"` field from a JSONL line as an owned string.
fn frame_type(line: &str) -> Option<String> {
    parse_host_line(line).and_then(|value| value.get("type")?.as_str().map(str::to_owned))
}

/// Emit the post-`invoke-action` scenario for a persistent candidate. The
/// default is three progress events then a Navigate outcome (the happy
/// repeated-invocation path). `persistent-invoke-hang` emits one progress and
/// never a terminal (cancel/timeout evidence). `persistent-invoke-then-crash`
/// emits one progress then exits 1 (post-Ready crash during invocation).
fn emit_persistent_invocation(
    mode: &str,
    generation: u64,
    record_dir: Option<&str>,
    plugin_id: &str,
) {
    match mode {
        "persistent-invoke-hang" | "persistent-cancel-then-terminal" => {
            emit(&progress_frame(generation, 1));
        }
        "persistent-timeout-then-terminal" => {
            emit(&progress_frame(generation, 1));
            wait_for_late_terminal_signal(record_dir, plugin_id);
            emit(&frame(
                "outcome",
                generation,
                r#"{"kind":"navigate","route_id":"r.late","activation":{}}"#,
            ));
        }
        "persistent-invoke-then-crash" => {
            emit(&progress_frame(generation, 1));
            std::process::exit(1);
        }
        _ => {
            for seq in 1..=3u16 {
                emit(&progress_frame(generation, seq));
            }
            emit(&frame(
                "outcome",
                generation,
                r#"{"kind":"navigate","route_id":"r.home","activation":{}}"#,
            ));
        }
    }
}

/// Wait for the host harness to confirm that its invocation timeout terminal
/// has been published before emitting the deliberately late provider terminal.
/// The bound prevents a standalone fixture process from waiting forever.
fn wait_for_late_terminal_signal(dir: Option<&str>, plugin_id: &str) {
    let marker =
        dir.map(|dir| std::path::Path::new(dir).join(format!("{plugin_id}.emit-late-terminal")));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if marker.as_ref().is_some_and(|path| path.exists()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Record that the host sent a `cancel` frame to this candidate (cancel
/// delivery evidence for CW10-09). Best-effort; never affects the protocol.
fn record_cancel_received(dir: Option<&str>, plugin_id: &str) {
    if let Some(dir) = dir {
        let path = std::path::Path::new(dir).join(format!("{plugin_id}.cancel"));
        let _ = std::fs::write(path, b"1");
    }
}

/// Emit the persistent shutdown-ack scenario. The default is a valid ack; the
/// `persistent-ack-*` fault modes diverge to prove the strict shutdown-ack
/// validation produces a typed cleanup failure while still killing/reaping.
fn emit_persistent_ack(mode: &str, generation: u64) -> Result<(), u8> {
    match mode {
        "persistent-ack-wrong-kind" => {
            // A progress frame instead of an ack: wrong kind.
            emit(&progress_frame(generation, 1));
            hang_forever();
        }
        "persistent-ack-missing" => hang_forever(),
        "persistent-ack-eof-before" => {
            // Exit without acknowledging: EOF before the ack.
            Ok(())
        }
        "persistent-ack-data-after" => {
            emit(&frame("shutdown-ack", generation, "{}"));
            emit(&frame(
                "outcome",
                generation,
                r#"{"kind":"notice","severity":"info","message":"stray"}"#,
            ));
            Ok(())
        }
        _ => {
            emit(&frame("shutdown-ack", generation, "{}"));
            Ok(())
        }
    }
}

/// Spawn a descendant in its own process group that inherits the provider's
/// pipes and lingers. On Unix `process_group(0)` escapes the leader's group, so
/// the supervisor's group reap cannot reach it and the inherited pipes stay open.
#[cfg(unix)]
fn spawn_escaping_descendant(record_dir: Option<&str>, plugin_id: &str) {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().unwrap_or_else(|error| {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "fixture: cannot resolve current exe: {error}");
        std::process::exit(3);
    });
    let mut command = std::process::Command::new(exe);
    command
        .arg("persistent-escape-child")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .process_group(0);
    if let Ok(child) = command.spawn()
        && let Some(dir) = record_dir
    {
        let path = std::path::Path::new(dir).join(format!("{plugin_id}.descendant-pid"));
        let _ = std::fs::write(path, child.id().to_string());
    }
}

/// Non-Unix fallback: no escaping descendant (Windows process-tree semantics
/// differ); the cleanup assertion is exercised only on Unix.
#[cfg(not(unix))]
fn spawn_escaping_descendant(_record_dir: Option<&str>, _plugin_id: &str) {}

/// The `ready` capabilities the persistent fixture reports for a mode.
///
/// `persistent-undeclared-cap` reports a capability the host manifest did not
/// declare, proving the capability-subset rejection. Every other mode reports
/// only `actions`, a subset of any reasonable declaration.
fn persistent_capabilities(mode: &str) -> String {
    if mode == "persistent-undeclared-cap" {
        r#"{"capabilities":["actions","panels"]}"#.to_owned()
    } else {
        r#"{"capabilities":["actions"]}"#.to_owned()
    }
}

/// Extract the plugin id the host sent in its `hello` frame.
fn parse_plugin_id(hello: &str) -> String {
    parse_host_line(hello)
        .and_then(|parsed| {
            parsed
                .get("payload")?
                .get("plugin_id")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Append one plugin id line to a shared startup-sequence file. The supervisor
/// starts candidates sequentially, so there is no concurrent append; the file
/// records the deterministic start order.
fn append_startup_sequence(dir: &str, plugin_id: &str) {
    let path = std::path::Path::new(dir).join("startup-sequence.txt");
    let _ = std::fs::create_dir_all(dir);
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = writeln!(file, "{plugin_id}");
}

/// Write this candidate's own pid to `{dir}/{plugin_id}.pid` so a test can prove
/// the supervisor reaped the exact process (and did not orphan it).
fn write_pid_file(dir: &str, plugin_id: &str) {
    let path = std::path::Path::new(dir).join(format!("{plugin_id}.pid"));
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(path, std::process::id().to_string());
}

fn hang_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
