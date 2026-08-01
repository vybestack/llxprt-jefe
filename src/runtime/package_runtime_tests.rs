use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    PackageExecutionTarget, finalize_local_invocation, finalize_local_invocation_at,
    package_invocation,
};
use crate::agent_candidate::{AgentCandidateResolver, CandidateResolution, VersionSelector};
use crate::domain::RemoteRepositorySettings;
use crate::domain::agent_definition::{Availability, Operation, Preflight, RemoteTarget};
use crate::runtime::agent_plan::{LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch};
use crate::runtime::agent_remote_plan::{
    RemotePlanOutcome, RemotePlanRequest, plan_remote_launch,
};

#[cfg(unix)]
fn executable(dir: &TempDir, name: &str, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.path().join(name);
    std::fs::write(&path, script).unwrap_or_else(|error| panic!("write fixture: {error}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod fixture: {error}"));
    path
}

#[cfg(unix)]
fn definition(name: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.display_name == name)
        .unwrap_or_else(|| panic!("shipped definition {name}"))
}

#[cfg(unix)]
fn compatible(generation: u64) -> Availability {
    Availability::InstalledCompatible {
        identity: "fixture".to_string(),
        capabilities: Vec::new(),
        generation,
    }
}

#[cfg(unix)]
fn resolve_package(definition: &AgentDefinition, bin: &Path, selector: &str) -> ResolvedCandidate {
    let snapshot =
        PathSnapshot::for_platform(AgentExecutablePlatform::Unix, vec![bin.to_path_buf()], None);
    let selector =
        VersionSelector::normalize(selector).unwrap_or_else(|error| panic!("selector: {error}"));
    let resolution = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector)
        .resolve(definition);
    let CandidateResolution::Resolved(candidate) = resolution else {
        panic!("package candidate must resolve");
    };
    candidate
}

#[cfg(unix)]
fn local_base_plan(
    definition: &AgentDefinition,
    candidate: &ResolvedCandidate,
    generation: u64,
    cache: &Path,
) -> AgentLaunchPlan {
    let invocation = finalize_local_invocation(candidate, cache)
        .unwrap_or_else(|error| panic!("finalize local invocation: {error}"));
    let fingerprint = invocation
        .fingerprint()
        .cloned()
        .unwrap_or_else(|| panic!("local invocation must carry a physical fingerprint"));
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition,
        operation: Operation::Normal,
        target: Target::Local {
            canonical_cwd: PathBuf::from("/repo"),
        },
        executable: invocation.executable().to_path_buf(),
        executable_fingerprint: fingerprint,
        executable_wrapper: invocation.wrapper_kind(),
        argv_prefix: invocation.prefix().to_vec(),
        probe: compatible(generation),
        probe_generation: generation,
        target_generation: 1,
        activation_generation: 1,
        values: &values,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("supported local plan: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn local_uvx_is_a_closed_structural_prefix() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(&bin, "uvx", "#!/bin/sh\nexit 0\n");
    let definition = definition("Code Puppy");
    let candidate = resolve_package(&definition, bin.path(), "0.0.634");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let plan = local_base_plan(&definition, &candidate, 4, cache.path());
    assert_eq!(plan.executable, candidate.executable());
    assert_eq!(
        plan.argv,
        ["--from", "code-puppy==0.0.634", "code-puppy"].map(OsString::from)
    );
}

#[cfg(unix)]
#[test]
fn local_npm_uses_general_managed_exact_install_after_precheck() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(
        &bin,
        "npm",
        "#!/bin/sh\nmkdir -p node_modules/.bin\nprintf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt\nchmod 755 node_modules/.bin/llxprt\n",
    );
    let definition = definition("LLxprt");
    let candidate = resolve_package(&definition, bin.path(), "2.0.0");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let plan = local_base_plan(&definition, &candidate, 8, cache.path());
    assert!(
        plan.argv
            .iter()
            .all(|a| matches!(a.to_string_lossy().as_ref(), "--yolo" | "--prompt-interactive" | "--continue")),
        "managed binary receives only default-flag argv: {:?}",
        plan.argv
    );
    assert!(plan.executable.starts_with(cache.path()));
    assert!(
        plan.executable
            .ends_with(Path::new("node_modules/.bin/llxprt"))
    );
    assert_eq!(
        std::fs::canonicalize(&plan.executable)
            .unwrap_or_else(|error| panic!("managed executable canonicalizes: {error}")),
        plan.executable_fingerprint.canonical_path(),
        "immutable planning carries the finalized managed executable fingerprint"
    );
    let package_json = std::fs::read_to_string(
        plan.executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .unwrap_or_else(|| panic!("managed install root"))
            .join("package.json"),
    )
    .unwrap_or_else(|error| panic!("package json: {error}"));
    assert!(package_json.contains("\"@vybestack/llxprt-code\": \"2.0.0\""));
}

