//! Behavioral tests for the jefe-managed LLxprt install cache (issue #425).

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use super::*;
use crate::domain::LlxprtNpmPackageSelector;
use crate::runtime::agent_executable::{AgentExecutablePlatform, AgentExecutableResolver};

fn selector(value: &str) -> LlxprtNpmPackageSelector {
    LlxprtNpmPackageSelector::normalize(value)
        .unwrap_or_else(|| panic!("selector fixture must be nonblank"))
}

/// Write a fixture binary with the execute bit on Unix so the cache-hit check
/// (which mirrors `AgentExecutableResolver::resolve_unix`) recognizes it. The
/// file name already includes the Windows `.exe` extension when needed.
fn write_fixture_bin(bin_dir: &Path, bin_name: &str) {
    let path = bin_dir.join(bin_name);
    fs::write(&path, "fixture").unwrap_or_else(|error| panic!("write bin: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod bin: {error}"));
    }
    let _ = path;
}

/// The fixture binary name including the platform extension so the cache-hit
/// check (PATHEXT-aware on Windows) recognizes it.
fn fixture_bin_name() -> String {
    if cfg!(windows) {
        format!("{}.exe", llxprt_bin_name())
    } else {
        llxprt_bin_name().to_owned()
    }
}

/// A test-local cache root isolating each test from the real platform cache.
///
/// Returns the path plus the `TempDir` guard that owns automatic cleanup.
/// Dropping the guard removes the temp directory, so tests never leak cache
/// state into the real platform cache or accumulate unbounded temp dirs.
fn test_cache_root() -> (PathBuf, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    (temp.path().to_path_buf(), temp)
}

fn empty_resolver() -> AgentExecutableResolver {
    AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        Vec::new(),
        std::env::var_os("PATHEXT"),
    )
}

fn llxprt_bin_name() -> &'static str {
    let definitions = crate::domain::agent_definition::AgentDefinition::shipped();
    let definition = definitions
        .get(3)
        .unwrap_or_else(|| panic!("shipped LLxprt definition"));
    let binary = definition
        .candidates
        .iter()
        .find_map(|candidate| candidate.kind.path_name())
        .unwrap_or_else(|| panic!("LLxprt definition has a PATH candidate"));
    Box::leak(binary.to_owned().into_boxed_str())
}

fn stage_cache_hit(cache: &Path, sel: &LlxprtNpmPackageSelector) -> PathBuf {
    let install_dir = install_dir_in(cache, sel);
    let bin_dir = bin_dir_in(cache, sel);
    fs::create_dir_all(&install_dir).unwrap_or_else(|error| panic!("mkdir install: {error}"));
    fs::write(install_dir.join(INSTALL_MARKER), marker_contents(sel))
        .unwrap_or_else(|error| panic!("write marker: {error}"));
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    // On Windows the executable carries an `.exe` extension; `AgentExecutableResolver`
    // checks PATHEXT when resolving from the managed bin dir.
    write_fixture_bin(&bin_dir, &fixture_bin_name());
    bin_dir
}

fn expect_ok<T, E: std::fmt::Debug>(result: Result<T, E>, ctx: &str) -> T {
    result.unwrap_or_else(|error| panic!("{ctx}: {error:?}"))
}

#[test]
fn cache_root_under_jefe_versions_subdir() {
    let root = cache_root();
    let components = root.components().collect::<Vec<_>>();
    assert!(
        components.iter().any(|c| c.as_os_str() == "jefe"),
        "cache root must live under a jefe dir: {}",
        root.display()
    );
    assert_eq!(
        components.last().map(|c| c.as_os_str()),
        Some(std::ffi::OsStr::new(VERSIONS_SUBDIR)),
        "cache root must end in the versions subdir: {}",
        root.display()
    );
}

