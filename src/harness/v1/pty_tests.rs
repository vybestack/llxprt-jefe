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
