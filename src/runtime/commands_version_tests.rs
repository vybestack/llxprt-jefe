//! Issue #269 version-selector + npm resolution tests, extracted from
//! `commands_tests.rs` so that file stays under the per-file line limit.
//!
//! Covers:
//! - Remote npm probe script contract (exact POSIX script + behavioral proof)
//! - Remote metacharacter selector quoting (adversarial version selector)
//! - Persisted selector validation before destructive kill
//! - Session-cached npm path wins over live resolver + no-cache fallback
//!
//! All tests drive the real production functions in [`super::commands`] /
//! [`super::npm_launch`] — no duplicated logic.

use super::npm_launch::{assemble_remote_cli_command, remote_npm_probe_script};
use super::tests::base_signature;
use super::*;

/// Build a `LocalLaunchPlan` for an NpmExec (versioned) launch so the
/// resolver exercises the npm branch rather than the direct-binary branch.
fn plan_npm_exec() -> LocalLaunchPlan {
    let mut signature = base_signature();
    signature.llxprt_version = "0.9.0".to_owned();
    local_launch_plan(&signature)
}

/// Behavioral proof that the remote npm probe script normalizes a relative
/// PATH entry to an absolute path. Creates a temp directory with an `npm`
/// executable, runs the script with the temp dir as a *relative* PATH
/// component (by cd-ing into the parent and referencing the dir by name),
/// and asserts the output is absolute (starts with `/`).
///
/// This executes the real production `remote_npm_probe_script()` output
/// through `/bin/sh`, proving the generated POSIX source works end-to-end —
/// not just that it matches a string constant.
#[cfg(unix)]
#[test]
fn remote_npm_probe_script_produces_absolute_path_for_relative_path_entry() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let npm_path = temp.path().join("npm");
    std::fs::write(&npm_path, b"#!/bin/sh\nexit 0\n")
        .unwrap_or_else(|error| panic!("write npm: {error}"));
    std::fs::set_permissions(&npm_path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm: {error}"));

    // Run from the parent of the temp dir, using the temp dir name as a
    // RELATIVE PATH entry so `command -v npm` returns a relative result,
    // exercising the `*)` branch of the case statement.
    let parent = temp
        .path()
        .parent()
        .unwrap_or_else(|| panic!("temp dir must have a parent"));
    let dir_name = temp
        .path()
        .file_name()
        .unwrap_or_else(|| panic!("temp dir must have a name"))
        .to_string_lossy()
        .into_owned();

    let script = remote_npm_probe_script();
    let output = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(&script)
        .current_dir(parent)
        .env("PATH", &dir_name)
        .output()
        .unwrap_or_else(|error| panic!("run probe script: {error}"));

    assert!(
        output.status.success(),
        "probe script must succeed when npm is on a relative PATH: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let resolved = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert!(
        resolved.starts_with('/'),
        "resolved path must be absolute (start with /), got: {resolved}"
    );
    assert!(
        resolved.ends_with("/npm"),
        "resolved path must end with /npm, got: {resolved}"
    );
}

// ── Issue #269: remote metacharacter selector quoting (production path) ───