#[test]
fn resolve_cache_root_honors_absolute_env_override() {
    // Use a platform-appropriate absolute path so the override is recognized
    // on both Unix (`/tmp/...`) and Windows (`C:\tmp\...`). On Windows,
    // `/tmp/...` lacks a drive letter and `Path::is_absolute()` returns
    // false, so the override would be ignored.
    let dir = if cfg!(windows) {
        PathBuf::from(r"C:\tmp\jefe-test-cache")
    } else {
        PathBuf::from("/tmp/jefe-test-cache")
    };
    let env_value = std::ffi::OsString::from(dir.clone());
    assert_eq!(
        resolve_cache_root(Some(env_value)),
        dir.join(VERSIONS_SUBDIR)
    );
}

#[test]
fn resolve_cache_root_ignores_relative_env_override() {
    let resolved = resolve_cache_root(Some(std::ffi::OsString::from("relative/cache")));
    assert!(resolved.is_absolute(), "relative override ignored");
}

#[test]
fn resolve_cache_root_ignores_empty_env_override() {
    let resolved_empty = resolve_cache_root(Some(std::ffi::OsString::new()));
    let resolved_none = resolve_cache_root(None);
    assert_eq!(resolved_empty, resolved_none);
}

#[test]
fn install_dir_is_cache_root_plus_version_dir_name() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    assert_eq!(
        install_dir_in(&cache, &sel),
        cache.join(sel.version_dir_name())
    );
}

#[test]
fn bin_dir_is_install_dir_node_modules_bin() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    assert_eq!(
        bin_dir_in(&cache, &sel),
        install_dir_in(&cache, &sel)
            .join("node_modules")
            .join(".bin")
    );
}

#[test]
fn package_json_pins_exact_selector_not_caret_range() {
    let sel = selector("0.9.0");
    let contents = package_json_contents(&sel);
    assert!(
        contents.contains("\"@vybestack/llxprt-code\": \"0.9.0\""),
        "package.json must pin the exact version, not a caret range: {contents}"
    );
    assert!(!contents.contains("^0.9.0"));
    assert!(contents.contains("\"private\": true"));

    let latest = selector("latest");
    let latest_contents = package_json_contents(&latest);
    assert!(
        latest_contents.contains("\"@vybestack/llxprt-code\": \"latest\""),
        "latest sentinel must pin the dist-tag, not a caret: {latest_contents}"
    );

    let nightly = selector("latest nightly");
    let nightly_contents = package_json_contents(&nightly);
    assert!(
        nightly_contents.contains("\"@vybestack/llxprt-code\": \"nightly\""),
        "latest nightly sentinel must pin the nightly dist-tag: {nightly_contents}"
    );
}

#[test]
fn marker_records_effective_install_spec_value() {
    assert_eq!(marker_contents(&selector("0.9.0")), "0.9.0");
    assert_eq!(marker_contents(&selector("latest")), "latest");
    assert_eq!(marker_contents(&selector("LATEST")), "latest");
    assert_eq!(marker_contents(&selector("latest nightly")), "nightly");
}

#[test]
fn is_cache_hit_requires_marker_match_and_binary_present() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    let install_dir = install_dir_in(&cache, &sel);
    let bin_dir = bin_dir_in(&cache, &sel);

    assert!(!is_cache_hit(&install_dir, &bin_dir, &sel));

    fs::create_dir_all(&install_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    fs::write(install_dir.join(INSTALL_MARKER), marker_contents(&sel))
        .unwrap_or_else(|error| panic!("write marker: {error}"));
    assert!(!is_cache_hit(&install_dir, &bin_dir, &sel));

    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    write_fixture_bin(&bin_dir, &fixture_bin_name());
    assert!(is_cache_hit(&install_dir, &bin_dir, &sel));
}

