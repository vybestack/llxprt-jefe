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

use std::io::Write;
use std::process::ExitCode;
use std::process::Stdio;

use jefe::runtime::provider::protocol::{Direction, parse_message};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

fn run() -> Result<(), u8> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "happy".to_owned());
    let record_dir = std::env::args().nth(2);

    // A hanging descendant that keeps the inherited pipes open. Spawned by the
    // descendant-hang mode; it never speaks the protocol.
    if mode == "descendant-hang-child" {
        hang_forever();
    }

    let hello = next_line()?;
    let generation = parse_generation(&hello);

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

/// Extract the fixed generation from the host `hello` frame.
fn parse_generation(line: &str) -> u64 {
    parse_message(line.as_bytes(), Direction::HostToProvider).map_or(1, |parsed| parsed.generation)
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

fn frame(kind: &str, generation: u64, payload: &str) -> String {
    format!(
        "{{\"protocol\":1,\"type\":\"{kind}\",\"request_id\":\"h-000001\",\"generation\":{generation},\"payload\":{payload}}}"
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
        r#"{{"id":"vendor.pkg.token","kind":"string","required":false,"default":{json},"restart":"none"}}"#,
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

fn hang_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
