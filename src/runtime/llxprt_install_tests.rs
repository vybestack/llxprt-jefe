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

/// A test-local cache root isolating each test from the real platform cache.
fn test_cache_root() -> PathBuf {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    temp.keep()
}

fn empty_resolver() -> AgentExecutableResolver {
    AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        Vec::new(),
        std::env::var_os("PATHEXT"),
    )
}

fn llxprt_bin_name() -> &'static str {
    AgentExecutableTarget::Agent(crate::domain::AgentKind::Llxprt).binary_name()
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
    let bin_name = if cfg!(windows) {
        format!("{llxprt}.exe", llxprt = llxprt_bin_name())
    } else {
        llxprt_bin_name().to_owned()
    };
    fs::write(bin_dir.join(&bin_name), "fixture")
        .unwrap_or_else(|error| panic!("write bin: {error}"));
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
    let cache = test_cache_root();
    let sel = selector("0.9.0");
    assert_eq!(
        install_dir_in(&cache, &sel),
        cache.join(sel.version_dir_name())
    );
}

#[test]
fn bin_dir_is_install_dir_node_modules_bin() {
    let cache = test_cache_root();
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
    let cache = test_cache_root();
    let sel = selector("0.9.0");
    let install_dir = install_dir_in(&cache, &sel);
    let bin_dir = bin_dir_in(&cache, &sel);

    assert!(!is_cache_hit(&install_dir, &bin_dir, &sel));

    fs::create_dir_all(&install_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    fs::write(install_dir.join(INSTALL_MARKER), marker_contents(&sel))
        .unwrap_or_else(|error| panic!("write marker: {error}"));
    assert!(!is_cache_hit(&install_dir, &bin_dir, &sel));

    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    fs::write(bin_dir.join(llxprt_bin_name()), "fixture")
        .unwrap_or_else(|error| panic!("write bin: {error}"));
    assert!(is_cache_hit(&install_dir, &bin_dir, &sel));
}

#[test]
fn is_cache_hit_rejects_stale_marker_for_different_selector() {
    let cache = test_cache_root();
    let old_sel = selector("0.9.0");
    let new_sel = selector("0.10.0");
    let install_dir = install_dir_in(&cache, &new_sel);
    let bin_dir = bin_dir_in(&cache, &new_sel);
    fs::create_dir_all(&install_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    fs::write(install_dir.join(INSTALL_MARKER), marker_contents(&old_sel))
        .unwrap_or_else(|error| panic!("write marker: {error}"));
    fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
    fs::write(bin_dir.join(llxprt_bin_name()), "fixture")
        .unwrap_or_else(|error| panic!("write bin: {error}"));
    assert!(!is_cache_hit(&install_dir, &bin_dir, &new_sel));
}

#[test]
fn ensure_installed_cache_hit_does_not_reinstall() {
    let cache = test_cache_root();
    let sel = selector("0.9.0");
    let bin_dir = stage_cache_hit(&cache, &sel);
    let result = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(5));
    assert_eq!(expect_ok(result, "cache hit should succeed"), bin_dir);
}

#[test]
fn ensure_installed_npm_missing_returns_typed_error() {
    let cache = test_cache_root();
    let sel = selector("0.9.0");
    let result = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(5));
    let Err(LlxprtInstallError::NpmMissing { selector: sel_name }) = result else {
        panic!("expected NpmMissing, got {result:?}");
    };
    assert_eq!(sel_name, sel.as_str());
}

#[test]
fn ensure_installed_writes_marker_and_returns_bin_dir_on_success() {
    let cache = test_cache_root();
    let sel = selector("0.9.0");
    let bin_dir = bin_dir_in(&cache, &sel);
    let staging = tempfile::tempdir().unwrap_or_else(|error| panic!("temp dir: {error}"));
    let npm_path = staging.path().join("npm");
    let bin_target = bin_dir.join(llxprt_bin_name());
    let script = if cfg!(windows) {
        // A .cmd wrapper: create the bin dir and touch the llxprt binary so
        // the install step looks successful without a real npm. The bin
        // target uses an `.exe` extension on Windows.
        format!(
            "@echo off\r\nif not exist \"{dir}\" mkdir \"{dir}\"\r\ntype nul > \"{bin}\"\r\nexit /b 0\r\n",
            dir = bin_dir.display(),
            bin = bin_target.with_extension("exe").display()
        )
    } else {
        format!(
            "#!/bin/sh\nmkdir -p '{}'\ntouch '{}'\nexit 0\n",
            bin_dir.display(),
            bin_target.display()
        )
    };
    fs::write(&npm_path, script).unwrap_or_else(|error| panic!("write npm: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&npm_path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod npm: {error}"));
    }
    // Windows expects a `.cmd` wrapper for the npm launch plan; Unix uses
    // the bare `npm` script. The search dir is the staging dir in both
    // cases.
    let search_dir = staging.path().to_path_buf();
    #[cfg(windows)]
    {
        let cmd = staging.path().join("npm.cmd");
        fs::write(&cmd, "@echo off\r\n%*\r\n")
            .unwrap_or_else(|error| panic!("write npm.cmd: {error}"));
    }
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        vec![search_dir],
        std::env::var_os("PATHEXT"),
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
    // Stage a cache hit under the real cache root so `local_managed_bin_dir`
    // (which uses the real cache) finds the marker and bin without invoking
    // npm. A unique selector avoids colliding with other tests that touch
    // the real cache root.
    let sel = selector("0.9.0-issue425-fixture");
    let bin_dir = stage_cache_hit(&cache_root(), &sel);
    let result = local_managed_bin_dir(&sel);
    assert_eq!(expect_ok(result, "cache hit should succeed"), bin_dir);
    let _ = fs::remove_dir_all(install_dir_for(&sel));
}

#[test]
fn ensure_installed_does_not_overwrite_existing_install_on_repeat_call() {
    let cache = test_cache_root();
    let sel = selector("0.10.0");
    let bin_dir = stage_cache_hit(&cache, &sel);
    let first = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(1));
    let second = ensure_installed_under(&cache, &sel, &empty_resolver(), Duration::from_secs(1));
    assert_eq!(expect_ok(first, "first hit"), bin_dir);
    assert_eq!(expect_ok(second, "second hit"), bin_dir);
}