#[test]
fn is_cache_hit_rejects_stale_marker_for_different_selector() {
    let (cache, _guard) = test_cache_root();
    let old_sel = selector("0.9.0");
    let new_sel = selector("0.10.0");
    let install_dir = install_dir_in(&cache, &new_sel);
    let bin_dir = bin_dir_in(&cache, &new_sel);
    fs::create_dir_all(&install_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    fs::write(install_dir.join(INSTALL_MARKER), marker_contents(&old_sel))
        .unwrap_or_else(|error| panic!("write marker: {error}"));
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    write_fixture_bin(&bin_dir, &fixture_bin_name());
    assert!(!is_cache_hit(&install_dir, &bin_dir, &new_sel));
}

#[test]
fn ensure_installed_cache_hit_does_not_reinstall() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    let bin_dir = stage_cache_hit(&cache, &sel);
    let result = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(5));
    assert_eq!(expect_ok(result, "cache hit should succeed"), bin_dir);
}

#[test]
fn ensure_installed_npm_missing_returns_typed_error() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    let result = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(5));
    let Err(LlxprtInstallError::NpmMissing { selector: sel_name }) = result else {
        panic!("expected NpmMissing, got {result:?}");
    };
    assert_eq!(sel_name, sel.as_str());
}

#[cfg(unix)]
#[test]
fn ensure_installed_writes_marker_and_returns_bin_dir_on_success() {
    // The install happy path runs a stubbed `npm`. On Unix a shell-script
    // stub suffices; on Windows, npm resolution requires the canonical
    // node.exe + npm-cli.js layout, which a test stub cannot provide. The
    // Windows install path is otherwise covered by the cache-hit and
    // error-classification tests.
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0");
    let bin_dir = bin_dir_in(&cache, &sel);
    let staging = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm_path = staging.path().join("npm");
    let bin_target = bin_dir.join(llxprt_bin_name());
    let script = format!(
        "#!/bin/sh\nmkdir -p '{}'\ntouch '{}'\nexit 0\n",
        bin_dir.display(),
        bin_target.display()
    );
    fs::write(&npm_path, script).unwrap_or_else(|error| panic!("write npm: {error}"));
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&npm_path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod npm: {error}"));
    }
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![staging.path().to_path_buf()],
        None,
    );
    let result = ensure_installed_under(&cache, &sel, &resolver, Duration::from_secs(30));
    assert_eq!(expect_ok(result, "install should succeed"), bin_dir);
    let marker = install_dir_in(&cache, &sel).join(INSTALL_MARKER);
    assert_eq!(
        fs::read_to_string(&marker).unwrap_or_else(|error| panic!("read marker: {error}")),
        marker_contents(&sel)
    );
}

#[test]
fn error_display_is_actionable() {
    let errors = [
        LlxprtInstallError::NpmMissing {
            selector: "0.9.0".to_owned(),
        },
        LlxprtInstallError::InstallDir {
            selector: "0.9.0".to_owned(),
            diagnostic: "permission denied".to_owned(),
        },
        LlxprtInstallError::InstallFailed {
            selector: "0.9.0".to_owned(),
            diagnostic: "E404 not found".to_owned(),
        },
    ];
    for error in errors {
        let message = error.to_string();
        assert!(message.contains("0.9.0"));
    }
}

#[test]
fn local_managed_bin_dir_returns_bin_dir_for_cache_hit() {
    // Drive the cache-root-injected core of `local_managed_bin_dir` against a
    // private temp cache root: stage a cache hit, then assert the bin dir is
    // returned without invoking npm. No real platform cache or env mutation.
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.9.0-issue425-fixture");
    let bin_dir = stage_cache_hit(&cache, &sel);
    let result = local_managed_bin_dir_under(&cache, &sel);
    assert_eq!(expect_ok(result, "cache hit should succeed"), bin_dir);
}

#[test]
fn ensure_installed_does_not_overwrite_existing_install_on_repeat_call() {
    let (cache, _guard) = test_cache_root();
    let sel = selector("0.10.0");
    let bin_dir = stage_cache_hit(&cache, &sel);
    let first = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(1));
    let second = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(1));
    assert_eq!(expect_ok(first, "first hit"), bin_dir);
    assert_eq!(expect_ok(second, "second hit"), bin_dir);
}