#[cfg(unix)]
fn remote_settings() -> RemoteRepositorySettings {
    RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_string(),
        host: "example.test".to_string(),
        port: Some(22),
        ..RemoteRepositorySettings::default()
    }
}

#[cfg(unix)]
#[test]
fn remote_npm_prefix_flows_through_the_audited_serializer() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(&bin, "npm", "#!/bin/sh\nexit 0\n");
    let definition = definition("LLxprt");
    let candidate = resolve_package(&definition, bin.path(), "latest nightly");
    let values = LaunchFieldValues::new();
    let settings = remote_settings();
    let request = RemotePlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Remote(RemoteTarget {
            user: "dev".to_string(),
            host: "example.test".to_string(),
            port: Some(22),
            run_as_user: String::new(),
            canonical_cwd: PathBuf::from("/srv/repo"),
        }),
        executable: PathBuf::from("npm"),
        executable_fingerprint: candidate.fingerprint().clone(),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: package_invocation(
            &candidate,
            PackageExecutionTarget::Remote,
            Path::new("/unused"),
        )
        .unwrap_or_else(|error| panic!("remote invocation: {error}"))
        .unwrap_or_else(|| panic!("package invocation"))
        .prefix()
        .to_vec(),
        probe: compatible(3),
        probe_generation: 3,
        target_generation: 1,
        activation_generation: 1,
        values: &values,
        preflight: Preflight::default(),
        ssh_settings: &settings,
    };
    let transcript = match plan_remote_launch(&request) {
        RemotePlanOutcome::Transcript(transcript) => *transcript,
        other => panic!("remote package plan: {other:?}"),
    };
    assert_eq!(transcript.plan().executable, PathBuf::from("npm"));
    assert_eq!(
        transcript.agent_argv(),
        [
            "exec",
            "--yes",
            "--package=@vybestack/llxprt-code@nightly",
            "--",
            "llxprt",
            "--yolo",
            "--prompt-interactive",
            "--continue",
        ]
        .map(OsString::from)
    );
    assert!(
        transcript
            .remote_command()
            .contains("exec 'npm' 'exec' '--yes'")
    );
}

#[cfg(unix)]
#[test]
fn package_probe_executes_the_selected_agent_not_the_runner_version() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(
        &bin,
        "npm",
        "#!/bin/sh\nif [ \"$1\" != install ]; then echo runner-version-was-probed; exit 91; fi\nmkdir -p node_modules/.bin\nprintf '#!/bin/sh\\nif [ \"$1\" = --version ]; then echo 1.2.3; else echo --prompt-interactive; fi\\n' > node_modules/.bin/llxprt\nchmod 755 node_modules/.bin/llxprt\n",
    );
    let definition = definition("LLxprt");
    let candidate = resolve_package(&definition, bin.path(), "1.2.3");
    let resolution = CandidateResolution::Resolved(candidate);
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let result = crate::runtime::run_local_agent_probe_with_cache(
        &definition,
        &resolution,
        12,
        cache.path(),
    );
    assert!(
        matches!(
            result.availability(),
            Availability::InstalledCompatible { generation: 12, .. }
        ),
        "selected managed package binary supplies identity and capabilities: {:?}",
        result.availability()
    );
}

#[cfg(unix)]
#[test]
fn structural_uvx_probe_executes_the_selected_agent_invocation() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(
        &bin,
        "uvx",
        "#!/bin/sh\nif [ \"$4\" = --version ]; then echo 0.0.634; else echo --interactive; fi\n",
    );
    let definition = definition("Code Puppy");
    let candidate = resolve_package(&definition, bin.path(), "latest");
    let resolution = CandidateResolution::Resolved(candidate);
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let result = crate::runtime::run_local_agent_probe_with_cache(
        &definition,
        &resolution,
        13,
        cache.path(),
    );
    assert!(
        matches!(
            result.availability(),
            Availability::InstalledCompatible { generation: 13, .. }
        ),
        "selected uvx invocation supplies identity and capabilities: {:?}",
        result.availability()
    );
}