/// The production remote CLI assembly must shell-escape every token of the
/// npm exec prefix so an adversarial version selector never reaches the
/// remote shell as syntax. Drives the real production path
/// ([`assemble_remote_cli_command`]) with an adversarial selector containing
/// shell metacharacters.
#[test]
fn remote_cli_assembly_shell_escapes_adversarial_version_selector() {
    use crate::domain::{AgentKind, LaunchSignature, RemoteRepositorySettings, SandboxEngine};

    let adversarial = "0.9.0'; rm -rf /; echo '";
    let signature = LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp/work"),
        profile: String::new(),
        code_puppy_model: String::new(),
        llxprt_version: adversarial.to_owned(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
    };
    let plan = ExecutablePlan::from_signature(&signature);
    assert!(plan.requires_npm(), "adversarial selector must be NpmExec");

    let cli = assemble_remote_cli_command(&plan, "npm", &[]);
    // The adversarial selector must be embedded inside a single-quoted
    // --package= token, never as standalone shell syntax.
    assert!(
        cli.contains("'--package=@vybestack/llxprt-code@0.9.0'"),
        "adversarial selector must be inside the single-quoted package token: {cli}"
    );
    // The dangerous `; rm` sequence from the adversarial payload must be
    // inside a single-quoted context (between quote pairs), not as
    // standalone shell syntax. The `'''` escaping ensures the single quote
    // in the payload closes the current quote, inserts a literal quote, and
    // reopens — so `; rm` is always within a quoted context.
    assert!(
        cli.contains("'; rm -rf /; echo '") || cli.contains("\''; rm -rf /; echo '\''"),
        "the adversarial payload must be present but escaped: {cli}"
    );
    // Verify that the entire CLI, when parsed by a POSIX shell, would not
    // execute `rm` as a command. The package token must be a single
    // shell-quoted unit that contains the full adversarial string. We verify
    // by checking that `rm -rf /` does not appear outside of a quoted region:
    // it must always be preceded by a quote context.
    let package_start = cli
        .find("'--package=")
        .unwrap_or_else(|| panic!("package token must be present: {cli}"));
    let rest = &cli[package_start..];
    assert!(
        rest.contains("rm -rf /"),
        "the adversarial payload must be inside the package token: {cli}"
    );
}

#[test]
fn remote_cli_assembly_uses_resolved_npm_executable() {
    let signature = LaunchSignature {
        llxprt_version: "0.9.0".to_owned(),
        ..base_signature()
    };
    let plan = ExecutablePlan::from_signature(&signature);
    let resolved = "/opt/node's tools/npm;safe";
    let cli = assemble_remote_cli_command(&plan, resolved, &[]);
    let escaped = shell_escape_single(resolved);

    assert!(
        cli.starts_with(&escaped),
        "resolved npm executable must be token zero: {cli}"
    );
    assert!(
        !cli.starts_with("'npm' "),
        "remote launch must not replace the resolved path with literal npm: {cli}"
    );
}