// ── Issue #432 AC9: install diagnostics name the phase, exit code, and timeout ─
//
// The install path must distinguish a nonzero npm exit from a timeout so the
// user can tell whether resolution/install ran and failed vs. never completed.
// `run_npm_install` already labels nonzero exits as
// `npm install exited with status <code>` and surfaces bounded npm stderr, and
// a timeout surfaces as `jefe llxprt install: timed out after Ns` (the capture
// context). These tests lock that contract so a future change cannot collapse
// the phases into a generic error.

#[cfg(unix)]
#[test]
fn install_failed_diagnostic_names_nonzero_exit_status_and_phase() {
    // Stage a stub `npm` that exits 42 with a stderr line, then drive
    // ensure_installed_under and assert the diagnostic identifies the install
    // phase, the exit code, and the (bounded) npm stderr.
    let (cache, _cache_guard) = test_cache_root();
    let staging = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm_path = staging.path().join("npm");
    let script = "#!/bin/sh\necho 'E404 no such package' >&2\nexit 42\n";
    fs::write(&npm_path, script).unwrap_or_else(|error| panic!("write npm stub: {error}"));
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&npm_path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod npm stub: {error}"));
    }
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![staging.path().to_path_buf()],
        None,
    );
    let sel = selector("0.9.0");
    let result = ensure_installed_under(&cache, &sel, &resolver, Duration::from_secs(30));
    let Err(LlxprtInstallError::InstallFailed {
        selector: sel_name,
        diagnostic,
    }) = result
    else {
        panic!("expected InstallFailed, got {result:?}");
    };
    assert_eq!(sel_name, sel.as_str());
    assert!(
        diagnostic.contains("npm install exited with status 42"),
        "diagnostic must name the install phase + exit code: {diagnostic}"
    );
    assert!(
        diagnostic.contains("E404 no such package"),
        "diagnostic must include bounded npm stderr: {diagnostic}"
    );
}

#[cfg(unix)]
#[test]
fn install_failed_diagnostic_names_timeout_phase_distinctly() {
    // Stage a stub `npm` that sleeps past the timeout, then assert the
    // diagnostic identifies the install phase as a timeout (not a generic
    // capture error and not a nonzero exit).
    let (cache, _cache_guard) = test_cache_root();
    let staging = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm_path = staging.path().join("npm");
    let script = "#!/bin/sh\nsleep 30\nexit 0\n";
    fs::write(&npm_path, script).unwrap_or_else(|error| panic!("write npm stub: {error}"));
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&npm_path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod npm stub: {error}"));
    }
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Unix,
        vec![staging.path().to_path_buf()],
        None,
    );
    let sel = selector("0.9.0");
    // Sub-second timeout so the test stays fast; the stub sleeps for 30s.
    let result = ensure_installed_under(&cache, &sel, &resolver, Duration::from_millis(500));
    let Err(LlxprtInstallError::InstallFailed { diagnostic, .. }) = result else {
        panic!("expected InstallFailed on timeout, got {result:?}");
    };
    assert!(
        diagnostic.contains("jefe llxprt install"),
        "timeout diagnostic must name the install phase: {diagnostic}"
    );
    assert!(
        diagnostic.contains("timed out"),
        "timeout diagnostic must say 'timed out': {diagnostic}"
    );
    assert!(
        !diagnostic.contains("exited with status"),
        "timeout must not be reported as a nonzero exit: {diagnostic}"
    );
}

