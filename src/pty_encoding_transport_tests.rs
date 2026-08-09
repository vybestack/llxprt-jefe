//! Real-multiplexer transport test for the `Ctrl+Enter` nudge chord (issue #692).
//!
//! `Ctrl+Enter` is the chord hosted agents bind to "steer"/"nudge" a prompt
//! that is already running. Jefe's whole part in delivering it is the bytes
//! `key_to_bytes` writes into the multiplexer client's PTY, so that is what
//! this test exercises: the encoder's real output, written to a real
//! `attach-session` client, in jefe's own topology.
//!
//! The encoder unit tests next door assert that jefe emits the bytes it
//! intended to emit. That is not the same claim, and the difference is the
//! whole bug: psmux has no CSI-u branch, so the `CSI 13 ; 5 u` form chosen for
//! tmux in issue #627 was consumed and dropped, and the chord reached no agent
//! on Windows at all while the unit tests stayed green. Only a test with a real
//! client on the far end can tell "jefe wrote the bytes" apart from "the chord
//! arrived".
//!
//! What the client made of the bytes is read back through root key bindings
//! that record the recognised key's own name in a server option. Binding the
//! neighbouring keys as well as `C-Enter` is deliberate: the regression this
//! guards against is not only "nothing arrived" but "a bare `Enter` arrived",
//! which would submit the prompt instead of steering it — a worse outcome than
//! silence, and one a boolean flag could not distinguish.
//!
//! Windows-only, because Windows is the only platform whose multiplexer is
//! psmux and the defect is psmux's alone.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use iocraft::prelude::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use jefe::runtime::{LocalPlatform, MultiplexerIsolation, MultiplexerPlan};

use super::key_to_bytes;

/// Server option the root bindings record the observed key name in.
const OBSERVED_KEY_OPTION: &str = "@jefe-observed-key";

/// Value the option holds before any key has been recognised.
const NOTHING_OBSERVED: &str = "nothing";

/// Session the attach client connects to.
const SESSION: &str = "jefe-ctrl-enter";

/// How long the client is given to attach, and the chord to arrive.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(20);

/// Gap between polls while waiting on the client or the chord.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Byte written to prove the client's input path is live before the chord is
/// sent. `CR` is the plainest key there is and is bound alongside the chord.
const PROBE_BYTE: &[u8] = b"\r";

/// Consecutive quiet polls that prove every probe byte has drained.
const QUIET_POLLS: u32 = 5;

/// Session-routing variables that must never be inherited by a client jefe
/// starts. The test process itself frequently runs *inside* a jefe-managed
/// pane, and a client that looks nested refuses to attach at all — which would
/// turn a transport failure into a silent no-op.
///
/// This mirrors the production list in `runtime::attach`'s
/// `scrub_inherited_multiplexer_env`. That function is not reachable from this
/// crate, so the list is restated here; keeping it identical — including
/// `TMUX_TMPDIR`, which is socket routing rather than session routing — is what
/// stops the rebuilt attach command from drifting away from the real one.
const INHERITED_SESSION_VARS: [&str; 5] = [
    "TMUX",
    "TMUX_PANE",
    "TMUX_TMPDIR",
    "PSMUX_SESSION",
    "PSMUX_TARGET_SESSION",
];

