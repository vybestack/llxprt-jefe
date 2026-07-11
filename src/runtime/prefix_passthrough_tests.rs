//! Real-tmux integration tests for tmux prefix passthrough (#200).
//!
//! These tests prove that when jefe disables the tmux prefix (`prefix None` /
//! `prefix2 None`, as [`super::disable_prefix_for_passthrough`] does in
//! production), application control chords written through an attached
//! `tmux attach-session` client reach the pane child unchanged and in order.
//!
//! They use a *private, per-test* tmux socket (not jefe's process-global
//! [`super::jefe_tmux_socket_path`]) so they never collide with the real jefe
//! server or with sibling tests. They are skipped when `tmux` is unavailable
//! (mirroring the harness driver's availability guard).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// Panic helpers that keep the test clippy-clean under `unwrap_used` /
/// `expect_used` (both `warn`, denied under `-D warnings`), matching the
/// `tests/support/mod.rs` convention used by the rest of the suite.
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

/// Unique suffix so parallel test invocations never share a socket. Returning
/// the sequence and deriving both the socket path and the session name from
/// the *same* value keeps them aligned for debugging (#200 review feedback).
static SOCKET_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_session_handle() -> (PathBuf, String) {
    let seq = SOCKET_SEQ.fetch_add(1, Ordering::Relaxed);
    let thread_id = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::thread::current().id().hash(&mut hasher);
        hasher.finish()
    };
    let socket = std::env::temp_dir().join(format!(
        "jefe-prefix-test-{}-{thread_id}-{seq}.sock",
        std::process::id(),
    ));
    let session = format!("passthrough-{seq}");
    (socket, session)
}

/// Whether a usable `tmux` binary is on PATH.
fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A live tmux server + session scoped to a private socket, cleaned up on drop.
struct IsolatedTmux {
    socket: PathBuf,
    session: String,
}

impl IsolatedTmux {
    /// Build a `tmux -f /dev/null -S <socket> ...` command (the same base flags
    /// jefe's [`super::tmux_command`] uses).
    fn tmux(&self) -> Command {
        let mut cmd = Command::new("tmux");
        cmd.args(["-f", "/dev/null", "-S", &self.socket.to_string_lossy()]);
        cmd
    }