// ── Issue #432: Windows canonical-layout install behavioral coverage ───────
//
// #425's managed-install fix landed in #431 about 11 hours after #432 was
// filed and resolves the original "Version=latest reports unavailable" failure
// on the reporting Windows machine (verified by running exactly what
// `ensure_installed` constructs — `node.exe npm-cli.js install` from a neutral
// jefe-owned dir against a hand-written package.json pinning `latest`).
//
// What #431 did NOT add is the Windows-specific behavioral evidence called out
// by #432 AC4/AC5: the install happy-path test was `#[cfg(unix)]`-only, with
// an explicit note that "on Windows, npm resolution requires the canonical
// node.exe + npm-cli.js layout, which a test stub cannot provide." The tests
// below close that gap by staging the canonical layout (a hardlinked real
// `node.exe` plus a JavaScript `npm-cli.js` stub that the real node runs) and
// driving the real `ensure_installed_under` subprocess, so the Jefe-specific
// execution context — the structured `node.exe <npm-cli.js> install` argv, the
// jefe-owned install cwd, and the bounded install phase — is exercised
// end-to-end rather than only asserted as constructed argv.
//
// The hardlink keeps the test self-contained (no ~100 MB copy) and the
// `JEFE_REQUIRE_NODE_INSTALL` gate mirrors the existing `JEFE_REQUIRE_PSMUX`
// convention: the test skips when system node is unavailable on the runner,
// but is required (panics) when CI sets the env var.