#[cfg(unix)]
#[test]
fn shipped_unsupported_remote_cell_returns_exact_reason_without_package_effects() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    executable(&bin, "uvx", "#!/bin/sh\nexit 99\n");
    let definition = definition("Code Puppy");
    let candidate = resolve_package(&definition, bin.path(), "latest");
    let values = LaunchFieldValues::new();
    let settings = RemoteRepositorySettings {
        enabled: true,
        login_user: "dev".to_string(),
        host: "example.test".to_string(),
        port: Some(22),
        ..RemoteRepositorySettings::default()
    };
    let request = RemotePlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Remote(RemoteTarget {
            user: "dev".to_string(),
            host: "example.test".to_string(),
            port: Some(22),
            run_as_user: String::new(),
            canonical_cwd: PathBuf::from("/srv/repo"),
        }),
        executable: candidate.executable().to_path_buf(),
        executable_fingerprint: crate::agent_candidate_fingerprint::CandidateFingerprint::new(std::path::PathBuf::from("/x"), None, None, 0, 0),
        executable_wrapper: crate::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(1),
        probe_generation: 1,
        target_generation: 1,
        activation_generation: 1,
        values: &values,
        preflight: Preflight::default(),
        ssh_settings: &settings,
    };
    match plan_remote_launch(&request) {
        RemotePlanOutcome::Unsupported { reason } => {
            assert_eq!(reason, "Code Puppy remote/setup is not fixture-verified");
        }
        other => panic!("unsupported remote cell: {other:?}"),
    }
}

// ── Issue #554: volatile-selector cache freshness ─────────────────────────
//
// `latest` / `latest nightly` resolve to moving dist-tags. The managed install
// cache must re-resolve them periodically (TTL) instead of caching the first
// resolution forever, and must drop a stale `package-lock.json` so npm actually
// re-resolves the dist-tag.

#[cfg(unix)]
fn counting_npm_stub(bin: &TempDir, witness: &Path) {
    // Each invocation records one witness line: "present" if a package-lock.json
    // already existed when the stub started, otherwise "absent". Counting lines
    // therefore proves how many times `npm install` ran, and the content proves
    // whether npm ever observed a prior lockfile.
    //
    // The witness path is absolute and outside the managed cache: installs are
    // staged in a fresh directory and promoted by rename (issue #556), so a
    // witness written into the install directory would not survive across
    // installs and could not count them.
    executable(
        bin,
        "npm",
        &format!(
            "#!/bin/sh
set -e
witness={witness}
if [ -f package-lock.json ]; then echo present >> \"$witness\"; else echo absent >> \"$witness\"; fi
mkdir -p node_modules/.bin
printf '#!/bin/sh\nexit 0\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
",
            witness = crate::runtime::commands::shell_escape_single(&witness.to_string_lossy())
        ),
    );
}

/// Install directory owning a managed executable.
///
/// Walks upward from the binary (`…/<hash>/node_modules/.bin/llxprt`) to the
/// nearest ancestor that holds the `.jefe-installed` marker, rather than
/// assuming a fixed directory depth, so a structural change cannot silently
/// target the wrong directory.
#[cfg(unix)]
fn install_dir_of(executable: &Path) -> &Path {
    let mut dir = executable
        .parent()
        .unwrap_or_else(|| panic!("managed executable has a parent dir: {}", executable.display()));
    loop {
        if dir.join(".jefe-installed").exists() {
            return dir;
        }
        dir = dir.parent().unwrap_or_else(|| {
            panic!(
                "no ancestor of {} holds the .jefe-installed marker",
                executable.display()
            )
        });
    }
}

#[cfg(unix)]
fn witness_lines(witness: &Path) -> Vec<String> {
    std::fs::read_to_string(witness)
        .unwrap_or_else(|error| panic!("read install witness: {error}"))
        .lines()
        .map(std::string::ToString::to_string)
        .collect()
}

/// Absolute witness path for [`counting_npm_stub`], kept outside the managed
/// cache so staged-and-promoted installs cannot discard it.
#[cfg(unix)]
fn witness_path(dir: &TempDir) -> PathBuf {
    dir.path().join("install-witness.log")
}

#[cfg(unix)]
fn volatile_npm_candidate(bin: &TempDir, selector: &str) -> ResolvedCandidate {
    let definition = definition("LLxprt");
    resolve_package(&definition, bin.path(), selector)
}

/// A fixed base instant well after the Unix epoch so offsets stay representable.
#[cfg(unix)]
fn base_now() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)
}

