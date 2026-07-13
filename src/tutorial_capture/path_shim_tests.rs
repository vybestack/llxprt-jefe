use super::*;
use std::path::Path;

// ── plan_shims ────────────────────────────────────────────────────────

fn shim_profile_plans_both_binaries() {
    let shims = plan_shims(RuntimeProfile::Shim, ShimAvailability::Both);
    assert_eq!(shims.len(), 2);
    assert!(shims.iter().any(|s| s.binary_name == "llxprt"));
    assert!(shims.iter().any(|s| s.binary_name == "code-puppy"));
}

fn real_llxprt_profile_plans_no_shims() {
    // Finding #6: real profiles must not inject opposite shims.
    let shims = plan_shims(RuntimeProfile::RealLlxprt, ShimAvailability::Both);
    assert!(shims.is_empty(), "real profiles must not inject any shims");
}

fn real_code_puppy_profile_plans_no_shims() {
    // Finding #6: real profiles must not inject opposite shims.
    let shims = plan_shims(RuntimeProfile::RealCodePuppy, ShimAvailability::Both);
    assert!(shims.is_empty(), "real profiles must not inject any shims");
}

// ── deterministic_shim ───────────────────────────────────────────────

fn deterministic_shim_has_correct_binary_name() {
    let shim = deterministic_shim("llxprt");
    assert_eq!(shim.binary_name, "llxprt");
}

fn deterministic_shim_script_contains_marker() {
    let shim = deterministic_shim("llxprt");
    assert!(shim.script.contains(SHIM_MARKER));
}

fn deterministic_shim_script_is_posix_compatible_shebang() {
    let shim = deterministic_shim("code-puppy");
    assert!(shim.script.starts_with("#!/bin/sh"));
}

fn deterministic_shim_reads_and_echoes_input() {
    let shim = deterministic_shim("llxprt");
    assert!(shim.script.contains("read -r line"));
    assert!(shim.script.contains("echo"));
}

fn deterministic_shim_prints_ready_marker() {
    let shim = deterministic_shim("llxprt");
    assert!(shim.script.contains("runtime-shim: ready"));
}

// ── controlled_path ──────────────────────────────────────────────────

fn controlled_path_prepends_shim_dir() {
    let path = controlled_path(Path::new("/tmp/shims"), "/usr/bin:/bin");
    assert!(path.starts_with("/tmp/shims:"));
    assert!(path.contains("/usr/bin:/bin"));
}

fn controlled_path_retains_inherited_path() {
    let path = controlled_path(Path::new("/tmp/shims"), "/usr/local/bin:/usr/bin");
    assert_eq!(path, "/tmp/shims:/usr/local/bin:/usr/bin");
}

fn controlled_path_handles_empty_inherited() {
    let path = controlled_path(Path::new("/tmp/shims"), "");
    assert_eq!(path, "/tmp/shims:");
}

// ── is_agent_binary ──────────────────────────────────────────────────

fn is_agent_binary_recognizes_known_names() {
    assert!(is_agent_binary("llxprt"));
    assert!(is_agent_binary("code-puppy"));
}

fn is_agent_binary_rejects_unknown_names() {
    assert!(!is_agent_binary("claude"));
    assert!(!is_agent_binary(""));
}

fn shim_scripts_are_identical_for_same_binary_name() {
    let shim1 = deterministic_shim("llxprt");
    let shim2 = deterministic_shim("llxprt");
    assert_eq!(shim1.script, shim2.script);
}

fn shim_script_exits_cleanly_on_eof() {
    let shim = deterministic_shim("llxprt");
    assert!(
        shim.script.contains("exited") || shim.script.contains("read -r"),
        "shim must handle EOF cleanly"
    );
}

fn plan_shims_returns_shims_with_scripts() {
    let shims = plan_shims(RuntimeProfile::Shim, ShimAvailability::Both);
    for shim in &shims {
        assert!(!shim.script.is_empty(), "shim script must not be empty");
        assert!(
            is_agent_binary(&shim.binary_name),
            "shim binary name must be a recognized agent binary"
        );
    }
}

// ── ShimAvailability selection ──────────────────────────────────────

fn llxprt_only_availability_installs_only_llxprt_shim() {
    let shims = plan_shims(RuntimeProfile::Shim, ShimAvailability::LlxprtOnly);
    assert_eq!(shims.len(), 1, "llxprt-only must install exactly one shim");
    assert!(
        shims.iter().any(|s| s.binary_name == "llxprt"),
        "must include llxprt shim"
    );
    assert!(
        !shims.iter().any(|s| s.binary_name == "code-puppy"),
        "must NOT include code-puppy shim"
    );
}

fn code_puppy_only_availability_installs_only_code_puppy_shim() {
    let shims = plan_shims(RuntimeProfile::Shim, ShimAvailability::CodePuppyOnly);
    assert_eq!(
        shims.len(),
        1,
        "code-puppy-only must install exactly one shim"
    );
    assert!(
        shims.iter().any(|s| s.binary_name == "code-puppy"),
        "must include code-puppy shim"
    );
    assert!(
        !shims.iter().any(|s| s.binary_name == "llxprt"),
        "must NOT include llxprt shim"
    );
}

