//! PtySession behavior tests (issue #381).
//!
//! Terminal identity/mode queries emitted by the app-under-test must receive
//! standard responses from the harness's embedded terminal model. Real TUIs
//! (crossterm-based) probe device attributes during raw-mode setup and never
//! enable their input pipeline if the answer does not arrive.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::super::contract::Size;
use super::PtySession;

#[test]
fn device_attribute_query_receives_response_bytes_on_stdin() {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        // Raw mode first (as real TUIs do), then emit a primary device
        // attributes query, block until at least three response bytes arrive
        // on stdin, then print the marker.
        "stty raw -echo; printf '\\033[c'; head -c 3 >/dev/null; echo GOT_RESPONSE".to_string(),
    ];
    let mut session = PtySession::launch(
        &argv,
        &env,
        std::env::temp_dir().as_path(),
        Size { cols: 80, rows: 24 },
    )
    .unwrap_or_else(|err| panic!("launch: {err}"));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed = String::new();
    while Instant::now() < deadline {
        observed = session
            .stream_text()
            .unwrap_or_else(|err| panic!("stream: {err}"));
        if observed.contains("GOT_RESPONSE") {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = session.stop();
    assert!(
        observed.contains("GOT_RESPONSE"),
        "device-attributes response never reached the app; stream: {observed:?}"
    );
}

/// A command that ignores TERM proves teardown does not fabricate an exit
/// code. portable-pty maps signal death to `code: 1` (`status.code()` is
/// `None` for a signalled process, and it falls back to `1`), which is
/// indistinguishable from a real `exit 1`. Teardown must therefore observe
/// the command's own exit before signalling.
#[test]
fn signalled_command_does_not_masquerade_as_exit_one() {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        // Ignore TERM, then exit 3 on its own shortly after teardown starts.
        "trap '' TERM; sleep 0.5; exit 3".to_string(),
    ];
    let mut session = PtySession::launch(
        &argv,
        &env,
        std::env::temp_dir().as_path(),
        Size { cols: 80, rows: 24 },
    )
    .unwrap_or_else(|err| panic!("launch: {err}"));

    let exit = session.stop().unwrap_or_else(|err| panic!("stop: {err}"));
    assert_ne!(
        exit.exit_code,
        Some(1),
        "teardown reported the signal fallback code instead of the command's exit"
    );
}
