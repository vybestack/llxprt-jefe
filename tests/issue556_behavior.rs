//! Issue #556 — serialize managed package installs across jefe processes and
//! make installation atomic (Unix).
//!
//! A1 is a genuine two-process behavioral test: it spawns two copies of the
//! `jefe-issue556-installer` fixture (each driving the production
//! `finalize_local_invocation` boundary) against a shared cache and a counting
//! npm stub, then asserts exactly one install invocation and one identical
//! resolved binary. A7 proves the uvx path is unaffected by the new managed
//! cache locking.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution, VersionSelector};
use jefe::agent_candidate_path::{AgentExecutablePlatform, PathSnapshot};
use jefe::domain::agent_definition::AgentDefinition;
use jefe::runtime::package_runtime::{
    PackageExecutionTarget, finalize_local_invocation, package_invocation,
};

/// Path to the two-process installer fixture (set by Cargo for `[[bin]]`
/// targets of the package under test).
const INSTALLER_FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-issue556-installer");

fn write_executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

/// A counting npm stub that sleeps briefly to guarantee two concurrent
/// installers overlap, then materializes the managed binary. Each invocation
/// appends a witness line to the cache root (the temp build dir's parent,
/// surviving the atomic rebuild), so a single line proves the install ran once.
const COUNTING_NPM: &str = "#!/bin/sh
set -e
echo install >> ../.jefe-lock-witness
sleep 0.3
mkdir -p node_modules/.bin
printf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
";

/// A1: two concurrent installers of the same digest serialize — exactly one
/// performs the install and the other observes a complete cache hit, resolving
/// to an identical managed binary.
#[test]
fn two_concurrent_installers_serialize_to_one_install() {
    let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("workspace: {error}"));
    let bin_dir = workspace.path().join("bin");
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    fs::create_dir_all(&cache).unwrap_or_else(|error| panic!("mkdir cache: {error}"));
    write_executable(&bin_dir.join("npm"), COUNTING_NPM.as_bytes());

    // Start both installers before waiting on either so they genuinely contend.
    let first = spawn_installer(&cache, &bin_dir, "2.0.0");
    let second = spawn_installer(&cache, &bin_dir, "2.0.0");

    let first_output = first
        .wait_with_output()
        .unwrap_or_else(|error| panic!("wait first installer: {error}"));
    let second_output = second
        .wait_with_output()
        .unwrap_or_else(|error| panic!("wait second installer: {error}"));

    assert!(
        first_output.status.success(),
        "first installer failed: {}",
        String::from_utf8_lossy(&first_output.stderr)
    );
    assert!(
        second_output.status.success(),
        "second installer failed: {}",
        String::from_utf8_lossy(&second_output.stderr)
    );

    let first_exec = stdout_path(&first_output.stdout, "first");
    let second_exec = stdout_path(&second_output.stdout, "second");
    assert_eq!(
        first_exec, second_exec,
        "serialized installers resolve an identical managed binary"
    );

    let witness = fs::read_to_string(cache.join(".jefe-lock-witness"))
        .unwrap_or_else(|error| panic!("read npm witness: {error}"));
    let installs = witness.lines().filter(|line| *line == "install").count();
    assert_eq!(
        installs, 1,
        "exactly one of the two concurrent installers performs the install (got {witness:?})"
    );
}

fn stdout_path(stdout: &[u8], which: &str) -> String {
    let text = String::from_utf8(stdout.to_vec())
        .unwrap_or_else(|error| panic!("{which} stdout utf8: {error}"));
    let trimmed = text.trim().to_owned();
    assert!(
        !trimmed.is_empty(),
        "{which} installer resolved no managed binary"
    );
    trimmed
}

fn spawn_installer(cache: &Path, bin_dir: &Path, selector: &str) -> Child {
    let path = std::env::var_os("PATH").unwrap_or_else(|| OsString::from("/usr/bin:/bin"));
    Command::new(INSTALLER_FIXTURE)
        .env_clear()
        .env("PATH", path)
        .env("JEFE_ISSUE556_CACHE", cache)
        .env("JEFE_ISSUE556_BIN", bin_dir)
        .env("JEFE_ISSUE556_SELECTOR", selector)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn installer fixture: {error}"))
}

fn shipped_code_puppy() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.display_name == "Code Puppy")
        .unwrap_or_else(|| panic!("shipped Code Puppy definition"))
}

fn resolve_uvx_candidate(
    bin_dir: &Path,
    selector: &str,
) -> jefe::agent_candidate::ResolvedCandidate {
    let definition = shipped_code_puppy();
    write_executable(&bin_dir.join("uvx"), b"#!/bin/sh\nexit 0\n");
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![bin_dir.to_path_buf()],
        None,
    );
    let selector = VersionSelector::normalize(selector)
        .unwrap_or_else(|error| panic!("normalize selector: {error}"));
    let resolution = AgentCandidateResolver::new(&snapshot, bin_dir.to_path_buf())
        .with_version_selector(selector)
        .resolve(&definition);
    let CandidateResolution::Resolved(candidate) = resolution else {
        panic!("uvx candidate must resolve: {resolution:?}");
    };
    candidate
}

/// A7: the uvx preparation path retains its closed structural semantics and is
/// unaffected by the managed-cache locking — it never manages the cache.
#[test]
fn uvx_preparation_is_unaffected_by_managed_cache_locking() {
    let workspace = tempfile::tempdir().unwrap_or_else(|error| panic!("workspace: {error}"));
    let cache = workspace.path().join("cache");
    fs::create_dir_all(&cache).unwrap_or_else(|error| panic!("mkdir cache: {error}"));
    let candidate = resolve_uvx_candidate(workspace.path(), "0.0.634");

    let invocation = finalize_local_invocation(&candidate, &cache)
        .unwrap_or_else(|error| panic!("uvx finalize: {error}"));

    assert_eq!(
        invocation.executable(),
        candidate.executable(),
        "uvx resolves the runner executable, not a managed cache path"
    );
    let expected: Vec<OsString> = ["--from", "code-puppy==0.0.634", "code-puppy"]
        .iter()
        .map(OsString::from)
        .collect();
    assert_eq!(
        invocation.prefix(),
        expected.as_slice(),
        "uvx keeps its closed structural prefix"
    );

    // The structural remote prefix is also unchanged.
    let remote = package_invocation(&candidate, PackageExecutionTarget::Remote, &cache)
        .unwrap_or_else(|error| panic!("remote uvx invocation: {error}"))
        .unwrap_or_else(|| panic!("remote uvx invocation must exist"));
    assert_eq!(remote.executable(), Path::new("uvx"));
    assert_eq!(remote.prefix(), expected.as_slice());

    // uvx preparation never manages the cache: no digest directory is created.
    let digest_entries = fs::read_dir(&cache).map_or(0, |entries| entries.flatten().count());
    assert_eq!(
        digest_entries, 0,
        "uvx preparation leaves the managed cache untouched"
    );
}