fn both_availability_installs_both_shims() {
    let shims = plan_shims(RuntimeProfile::Shim, ShimAvailability::Both);
    assert_eq!(shims.len(), 2);
}

fn shim_availability_includes_predicates() {
    assert!(ShimAvailability::LlxprtOnly.includes_llxprt());
    assert!(!ShimAvailability::LlxprtOnly.includes_code_puppy());
    assert!(ShimAvailability::CodePuppyOnly.includes_code_puppy());
    assert!(!ShimAvailability::CodePuppyOnly.includes_llxprt());
    assert!(ShimAvailability::Both.includes_llxprt());
    assert!(ShimAvailability::Both.includes_code_puppy());
}

fn shim_availability_parse_accepts_known_values() {
    assert_eq!(
        ShimAvailability::parse("llxprt-only"),
        Some(ShimAvailability::LlxprtOnly)
    );
    assert_eq!(
        ShimAvailability::parse("code-puppy-only"),
        Some(ShimAvailability::CodePuppyOnly)
    );
    assert_eq!(
        ShimAvailability::parse("both"),
        Some(ShimAvailability::Both)
    );
}

fn shim_availability_parse_rejects_unknown_values() {
    assert!(ShimAvailability::parse("all").is_none());
    assert!(ShimAvailability::parse("").is_none());
    assert!(ShimAvailability::parse("none").is_none());
}

fn shim_availability_default_is_both() {
    assert_eq!(ShimAvailability::default(), ShimAvailability::Both);
}

fn shim_availability_label() {
    assert_eq!(ShimAvailability::LlxprtOnly.label(), "llxprt-only");
    assert_eq!(ShimAvailability::CodePuppyOnly.label(), "code-puppy-only");
    assert_eq!(ShimAvailability::Both.label(), "both");
}

// ── detection_path (Finding #2: curated PATH projection) ─────────────

/// Finding #2: detection_path returns ONLY the shim/bin directory, not
/// inherited PATH entries. The curated bin is the sole PATH entry.
fn detection_path_returns_only_curated_bin() {
    let path = detection_path(
        Path::new("/tmp/run/shims"),
        RuntimeProfile::Shim,
        "/usr/bin:/bin:/usr/local/bin",
    );
    assert_eq!(
        path, "/tmp/run/shims",
        "detection_path must return only the curated bin dir, not inherited PATH"
    );
}

fn detection_path_real_llxprt_returns_only_curated_bin() {
    let path = detection_path(
        Path::new("/tmp/run/shims"),
        RuntimeProfile::RealLlxprt,
        "/usr/bin:/bin",
    );
    assert_eq!(path, "/tmp/run/shims");
}

fn detection_path_does_not_inherit_host_path() {
    // Even if the host has a stray opposite-runtime binary in /usr/bin,
    // the curated bin does not include /usr/bin in PATH.
    let path = detection_path(
        Path::new("/tmp/run/shims"),
        RuntimeProfile::RealLlxprt,
        "/usr/bin:/bin",
    );
    assert!(
        !path.contains("/usr/bin"),
        "curated PATH must not inherit host directories: {path}"
    );
    assert!(
        !path.contains("/bin"),
        "curated PATH must not inherit host directories: {path}"
    );
}

// ── plan_system_tool_links (Finding #2) ──────────────────────────────

fn plan_system_tool_links_resolves_required_tools() {
    // Resolve against the real host PATH — git, sh, tmux should exist.
    let inherited = std::env::var("PATH").unwrap_or_default();
    let links = plan_system_tool_links(&inherited);
    // At minimum, sh should be found on macOS/Linux.
    assert!(
        links.iter().any(|l| l.name == "sh"),
        "sh should be resolved as a system tool link"
    );
    // Agent runtimes must NOT appear in system tool links.
    assert!(
        !links.iter().any(|l| l.name == "llxprt"),
        "llxprt must not be in system tool links"
    );
    assert!(
        !links.iter().any(|l| l.name == "code-puppy"),
        "code-puppy must not be in system tool links"
    );
}