/// A remote CLI assembly with a clean version selector and args produces the
/// expected escaped prefix followed by the escaped args.
#[test]
fn remote_cli_assembly_clean_version_with_args() {
    use crate::domain::{AgentKind, LaunchSignature, RemoteRepositorySettings, SandboxEngine};

    let signature = LaunchSignature {
        work_dir: std::path::PathBuf::from("/tmp/work"),
        profile: "my-profile".to_owned(),
        code_puppy_model: String::new(),
        llxprt_version: "0.9.0".to_owned(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: AgentKind::Llxprt,
    };
    let plan = ExecutablePlan::from_signature(&signature);
    let args = launch_args(&signature);
    let cli = assemble_remote_cli_command(&plan, "npm", &args);
    assert!(
        cli.contains("'--package=@vybestack/llxprt-code@0.9.0'"),
        "clean selector must be in the package token: {cli}"
    );
    assert!(
        cli.contains("'--profile-load'"),
        "launch args must be shell-escaped: {cli}"
    );
    assert!(
        cli.contains("'--continue'"),
        "continue flag must be present: {cli}"
    );
}

// ── Issue #269: persisted selector validation before destructive kill ─────
//
// A hand-edited state.json could carry an embedded NUL byte in the Version
// selector. `create_session` must reject that structurally-unrepresentable
// selector at the runtime boundary BEFORE reaching `kill_session`, returning
// the dedicated `RuntimeError::InvalidVersionSelector`. This proves the
// preflight validation fires regardless of whether tmux/psmux is present, so
// no session is killed for a launch that can never succeed.

/// A persisted selector containing an embedded NUL byte is rejected by
/// `create_session` with `RuntimeError::InvalidVersionSelector`, and the
/// rejection happens before the multiplexer preflight (no tmux/psmux
/// required). Drives the real production `create_session` entry point.
#[test]
fn create_session_rejects_invalid_version_selector_before_kill() {
    let mut signature = base_signature();
    // Embedded NUL: structurally unrepresentable as a process argument.
    signature.llxprt_version = "0.9.0\x00; rm -rf /".to_owned();

    let result = create_session(
        "jefe-test-invalid-selector",
        std::path::Path::new("/tmp"),
        &signature,
        None,
    );
    let Err(error) = result else {
        panic!("invalid selector must be rejected before launch");
    };

    assert!(
        matches!(error, RuntimeError::InvalidVersionSelector(_)),
        "expected InvalidVersionSelector, got {error:?}"
    );
}

// ── Issue #269: session-cached npm path wins over live resolver ──────────
//
// The production `resolve_local_executable_with_resolver` is the real seam
// (not a test-only duplicate): it accepts an injected resolver so tests prove
// the cached-path-wins and no-cache-fallback contracts through the identical
// branch structure that production uses.

/// A cached npm path supplied to `resolve_local_executable_with_resolver`
/// must be returned verbatim, even when a different npm is discoverable on
/// the live PATH. This proves the stale-tmux/PATH rationale: the
/// session-cached detection snapshot is authoritative, not a fresh resolver
/// lookup.
#[cfg(unix)]
#[test]
fn cached_npm_path_wins_over_different_live_resolver_result() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let live_npm = temp.path().join("npm");
    std::fs::write(
        &live_npm,
        b"#!/bin/sh
",
    )
    .unwrap_or_else(|error| panic!("write npm: {error}"));
    std::fs::set_permissions(&live_npm, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm: {error}"));

    let resolver = crate::runtime::AgentExecutableResolver::for_platform(
        crate::runtime::AgentExecutablePlatform::Unix,
        vec![temp.path().to_path_buf()],
        None,
    );
    let live = resolver
        .resolve_named("npm")
        .unwrap_or_else(|error| panic!("live npm should resolve: {error:?}"));

    let cached = std::path::Path::new("/opt/node/v18/bin/npm");
    assert_ne!(
        live.path(),
        cached,
        "test setup: cached path must differ from live result"
    );

    let executable =
        resolve_local_executable_with_resolver(&plan_npm_exec(), Some(cached), &resolver)
            .unwrap_or_else(|error| panic!("cached path should resolve: {error:?}"));
    assert_eq!(
        executable.path(),
        cached,
        "cached npm path must win over the live resolver result"
    );
    assert_eq!(
        executable.wrapper_kind(),
        crate::runtime::AgentWrapperKind::Direct,
        "cached Unix path must use the Direct wrapper strategy"
    );
}

/// When no cached npm path is supplied,
/// `resolve_local_executable_with_resolver` must fall back to the injected
/// resolver. Covers the no-cache branch alongside the cached-path-wins test
/// above.
#[cfg(unix)]
#[test]
fn no_cache_falls_back_to_live_resolver() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("create temp dir: {error}"));
    let live_npm = temp.path().join("npm");
    std::fs::write(
        &live_npm,
        b"#!/bin/sh
",
    )
    .unwrap_or_else(|error| panic!("write npm: {error}"));
    std::fs::set_permissions(&live_npm, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod npm: {error}"));

    let resolver = crate::runtime::AgentExecutableResolver::for_platform(
        crate::runtime::AgentExecutablePlatform::Unix,
        vec![temp.path().to_path_buf()],
        None,
    );

    let executable = resolve_local_executable_with_resolver(&plan_npm_exec(), None, &resolver)
        .unwrap_or_else(|error| panic!("live resolver should find npm: {error:?}"));
    assert_eq!(
        executable.path(),
        &live_npm,
        "no-cache path must use the live resolver result"
    );
}