#[cfg(windows)]
mod windows_canonical_install {
    use super::*;
    use crate::runtime::agent_executable::AgentExecutableTarget;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Locate the real system `node.exe` by scanning PATH, mirroring how an
    /// end user's environment provides it. Returns `None` when node is not on
    /// PATH so the test can skip on runners without Node (unless explicitly
    /// required via `JEFE_REQUIRE_NODE_INSTALL=1`).
    fn locate_system_node() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("node.exe");
            if candidate.is_file()
                && Command::new(&candidate)
                    .arg("--version")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success())
            {
                return Some(candidate);
            }
        }
        None
    }

    /// Stage the canonical Windows npm layout in `staging`:
    /// - `node.exe` — a hardlink to the real system node so the resolver's
    ///   `canonicalize(<dir>/node.exe)` succeeds AND the binary actually runs.
    /// - `npm.cmd` — a minimal wrapper file so `AgentExecutableResolver`
    ///   selects this directory and computes the script launch plan.
    /// - `node_modules/npm/bin/npm-cli.js` — a JavaScript stub run by the real
    ///   node that performs the install side effect (create `node_modules/.bin`
    ///   and write `llxprt.cmd`), mirroring what a real `npm install` leaves
    ///   behind for a package that ships a `llxprt` bin entry.
    ///
    /// Returns `Some(resolver)` scoped to the staging dir, or `None` when node
    /// is unavailable and not required (skip), or panics when required but
    /// unavailable.
    fn stage_canonical_npm(staging: &Path) -> Option<AgentExecutableResolver> {
        let node = locate_system_node()?;
        let staged_node = staging.join("node.exe");
        // A hardlink avoids a multi-MB copy and works whenever staging lives
        // on the same volume as the system node (the common case for both
        // local dev and Windows CI). If it fails, fall through to the skip
        // path unless the test is explicitly required.
        if std::fs::hard_link(&node, &staged_node).is_err() {
            return require_node_install_or_skip();
        }
        // The resolver only inspects npm.cmd's existence + the canonical
        // node/npm-cli.js neighbors; it does not execute npm.cmd.
        std::fs::write(staging.join("npm.cmd"), "@echo off\r\nrem stub\r\n")
            .unwrap_or_else(|error| panic!("write npm.cmd stub: {error}"));
        let cli_dir = staging.join("node_modules").join("npm").join("bin");
        std::fs::create_dir_all(&cli_dir).unwrap_or_else(|error| panic!("mkdir cli: {error}"));
        // The stub runs with cwd = jefe install dir (set by run_npm_install)
        // and argv = ["install"]. It creates node_modules/.bin/llxprt.cmd in
        // the cwd so the cache-hit check recognizes the install, exactly as a
        // real npm install of @vybestack/llxprt-code would.
        let stub = concat!(
            "const fs=require('fs');const path=require('path');",
            "const binDir=path.join(process.cwd(),'node_modules','.bin');",
            "fs.mkdirSync(binDir,{recursive:true});",
            "fs.writeFileSync(path.join(binDir,'llxprt.cmd'),'@echo off\\r\\n');",
            "process.exit(0);"
        );
        std::fs::write(cli_dir.join("npm-cli.js"), stub)
            .unwrap_or_else(|error| panic!("write npm-cli.js stub: {error}"));
        let resolver = AgentExecutableResolver::for_platform(
            AgentExecutablePlatform::Windows,
            vec![staging.to_path_buf()],
            Some(OsString::from(".CMD")),
        );
        Some(resolver)
    }

    fn require_node_install_or_skip<T>() -> Option<T> {
        // Match the established `JEFE_REQUIRE_PSMUX` truthiness convention
        // (see tests/psmux_smoke.rs, multiplexer_tests.rs): only the literal
        // value "1" forces the test; "0"/empty/absent all skip. This avoids
        // surprising CI operators who set the var to "0" expecting a skip
        // (OCR finding F4 on PR #483).
        let required = std::env::var("JEFE_REQUIRE_NODE_INSTALL").as_deref() == Ok("1");
        assert!(
            !required,
            "JEFE_REQUIRE_NODE_INSTALL=1 is set but the canonical Windows npm layout could not \
             be staged (system node.exe unavailable or cross-volume hardlink failed)"
        );
        None
    }

    fn assert_canonical_layout_then_run(sel_value: &str) {
        let staging = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let Some(resolver) = stage_canonical_npm(staging.path()) else {
            return;
        };
        let (cache, _cache_guard) = test_cache_root();
        let sel = selector(sel_value);
        let expected_bin_dir = bin_dir_in(&cache, &sel);
        // Sanity: the resolver recognizes the canonical layout as npm before
        // we rely on it for the install subprocess. This pins #258's
        // structured-argument contract (no cmd.exe mediation) at the point the
        // install path consumes it.
        let npm_executable = resolver
            .resolve_target(AgentExecutableTarget::Npm)
            .unwrap_or_else(|error| panic!("resolve canonical npm layout: {error}"));
        assert!(
            npm_executable.script_launch_plan().is_some(),
            "canonical Windows npm layout must resolve to a node.exe + npm-cli.js script plan"
        );

        let result = ensure_installed_under(&cache, &sel, &resolver, Duration::from_secs(60));
        let returned_bin_dir =
            expect_ok(result, "ensure_installed_under should succeed on Windows");
        assert_eq!(returned_bin_dir, expected_bin_dir);

        // AC5/AC6: the install actually ran from the jefe-owned install cwd
        // (no repo worktree node_modules/.npmrc dependency) and produced the
        // cached llxprt bin plus the selector-matching marker.
        let llxprt_cmd = expected_bin_dir.join(format!(
            "{}.cmd",
            AgentExecutableTarget::Agent("llxprt").binary_name()
        ));
        assert!(
            llxprt_cmd.is_file(),
            "managed install must produce llxprt.cmd in node_modules/.bin: {}",
            llxprt_cmd.display()
        );
        let marker_path = install_dir_in(&cache, &sel).join(INSTALL_MARKER);
        assert_eq!(
            std::fs::read_to_string(&marker_path)
                .unwrap_or_else(|error| panic!("read marker: {error}")),
            marker_contents(&sel),
            "marker must record the effective selector"
        );
    }

    #[test]
    fn windows_canonical_install_succeeds_for_latest_sentinel() {
        // AC1: Version=latest installs the dist-tag and stages the cached bin.
        assert_canonical_layout_then_run("latest");
    }

    #[test]
    fn windows_canonical_install_succeeds_for_latest_nightly_sentinel() {
        // AC2: latest nightly maps to the nightly dist-tag.
        assert_canonical_layout_then_run("latest nightly");
    }

    #[test]
    fn windows_canonical_install_succeeds_for_pinned_version() {
        // AC2: an explicit pinned version also installs.
        assert_canonical_layout_then_run("0.9.0");
    }
}