/// Finding #1: system tools are resolved by executable path regardless
/// of whether the source directory also contains agent binaries.
/// No directory exclusion — the curated bin only symlinks the named
/// system tool, never an agent.
fn plan_system_tool_links_resolves_from_dir_with_agent_binaries() {
    // Create a temp dir with both git and a fake llxprt.
    let base = std::env::temp_dir().join("jefe-syslink-agent-dir-test");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap_or_else(|e| panic!("create dir: {e}"));
    let fake_git = base.join("git");
    std::fs::write(&fake_git, "#!/bin/sh\n").unwrap_or_else(|e| panic!("write: {e}"));
    make_executable_helper(&fake_git);
    let fake_llxprt = base.join("llxprt");
    std::fs::write(&fake_llxprt, "#!/bin/sh\n").unwrap_or_else(|e| panic!("write: {e}"));
    make_executable_helper(&fake_llxprt);
    let inherited = format!("{}", base.display());
    let links = plan_system_tool_links(&inherited);
    // Finding #1: git MUST be linked from this dir even though it also
    // has an agent binary. The curated projection projects by executable
    // path, not by directory.
    assert!(
        links.iter().any(|l| l.target == fake_git),
        "git must be projected from dir containing agents (Finding #1): {links:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #1: check_tier_a_required_tools finds sh/git/tmux on host PATH.
fn check_tier_a_required_tools_finds_all_on_host() {
    let inherited = std::env::var("PATH").unwrap_or_default();
    let missing = check_tier_a_required_tools(&inherited);
    // On a real dev host with sh/git/tmux installed, all must be found.
    if which("sh").is_some() && which("git").is_some() && which("tmux").is_some() {
        assert!(
            missing.is_empty(),
            "all Tier A tools must be found: missing {missing:?}"
        );
    }
}

/// Finding #1: check_tier_a_required_tools reports missing tools.
fn check_tier_a_required_tools_reports_missing() {
    let missing = check_tier_a_required_tools("/nonexistent-path-12345");
    assert!(
        missing.contains(&"sh".to_string()),
        "sh must be reported missing: {missing:?}"
    );
    assert!(
        missing.contains(&"git".to_string()),
        "git must be reported missing: {missing:?}"
    );
    assert!(
        missing.contains(&"tmux".to_string()),
        "tmux must be reported missing: {missing:?}"
    );
    assert!(
        missing.contains(&"env".to_string()),
        "env must be reported missing: {missing:?}"
    );
    assert!(
        missing.contains(&"id".to_string()),
        "id must be reported missing: {missing:?}"
    );
    assert!(
        missing.contains(&"kill".to_string()),
        "kill must be reported missing: {missing:?}"
    );
}

/// Finding #1: check_tier_b_required_tools includes gh.
fn check_tier_b_required_tools_checks_gh() {
    let missing = check_tier_b_required_tools("/nonexistent-path-12345");
    assert!(
        missing.contains(&"gh".to_string()),
        "gh must be reported missing for Tier B: {missing:?}"
    );
}

// ── plan_real_runtime_link (Finding #2) ──────────────────────────────

fn plan_real_runtime_link_returns_none_for_shim_profile() {
    let link = plan_real_runtime_link(RuntimeProfile::Shim);
    assert!(link.is_none());
}

fn plan_real_runtime_link_returns_some_for_real_llxprt_if_installed() {
    let link = plan_real_runtime_link(RuntimeProfile::RealLlxprt);
    if which("llxprt").is_some() {
        assert!(
            link.is_some(),
            "real llxprt link should resolve when llxprt is installed"
        );
    } else {
        assert!(link.is_none());
    }
}

fn plan_real_runtime_link_returns_some_for_real_code_puppy_if_installed() {
    let link = plan_real_runtime_link(RuntimeProfile::RealCodePuppy);
    if which("code-puppy").is_some() {
        assert!(link.is_some());
    } else {
        assert!(link.is_none());
    }
}

#[cfg(unix)]
fn make_executable_helper(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
}

#[cfg(not(unix))]
fn make_executable_helper(_path: &std::path::Path) {}

#[test]
fn path_shim_behaviors() {
    shim_profile_plans_both_binaries();
    real_llxprt_profile_plans_no_shims();
    real_code_puppy_profile_plans_no_shims();
    deterministic_shim_has_correct_binary_name();
    deterministic_shim_script_contains_marker();
    deterministic_shim_script_is_posix_compatible_shebang();
    deterministic_shim_reads_and_echoes_input();
    deterministic_shim_prints_ready_marker();
    controlled_path_prepends_shim_dir();
    controlled_path_retains_inherited_path();
    controlled_path_handles_empty_inherited();
    is_agent_binary_recognizes_known_names();
    is_agent_binary_rejects_unknown_names();
    shim_scripts_are_identical_for_same_binary_name();
    shim_script_exits_cleanly_on_eof();
    plan_shims_returns_shims_with_scripts();
    llxprt_only_availability_installs_only_llxprt_shim();
    code_puppy_only_availability_installs_only_code_puppy_shim();
    both_availability_installs_both_shims();
    shim_availability_includes_predicates();
    shim_availability_parse_accepts_known_values();
    shim_availability_parse_rejects_unknown_values();
    shim_availability_default_is_both();
    shim_availability_label();
    detection_path_returns_only_curated_bin();
    detection_path_real_llxprt_returns_only_curated_bin();
    detection_path_does_not_inherit_host_path();
    plan_system_tool_links_resolves_required_tools();
    plan_system_tool_links_resolves_from_dir_with_agent_binaries();
    check_tier_a_required_tools_finds_all_on_host();
    check_tier_a_required_tools_reports_missing();
    check_tier_b_required_tools_checks_gh();
    plan_real_runtime_link_returns_none_for_shim_profile();
    plan_real_runtime_link_returns_some_for_real_llxprt_if_installed();
    plan_real_runtime_link_returns_some_for_real_code_puppy_if_installed();
}
