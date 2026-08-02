//! Behavioral tests for application-socket reaping (issue #586).

use super::*;

fn env_with(value: &str) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    environment.insert(APP_SOCKET_ENV.to_owned(), value.to_owned());
    environment
}

fn workspace() -> tempfile::TempDir {
    tempfile::tempdir().unwrap_or_else(|error| panic!("workspace temp dir: {error}"))
}

/// A per-run socket inside the workspace is this run's to clean up.
#[test]
fn a_socket_inside_the_workspace_is_reaped() {
    let root = workspace();
    let socket = root.path().join("jefe.sock");
    let environment = env_with(&socket.to_string_lossy());

    assert_eq!(socket_to_reap(&environment, root.path()), Some(socket));
}

/// A socket outside the workspace belongs to someone else.
///
/// This is the property that keeps a scenario from killing the developer's real
/// jefe server and every live agent in it.
#[test]
fn a_socket_outside_the_workspace_is_never_reaped() {
    let root = workspace();
    let elsewhere = workspace();
    let ambient = elsewhere.path().join("jefe-501.sock");

    assert_eq!(
        socket_to_reap(&env_with(&ambient.to_string_lossy()), root.path()),
        None
    );
}

/// The fixed shared path this defect was reported against is refused.
#[test]
fn the_fixed_shared_tmp_socket_is_never_reaped() {
    let root = workspace();

    assert_eq!(
        socket_to_reap(&env_with("/tmp/jefe-jsp-preview.sock"), root.path()),
        None,
        "a shared /tmp socket is not this run's to kill, which is exactly why \
         scenarios must use a per-run path instead"
    );
}

/// Traversal out of the workspace does not buy containment.
#[test]
fn a_path_escaping_the_workspace_with_dotdot_is_not_reaped() {
    let root = workspace();
    let escaping = root.path().join("..").join("escaped.sock");

    assert_eq!(
        socket_to_reap(&env_with(&escaping.to_string_lossy()), root.path()),
        None
    );
}

/// Nothing to reap when the scenario never asked for a socket.
#[test]
fn an_absent_or_empty_socket_variable_reaps_nothing() {
    let root = workspace();

    assert_eq!(socket_to_reap(&BTreeMap::new(), root.path()), None);
    assert_eq!(socket_to_reap(&env_with(""), root.path()), None);
}

/// A relative path cannot be shown to be contained, so it is left alone.
///
/// Jefe ignores a relative `JEFE_SOCKET_PATH` for the same reason.
#[test]
fn a_relative_socket_path_is_not_reaped() {
    let root = workspace();

    assert_eq!(socket_to_reap(&env_with("jefe.sock"), root.path()), None);
}

/// A workspace-relative socket must still fit the kernel's `sun_path` limit.
///
/// This is not theoretical margin. On macOS the temp directory canonicalizes
/// through `/private`, which costs 57 bytes before the workspace name is even
/// added, and the workspace name carries a pid, a sequence and a nanosecond
/// hex stamp. `${workspace}/jefe.sock` measures 107 bytes against a 104-byte
/// kernel limit and would simply fail to bind; the one-character name the
/// scenarios use measures 99.
///
/// Pinning it here means a future rename of the workspace prefix fails this
/// test instead of silently breaking the preview scenarios at run time.
#[test]
fn a_workspace_relative_socket_fits_the_unix_socket_path_limit() {
    /// The same conservative bound `runtime::socket` applies, below the
    /// 104-byte macOS and 108-byte Linux kernel limits.
    const SAFE_LIMIT: usize = 100;

    let workspace = crate::harness::v1::workspace::Workspace::allocate()
        .unwrap_or_else(|error| panic!("workspace must allocate: {error}"));
    let socket = workspace.root().join("s");
    let length = socket.as_os_str().len();

    assert!(
        length <= SAFE_LIMIT,
        "a workspace-relative socket must fit the sun_path limit, but {} is {length} bytes; \
         either the workspace name grew or the socket name did",
        socket.display()
    );
}