/// Panic helpers that keep the test clippy-clean under `unwrap_used` /
/// `expect_used`, matching the convention in `prefix_passthrough_tests.rs`.
trait PanicResult<T> {
    fn or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> PanicResult<T> for Result<T, E> {
    fn or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> PanicResult<T> for Option<T> {
    fn or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

/// Whether this environment promises a usable multiplexer.
///
/// CI sets `JEFE_REQUIRE_PSMUX` on the native Windows job. Where it is set, an
/// unresolvable multiplexer is a failure rather than a reason to skip: a test
/// that quietly does nothing is how a broken transport survives a green build.
fn psmux_is_required() -> bool {
    std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1")
}

/// The multiplexer binary to probe: the same override variable production
/// honours, falling back to the platform's first candidate name on `PATH`.
fn resolve_multiplexer_binary(platform: LocalPlatform) -> PathBuf {
    let override_name = match platform {
        LocalPlatform::Unix => "JEFE_TMUX_BIN",
        LocalPlatform::Windows => "JEFE_PSMUX_BIN",
    };
    if let Some(explicit) = std::env::var_os(override_name).filter(|value| !value.is_empty()) {
        return PathBuf::from(explicit);
    }
    match platform {
        LocalPlatform::Unix => PathBuf::from("tmux"),
        LocalPlatform::Windows => PathBuf::from("psmux.exe"),
    }
}

/// A throwaway namespace, torn down on every exit path including unwinding.
struct ScratchServer {
    plan: MultiplexerPlan,
}

impl ScratchServer {
    /// Reserve a namespace no other test or live session can be addressing.
    ///
    /// The plan is built from the platform policy and the resolved binary
    /// directly rather than from `MultiplexerPlan::current`, because the
    /// production handle is only available once startup has resolved the
    /// effective config — and a transport test wants a server it owns outright
    /// in any case, so that `kill-server` teardown can never reach a live one.
    fn reserve() -> Result<Self, String> {
        let platform = LocalPlatform::current();
        let executable = resolve_multiplexer_binary(platform);
        let namespace = format!("jefe-ctrl-enter-{}", std::process::id());
        let plan = MultiplexerPlan::for_platform(
            platform,
            executable.clone(),
            MultiplexerIsolation::Namespace(namespace),
        )
        .map_err(|error| format!("{error}"))?;

        // `for_platform` validates the executable's *name*, not its presence, so
        // a plan for a multiplexer that was never installed builds cleanly and
        // only fails later inside the pty spawn — as an `os error 2` that reads
        // like a defect in the transport rather than a missing dependency. Jobs
        // that do not install a multiplexer have to reach the skip path here.
        let mut probe = plan.command();
        probe.arg("-V");
        if !probe.output().is_ok_and(|output| output.status.success()) {
            return Err(format!(
                "`{}` is not runnable on this machine",
                executable.display()
            ));
        }

        Ok(Self { plan })
    }

    fn plan(&self) -> &MultiplexerPlan {
        &self.plan
    }
}

impl Drop for ScratchServer {
    /// A failed teardown stays silent: this runs during unwinding, where
    /// panicking again would abort the process over a scratch server.
    fn drop(&mut self) {
        let _ = run(&self.plan, &["kill-server"]);
    }
}

/// Run one multiplexer control command and return its trimmed stdout.
///
/// `MultiplexerPlan::command` is the production constructor, so the isolation
/// flags and the native session-variable scrubbing are the real ones.
fn run(plan: &MultiplexerPlan, args: &[&str]) -> String {
    let mut command: Command = plan.command();
    command.args(args);
    match command.output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        Err(_) => String::new(),
    }
}

/// Bind `key` at the root table so that pressing it records its own name.
///
/// Recording the name rather than a bare flag is what lets the assertion say
/// which key arrived, not merely that one did.
fn record_key(plan: &MultiplexerPlan, key: &str) {
    let _ = run(
        plan,
        &[
            "bind-key",
            "-n",
            key,
            "set-option",
            "-g",
            OBSERVED_KEY_OPTION,
            key,
        ],
    );
}

/// The key name the client has recognised so far.
fn observed_key(plan: &MultiplexerPlan) -> String {
    run(plan, &["show-options", "-gqv", OBSERVED_KEY_OPTION])
}

/// Poll `probe` until it reports ready, or the settle timeout expires.
fn wait_until(mut probe: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    loop {
        if probe() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Drive the client with a probe key until one actually reaches a binding.
///
/// `list-clients` reports a client the moment it registers with the server,
/// which is earlier than the moment its terminal starts delivering keystrokes.
/// Bytes written into that window are discarded, so a test that trusts
/// registration alone reports "nothing arrived" for a chord that was in fact
/// encoded correctly — the released psmux build loses the chord this way
/// whenever the machine is loaded enough to widen the window. A probe key that
/// has been *observed* is the only evidence that the path under test is live,
/// so it is written repeatedly rather than once.
fn wait_for_live_input(plan: &MultiplexerPlan, writer: &mut Box<dyn Write + Send>) -> bool {
    wait_until(|| {
        if writer.write_all(PROBE_BYTE).is_err() || writer.flush().is_err() {
            return false;
        }
        observed_key(plan) != NOTHING_OBSERVED
    })
}

/// Clear the observed key and wait until it stays cleared.
///
/// A probe byte still in flight would otherwise land after the reset and be
/// read as the chord's result, reporting `Enter` for a chord that was never
/// delivered. Requiring a run of quiet polls drains them before the real
/// measurement starts.
fn settle_after_probe(plan: &MultiplexerPlan) -> bool {
    let deadline = Instant::now() + SETTLE_TIMEOUT;
    loop {
        let _ = run(
            plan,
            &["set-option", "-g", OBSERVED_KEY_OPTION, NOTHING_OBSERVED],
        );

        let mut quiet = 0;
        while quiet < QUIET_POLLS {
            std::thread::sleep(POLL_INTERVAL);
            if observed_key(plan) != NOTHING_OBSERVED {
                break;
            }
            quiet += 1;
        }

        if quiet >= QUIET_POLLS {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
    }
}

/// The attach command jefe runs, built from the same resolved executable,
/// isolation flags and environment scrubbing the production path uses.
fn attach_command(plan: &MultiplexerPlan) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(plan.executable());
    for arg in plan.base_args() {
        cmd.arg(arg);
    }
    cmd.arg("attach-session");
    cmd.arg("-t");
    cmd.arg(SESSION);
    for variable in INHERITED_SESSION_VARS {
        cmd.env_remove(OsString::from(variable));
    }
    cmd.env("TERM", "xterm-256color");
    cmd
}

/// Reap the attach client on every exit path, so a failed assertion cannot
/// leave a client holding the scratch server open against its teardown.
struct AttachClientGuard {
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
}

impl AttachClientGuard {
    fn kill_now(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for AttachClientGuard {
    fn drop(&mut self) {
        self.kill_now();
    }
}

/// Write `bytes` to a live attach client and return the key it recognised.
fn key_recognised_for(plan: &MultiplexerPlan, bytes: &[u8]) -> String {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .or_panic("open a pty for the attach client");

    let child = pair
        .slave
        .spawn_command(attach_command(plan))
        .or_panic("spawn the attach client");
    let mut guard = AttachClientGuard { child: Some(child) };

    let mut writer = pair
        .master
        .take_writer()
        .or_panic("take the attach client's pty writer");

    let attached = wait_until(|| !run(plan, &["list-clients", "-t", SESSION]).is_empty());
    assert!(
        attached,
        "the attach client never attached, so nothing about the chord could be observed"
    );

    let live = wait_for_live_input(plan, &mut writer);
    assert!(
        live,
        "the attach client never delivered even a plain Enter, so a silent chord \
         would prove nothing about its encoding"
    );

    let settled = settle_after_probe(plan);
    assert!(
        settled,
        "the probe keys never drained, so the chord could not be measured on its own"
    );

    writer
        .write_all(bytes)
        .or_panic("write the chord to the attach client");
    writer.flush().or_panic("flush the attach client's pty");

    wait_until(|| observed_key(plan) != NOTHING_OBSERVED);
    let observed = observed_key(plan);

    drop(writer);
    drop(pair);
    guard.kill_now();

    observed
}

/// The bytes jefe's encoder produces for `Ctrl+Enter` must be a chord the
/// multiplexer client actually recognises as `Ctrl+Enter`.
///
/// Before issue #692 they were `CSI 13 ; 5 u`, which psmux discards outright:
/// the option stayed at `nothing` because no binding ever fired, and every
/// agent bound to the nudge chord was unreachable through jefe on Windows.
#[test]
fn ctrl_enter_reaches_the_multiplexer_as_ctrl_enter() {
    let scratch = match ScratchServer::reserve() {
        Ok(scratch) => scratch,
        Err(error) => {
            assert!(
                !psmux_is_required(),
                "JEFE_REQUIRE_PSMUX is set but no multiplexer resolved: {error}"
            );
            return;
        }
    };
    let plan = scratch.plan();

    let _ = run(
        plan,
        &[
            "new-session",
            "-d",
            "-s",
            SESSION,
            "-x",
            "120",
            "-y",
            "24",
            "cmd.exe",
        ],
    );
    let _ = run(
        plan,
        &["set-option", "-g", OBSERVED_KEY_OPTION, NOTHING_OBSERVED],
    );
    // The chord under test, plus every neighbour it could be mistaken for.
    for key in ["C-Enter", "Enter", "C-j", "C-m"] {
        record_key(plan, key);
    }

    let mut ctrl_enter = KeyEvent::new(KeyEventKind::Press, KeyCode::Enter);
    ctrl_enter.modifiers = KeyModifiers::CONTROL;
    let bytes = key_to_bytes(&ctrl_enter).or_panic("the encoder must produce bytes for Ctrl+Enter");

    let observed = key_recognised_for(plan, &bytes);

    assert_eq!(
        observed, "C-Enter",
        "the multiplexer client must recognise jefe's Ctrl+Enter bytes ({bytes:?}) as C-Enter, \
         but it saw {observed:?}; an agent bound to the nudge chord never sees the chord"
    );
}