#[cfg(unix)]
#[test]
fn volatile_cache_stays_fresh_within_ttl() {
    // AC2: a re-invocation inside the TTL is a cache hit — npm is not re-run.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let within_ttl =
        base_now() + super::VOLATILE_SELECTOR_TTL - std::time::Duration::from_secs(1);
    let second = finalize_local_invocation_at(&candidate, cache.path(), within_ttl)
        .unwrap_or_else(|error| panic!("second invocation: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "within-TTL re-invocation must be a cache hit (npm not re-run)"
    );
    assert_eq!(
        first.executable(),
        second.executable(),
        "cache hit must reuse the same managed executable"
    );
}

#[cfg(unix)]
#[test]
fn volatile_cache_re_resolves_after_ttl() {
    // AC1: once the install age exceeds the TTL, the cache is a miss and npm
    // re-runs (re-resolving the moving dist-tag).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let expired = base_now() + super::VOLATILE_SELECTOR_TTL + std::time::Duration::from_secs(1);
    finalize_local_invocation_at(&candidate, cache.path(), expired)
        .unwrap_or_else(|error| panic!("expired re-resolve: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        2,
        "past-TTL re-invocation must re-run npm to re-resolve the dist-tag"
    );
}

#[cfg(unix)]
#[test]
fn volatile_old_marker_without_timestamp_re_installs() {
    // AC3: a legacy/stuck marker (3 lines, no timestamp) for a volatile selector
    // is treated as expired and re-installed, writing a fresh timestamped marker.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let install_dir = install_dir_of(first.executable()).to_path_buf();
    // Simulate a stuck/legacy cache: overwrite the marker with the old 3-line
    // form (no install-time line) while keeping the binary present.
    let selection = candidate
        .package()
        .unwrap_or_else(|| panic!("volatile candidate carries a package selection"));
    let effective = selection
        .selector()
        .effective(selection.runner())
        .unwrap_or_default();
    let legacy_marker =
        format!("{}
{}
{}
", selection.package(), selection.binary(), effective);
    std::fs::write(install_dir.join(".jefe-installed"), legacy_marker)
        .unwrap_or_else(|error| panic!("write legacy marker: {error}"));
    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "setup: one install recorded so far"
    );

    // Any `now` should treat the timestamp-less marker as expired.
    finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("legacy re-resolve: {error}"));
    assert_eq!(
        witness_lines(&witness).len(),
        2,
        "a timestamp-less volatile marker must trigger a re-install (auto-heal)"
    );
    let refreshed = std::fs::read_to_string(install_dir.join(".jefe-installed"))
        .unwrap_or_else(|error| panic!("read refreshed marker: {error}"));
    let lines: Vec<&str> = refreshed.lines().collect();
    assert!(
        lines.len() >= 4,
        "refreshed volatile marker must carry an install-time line: {refreshed:?}"
    );
    // The identity lines must be preserved across the refresh (no corruption).
    assert_eq!(lines[0], selection.package(), "package line preserved");
    assert_eq!(lines[1], selection.binary(), "binary line preserved");
    assert_eq!(lines[2], effective, "effective-selector line preserved");
    assert!(
        lines[3].parse::<u64>().is_ok(),
        "the 4th line must be a valid install-time epoch, got {:?}",
        lines[3]
    );
}

#[cfg(unix)]
#[test]
fn pinned_cache_remains_permanent_hit() {
    // AC4: an explicit (pinned) version is immutable — the cache is a permanent
    // hit regardless of how much time has passed.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    let candidate = volatile_npm_candidate(&bin, "0.10.0-nightly.260720.abc");
    assert!(
        !candidate
            .package()
            .unwrap_or_else(|| panic!("pinned candidate"))
            .selector()
            .is_volatile(),
        "fixture: explicit version is not volatile"
    );
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    // Far beyond any TTL: a pinned version must still be a cache hit.
    let far_future = base_now() + std::time::Duration::from_secs(365 * 24 * 60 * 60);
    finalize_local_invocation_at(&candidate, cache.path(), far_future)
        .unwrap_or_else(|error| panic!("pinned re-invocation: {error}"));
    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "a pinned version must be a permanent cache hit"
    );
}

#[cfg(unix)]
#[test]
fn volatile_re_resolve_removes_stale_lockfile() {
    // AC5 (#554): when a volatile cache expires, `npm install` must not observe a
    // prior package-lock.json, so the dist-tag is re-resolved instead of reused,
    // and the stale lockfile must not survive into the refreshed cache entry.
    // Since #556 this is guaranteed structurally: the re-install is built in a
    // fresh staging directory and promoted by rename, so nothing from the
    // previous entry can leak into it.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let install_dir = install_dir_of(first.executable()).to_path_buf();
    // Plant a stale lockfile exactly where npm would leave it.
    std::fs::write(install_dir.join("package-lock.json"), "stale-lockfile")
        .unwrap_or_else(|error| panic!("plant stale lockfile: {error}"));

    let expired = base_now() + super::VOLATILE_SELECTOR_TTL + std::time::Duration::from_secs(1);
    finalize_local_invocation_at(&candidate, cache.path(), expired)
        .unwrap_or_else(|error| panic!("expired re-resolve: {error}"));

    let lines = witness_lines(&witness);
    assert_eq!(lines.len(), 2, "expired volatile cache must re-run npm");
    assert_eq!(
        lines[1], "absent",
        "npm must not see the stale package-lock.json when it re-resolves (got {lines:?})",
    );
    assert!(
        !install_dir.join("package-lock.json").exists(),
        "the stale lockfile must remain absent after the re-resolve completes"
    );
}