    /// Start a detached session running `cat -v`, which echoes every received
    /// control byte in a visible caret form (Ctrl-X -> "^X", Ctrl-B -> "^B",
    /// Ctrl-C -> "^C"). That makes the bytes that reached the child observable
    /// via `capture-pane` without a custom reader binary.
    ///
    /// When `prefix_disabled` is true, the prefix is disabled using the
    /// *production* option list ([`super::prefix_disable_option_names`]) so a
    /// regression in the real helper (wrong option name, omitted `prefix2`) is
    /// caught here, not just the tmux concept in isolation (#200 review).
    fn new(prefix_disabled: bool) -> Self {
        let (socket, session) = next_session_handle();

        let instance = Self {
            socket: socket.clone(),
            session: session.clone(),
        };

        // Clean any stale socket/server from a prior (crashed) run.
        let _ = instance
            .tmux()
            .args(["kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_file(&socket);

        let status = instance
            .tmux()
            .args([
                "new-session",
                "-d",
                "-s",
                &session,
                "-x",
                "120",
                "-y",
                "24",
                "cat",
                "-v",
            ])
            .status()
            .or_panic("tmux new-session should spawn");
        assert!(status.success(), "tmux new-session failed");

        // Match jefe's finalize_local_session: keep the pane alive if the child
        // exits, then disable the prefix using the production option set.
        instance.run(&["set-option", "-t", &session, "remain-on-exit", "on"]);

        if prefix_disabled {
            for option in super::prefix_disable_option_names() {
                instance.run(&["set-option", "-t", &session, option, "None"]);
            }
            // Assert the production helper's options actually took effect on
            // this isolated session: both must read back as `None`.
            for option in super::prefix_disable_option_names() {
                let value = instance.show_option(option);
                assert_eq!(
                    value, "None",
                    "production prefix option {option} should be None, got {value:?}"
                );
            }
        }

        instance
    }

    fn run(&self, args: &[&str]) {
        let status = self
            .tmux()
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .or_panic(&format!("tmux {args:?} should run"));
        assert!(status.success(), "tmux {args:?} exited non-zero");
    }

    /// Read back a tmux option value for this session (trimmed).
    fn show_option(&self, option: &str) -> String {
        let output = self
            .tmux()
            .args(["show-options", "-t", &self.session, option])
            .output()
            .or_panic("show-options should run");
        assert!(
            output.status.success(),
            "show-options failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let line = String::from_utf8_lossy(&output.stdout);
        // show-options prints "<name> <value>"; return the value token.
        line.split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned()
    }

    /// Capture the visible pane text (what `cat -v` echoed).
    fn capture(&self) -> String {
        let output = self
            .tmux()
            .args(["capture-pane", "-p", "-t", &self.session])
            .output()
            .or_panic("capture-pane should run");
        assert!(
            output.status.success(),
            "capture-pane failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Run a `display-message -p` format query and return its trimmed stdout.
    fn display(&self, format: &str) -> String {
        let output = self
            .tmux()
            .args(["display-message", "-p", "-t", &self.session, format])
            .output()
            .or_panic("display-message should run");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

impl Drop for IsolatedTmux {
    fn drop(&mut self) {
        let _ = self
            .tmux()
            .args(["kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Spawn a `tmux attach-session` client on a PTY (exactly what jefe's
/// [`super::AttachedViewer`] does), write a byte sequence to the PTY master,
/// and return the pane contents after a short settle window.
fn send_through_attach_client(tmux: &IsolatedTmux, bytes: &[u8]) -> String {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .or_panic("openpty");

    let mut cmd = CommandBuilder::new("tmux");
    cmd.args(["-f", "/dev/null", "-S", &tmux.socket.to_string_lossy()]);
    cmd.arg("attach-session");
    cmd.arg("-t");
    cmd.arg(&tmux.session);
    cmd.env("TERM", "xterm-256color");

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .or_panic("spawn tmux attach-session");

    let reader = pair
        .master
        .try_clone_reader()
        .or_panic("clone attach-client reader");
    let mut writer = pair
        .master
        .take_writer()
        .or_panic("take attach-client writer");

    // Let the attach client settle into raw mode before sending.
    std::thread::sleep(Duration::from_millis(700));

    // Drop the reader without blocking; the attach client's initial terminal
    // output must not be mistaken for child echo.
    drop(reader);

    writer
        .write_all(bytes)
        .or_panic("write to attach-client PTY");
    writer.flush().or_panic("flush attach-client PTY");

    // Give the client time to forward the bytes and `cat -v` time to echo.
    std::thread::sleep(Duration::from_millis(800));

    let captured = tmux.capture();

    // Explicitly terminate the attach client and drop the writer/PTY so the
    // client never lingers across the next test (which could otherwise block
    // `kill-server` in `IsolatedTmux::drop` waiting for the client to detach).
    drop(writer);
    drop(pair);
    let _ = child.kill();
    let _ = child.wait();

    captured
}

/// Lines from a pane capture that contain a caret (control-byte echo from
/// `cat -v`), trimmed of trailing whitespace.
fn caret_echo_lines(pane: &str) -> Vec<&str> {
    pane.lines()
        .map(str::trim_end)
        .filter(|line| line.contains('^'))
        .collect()
}

/// With the default prefix (`C-b`) active, the `0x02` byte in a Ctrl-X Ctrl-B
/// chord is consumed by the attach client's prefix key table and never reaches
/// the child. This is the regression this test guards against (#200).
#[test]
fn default_prefix_eats_ctrl_b_byte_in_chord() {
    if !tmux_available() {
        return;
    }
    let tmux = IsolatedTmux::new(false);
    let pane = send_through_attach_client(&tmux, b"\x18\x02");
    let echoed = caret_echo_lines(&pane).join("\n");
    // The 0x18 arrives (^X) but the 0x02 is swallowed by the prefix, proving
    // the collision exists when the prefix is left enabled.
    assert!(
        echoed.contains("^X") && !echoed.contains("^B"),
        "default prefix must eat 0x02; pane echoed: {echoed:?}"
    );
}

/// With jefe's prefix disabled (`prefix None` / `prefix2 None`), the full
/// Ctrl-X Ctrl-B chord (`0x18 0x02`) reaches the child unchanged and in order.
/// Acceptance criterion #1 for #200.
#[test]
fn prefix_disabled_ctrl_x_ctrl_b_reaches_child() {
    if !tmux_available() {
        return;
    }
    let tmux = IsolatedTmux::new(true);
    let pane = send_through_attach_client(&tmux, b"\x18\x02");
    let echoed = caret_echo_lines(&pane);
    let joined = echoed.join("\n");
    assert!(
        joined.contains("^X") && joined.contains("^B"),
        "Ctrl-X Ctrl-B must both reach the child; pane: {joined:?}"
    );
    // Order: ^X must appear before ^B on the echoed line.
    let line = echoed
        .iter()
        .find(|line| line.contains("^X"))
        .copied()
        .or_panic(&format!("no ^X echo: {joined:?}"));
    let Some(x) = line.find("^X") else {
        panic!("^X present: {line:?}");
    };
    let Some(b) = line.find("^B") else {
        panic!("^B present: {line:?}");
    };
    assert!(
        x < b,
        "^X must precede ^B (in-order delivery); line: {line:?}"
    );
}

/// Ctrl-X Ctrl-X (`0x18 0x18`) must reach the child unchanged and in order.
/// This chord is NOT a prefix collision, so it must work regardless of the
/// prefix setting; tested here with the prefix disabled (jefe's production
/// config) to lock the acceptance criterion #2 for #200.
#[test]
fn prefix_disabled_ctrl_x_ctrl_x_reaches_child() {
    if !tmux_available() {
        return;
    }
    let tmux = IsolatedTmux::new(true);
    let pane = send_through_attach_client(&tmux, b"\x18\x18");
    let echoed = caret_echo_lines(&pane);
    let joined = echoed.join("\n");
    let line = echoed
        .iter()
        .find(|line| line.matches("^X").count() >= 2)
        .copied()
        .or_panic(&format!("expected two ^X echoes on one line: {joined:?}"));
    let mut positions = line.match_indices("^X");
    let first = positions.next().map(|(i, _)| i);
    let second = positions.next().map(|(i, _)| i);
    assert!(
        first.is_some() && second.is_some() && first < second,
        "two ^X must echo in order; line: {line:?}"
    );
}

/// Ctrl-C (`0x03`) reaches the child and delivers SIGINT. Acceptance criterion
/// #3 for #200.
///
/// Because SIGINT kills `cat`, "exactly once" is not observably distinguishable
/// from "at least once" through tmux's dead-state (tmux reports the same dead
/// state either way). So this test proves the stronger *delivery* property:
/// the byte is forwarded by the attach client, the child dies, and it dies
/// specifically of SIGINT. The signal is read via the portable tmux format
/// `#{pane_dead_signal}` (which yields `int`), rather than parsing the
/// "Pane is dead" banner text — that banner is terminal/locale dependent and
/// is not reliably present in `capture-pane` output on Linux CI. The
/// byte-level "unchanged and in order" guarantee for the non-killing chords is
/// covered by the sibling tests above.
#[test]
fn prefix_disabled_ctrl_c_reaches_child_via_sigint() {
    if !tmux_available() {
        return;
    }
    let tmux = IsolatedTmux::new(true);

    // Write a single 0x03. cat receives SIGINT and dies.
    let _pane = send_through_attach_client(&tmux, b"\x03");

    // Poll pane_dead; cat should be dead from SIGINT.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut became_dead = false;
    while Instant::now() < deadline {
        if tmux.display("#{pane_dead}") == "1" {
            became_dead = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        became_dead,
        "Ctrl-C must reach the child (pane should be dead from SIGINT)"
    );

    // #{pane_dead_signal} yields the signal name tmux recorded for the dead
    // pane. SIGINT ("int") proves the death cause was Ctrl-C, not an unrelated
    // exit. This format is stable across macOS/Linux tmux builds, unlike the
    // "Pane is dead (signal int, ...)" banner rendered into capture-pane.
    let signal = tmux.display("#{pane_dead_signal}");
    assert_eq!(
        signal, "int",
        "dead pane should report SIGINT; got signal={signal:?}"
    );
}
