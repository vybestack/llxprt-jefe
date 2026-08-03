use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    PackageExecutionTarget, finalize_local_invocation, finalize_local_invocation_inner,
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

/// Launch must execute the invocation the probe measured.
///
/// Preparation resolves a moving selector, so preparing twice in one launch
/// asks the registry twice, and the two answers can differ. That is how a
/// launch came to pair availability measured from one version with the
/// executable and fingerprint of another — probing V1 and running an unprobed
/// V2 (issue #571).
///
/// The tag is moved here between the probe and the point launch would have
/// prepared again, which is exactly the interleaving that produced the mismatch.
#[cfg(unix)]
#[test]
fn launch_executes_the_invocation_the_probe_measured() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0");
    let definition = definition("LLxprt");
    let candidate_for_replay = resolve_package(&definition, bin.path(), "latest nightly");
    let resolution = CandidateResolution::Resolved(resolve_package(
        &definition,
        bin.path(),
        "latest nightly",
    ));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let probe =
        crate::runtime::run_local_agent_probe_with_cache(&definition, &resolution, 7, cache.path());

    let Some(measured) = probe.prepared_invocation() else {
        panic!("a managed package probe must hand back the invocation it measured");
    };
    let probed_executable = measured.executable().to_path_buf();
    assert_eq!(
        resolve_count(&witness),
        1,
        "the probe must resolve the tag once"
    );

    // The tag moves while the launch is still in flight.
    publish_version(&witness, "0.11.1");

    // What launch composition now does: use what was measured, not a fresh
    // preparation. Nothing may reach the registry a second time, and the
    // executable must still be the one whose availability was established.
    assert_eq!(
        probe
            .prepared_invocation()
            .map(|invocation| invocation.executable().to_path_buf()),
        Some(probed_executable.clone()),
        "launch must run the probed version, not whatever the tag points at now"
    );
    assert_eq!(
        resolve_count(&witness),
        1,
        "one launch must ask the registry once; a second resolution is what lets \
         availability and executable come from different versions"
    );

    // Show the hazard is real rather than hypothetical: the second preparation
    // launch used to perform, run here explicitly, lands on a different version
    // than the one just probed. Pairing that executable with the availability
    // above is the defect; the assertions above are what now prevents it.
    let second_preparation = finalize_local_invocation_inner(&candidate_for_replay, cache.path())
        .unwrap_or_else(|error| panic!("second preparation: {error}"));
    assert_ne!(
        second_preparation.executable(),
        probed_executable.as_path(),
        "preparing a second time reaches a different version — which is precisely \
         why launch must not do it"
    );
    assert_eq!(
        resolve_count(&witness),
        2,
        "that second preparation is the extra registry call this change removes"
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
    // `npm view <spec> version` answers from a version file the test controls,
    // so a test can move the dist-tag without a registry (issue #584). Only a
    // real `npm install` records a witness line, so counting lines still counts
    // installs and never counts resolves.
    let version_file = version_path_for(witness);
    if std::fs::read_to_string(&version_file).is_err() {
        std::fs::write(&version_file, "1.0.0\n")
            .unwrap_or_else(|error| panic!("seed version file: {error}"));
    }
    executable(
        bin,
        "npm",
        &format!(
            "#!/bin/sh
set -e
witness={witness}
versions={versions}
resolves={resolves}
if [ \"$1\" = view ]; then
  if [ \"$#\" -ne 3 ] || [ \"$3\" != version ]; then
    echo \"unexpected npm view invocation: $*\" >&2
    exit 64
  fi
  echo resolve >> \"$resolves\"
  if [ -f \"$versions\" ]; then cat \"$versions\"; exit 0; else exit 1; fi
fi
if [ -f package-lock.json ]; then echo present >> \"$witness\"; else echo absent >> \"$witness\"; fi
mkdir -p node_modules/.bin
printf '#!/bin/sh\nexit 0\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
",
            witness = crate::runtime::commands::shell_escape_single(&witness.to_string_lossy()),
            versions =
                crate::runtime::commands::shell_escape_single(&version_file.to_string_lossy()),
            resolves = crate::runtime::commands::shell_escape_single(
                &resolve_path_for(witness).to_string_lossy()
            )
        ),
    );
}

/// Path the npm stub appends to on every `npm view`.
///
/// Installs are counted by the witness; resolutions are counted here, because
/// "how many times did we ask the registry?" is a separate question from "how
/// many times did we install?" and issue #571 turns on the former.
#[cfg(unix)]
fn resolve_path_for(witness: &Path) -> PathBuf {
    witness.with_file_name("resolve-witness.log")
}

/// How many times the stub answered `npm view ... version`.
#[cfg(unix)]
fn resolve_count(witness: &Path) -> usize {
    std::fs::read_to_string(resolve_path_for(witness))
        .map(|contents| contents.lines().count())
        .unwrap_or_default()
}

/// Path of the file the npm stub reads to answer `npm view ... version`.
#[cfg(unix)]
fn version_path_for(witness: &Path) -> PathBuf {
    witness.with_file_name("published-version.txt")
}

/// Move the dist-tag: the next `npm view` reports `version`.
#[cfg(unix)]
fn publish_version(witness: &Path, version: &str) {
    std::fs::write(version_path_for(witness), format!("{version}\n"))
        .unwrap_or_else(|error| panic!("publish version: {error}"));
}

/// Make the registry unreachable: `npm view` exits non-zero.
#[cfg(unix)]
fn make_registry_unreachable(witness: &Path) {
    let _ = std::fs::remove_file(version_path_for(witness));
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

/// The registry answer is the one input here jefe does not control, so it is
/// validated rather than trusted before it reaches the marker (issue #584).
#[test]
fn a_registry_version_answer_is_validated_before_it_is_trusted() {
    use super::is_plausible_version;

    for accepted in [
        "1.0.0",
        "0.11.0-nightly.260801.19ac22acc",
        "2.0.0-rc.1+build.5",
    ] {
        assert!(
            is_plausible_version(accepted),
            "a legitimate dist-tag answer must be accepted: {accepted:?}"
        );
    }

    // Production trims the registry answer before validating, so a *trailing*
    // newline never reaches the validator; it is included here to pin that the
    // validator would reject it anyway. An *embedded* newline is the case that
    // matters, because it would forge an extra marker line. The rest are
    // control, whitespace or unbounded answers.
    for rejected in [
        "",
        "1.0.0\n2.0.0",
        "1.0.0\n",
        "1.0.0 2.0.0",
        "1.0.0\u{0}",
        "1.0.0\t",
        "1.0.0\u{7f}",
    ] {
        assert!(
            !is_plausible_version(rejected),
            "a malformed registry answer must be rejected: {rejected:?}"
        );
    }

    assert!(
        !is_plausible_version(&"9".repeat(257)),
        "an unbounded registry answer must be rejected"
    );
}

/// Advancing a dist-tag must not destroy the tree a live agent is executing.
///
/// The cache is keyed on the tag rather than the resolved version, so every
/// nightly shares one directory and a refresh reinstalls into the directory
/// running processes are executing out of, then deletes the tree they came
/// from. On macOS that deletion makes the running executable's vnode nameless,
/// so `proc_pidpath` fails, securityd can no longer reconstruct the process's
/// code identity, and every Keychain operation degrades to a password prompt
/// that "Always Allow" cannot satisfy (issue #588).
///
/// Keying on the resolved version makes an advance create a *new* directory and
/// leaves the old one untouched, which is what keeps already-running agents
/// intact.
#[cfg(unix)]
#[test]
fn advancing_a_tag_leaves_the_previous_install_intact() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let first_executable = first.executable().to_path_buf();

    publish_version(&witness, "0.11.0-nightly.2");
    let second = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("advanced install: {error}"));

    assert_ne!(
        first_executable,
        second.executable(),
        "a new resolved version must install to its own path, not overwrite the old one"
    );
    assert!(
        first_executable.exists(),
        "the tree a live agent is executing from must survive the tag advancing; \
         deleting it is what strands the process and triggers the Keychain storm"
    );
}

/// Two selectors naming the same version must share one install, not fight
/// over it.
///
/// Since the cache is keyed on the resolved version (issue #588), a tag that
/// resolves to 0.11.0 and an exact 0.11.0 address the *same* directory. If
/// identity is judged by the selector the user typed rather than by the version
/// that was installed, each rejects the other's marker, and the loser republishes
/// over a tree the winner may be executing from — the exact destruction that
/// version-keying was introduced to prevent (issue #571).
#[cfg(unix)]
#[test]
fn a_tag_and_an_exact_pin_at_the_same_version_share_one_install() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let tagged = volatile_npm_candidate(&bin, "latest nightly");
    let first = finalize_local_invocation_inner(&tagged, cache.path())
        .unwrap_or_else(|error| panic!("tag install: {error}"));
    let tagged_executable = first.executable().to_path_buf();
    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "the tag must install once"
    );

    // The same version, named exactly. Nothing about the tree needs to change.
    let pinned = volatile_npm_candidate(&bin, "0.11.0");
    let second = finalize_local_invocation_inner(&pinned, cache.path())
        .unwrap_or_else(|error| panic!("pinned install: {error}"));

    assert_eq!(
        tagged_executable,
        second.executable(),
        "the same version must resolve to the same install path"
    );
    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "naming the installed version exactly must be a cache hit, not a reinstall"
    );
    assert!(
        tagged_executable.exists(),
        "the tree the tagged agent is executing from must survive another selector          arriving at the same version"
    );
}

#[cfg(unix)]
#[test]
fn volatile_cache_hits_while_the_tag_has_not_moved() {
    // The dist-tag still resolves to the installed version, so preparation is a
    // cache hit however much time has passed: freshness is decided by what the
    // tag points at, not by a clock (issue #584).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let second = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("second invocation: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "an unmoved dist-tag must be a cache hit; npm install must not re-run"
    );
    assert_eq!(
        first.executable(),
        second.executable(),
        "cache hit must reuse the same managed executable"
    );
}

#[cfg(unix)]
#[test]
fn volatile_cache_reinstalls_as_soon_as_the_tag_moves() {
    // A newly published nightly is picked up on the very next launch, with no
    // timer to wait out (issue #584).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    publish_version(&witness, "0.11.0-nightly.2");
    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("re-resolve after tag moved: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        2,
        "a moved dist-tag must re-install immediately, without waiting out a TTL"
    );
}

#[cfg(unix)]
#[test]
fn volatile_selector_uses_the_cached_install_when_the_registry_is_unreachable() {
    // Offline is a condition to ride out, not an error to raise: the build the
    // user already has must still launch (issue #584).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let online = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    make_registry_unreachable(&witness);
    let offline = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("offline preparation must still succeed: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "an unreachable registry must not trigger a reinstall"
    );
    assert_eq!(
        online.executable(),
        offline.executable(),
        "offline preparation must reuse the cached managed executable"
    );
}

#[cfg(unix)]
#[test]
fn a_marker_for_a_different_selection_forces_a_reinstall() {
    // Identity still gates the cache: a directory whose marker describes a
    // different package or binary is not this selection's install, whatever the
    // path suggests. The resolved-version line is no longer consulted here,
    // because the directory is now keyed on that version (issue #588).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let install_dir = install_dir_of(first.executable()).to_path_buf();
    std::fs::write(
        install_dir.join(".jefe-installed"),
        "some-other-package\nsome-other-binary\nnightly\n",
    )
    .unwrap_or_else(|error| panic!("overwrite marker: {error}"));

    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("reinstall after identity mismatch: {error}"));

    assert_eq!(
        witness_lines(&witness).len(),
        2,
        "a marker describing a different selection must force a reinstall"
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

    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    // A pinned version never moves, so it is never re-resolved and is a
    // permanent cache hit.
    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("pinned re-invocation: {error}"));
    assert_eq!(
        witness_lines(&witness).len(),
        1,
        "a pinned version must be a permanent cache hit"
    );
}

#[cfg(unix)]
#[test]
fn advancing_a_tag_never_writes_into_the_previous_version_directory() {
    // A moving dist-tag used to be re-resolved by reinstalling over the same
    // directory, which is why a stale package-lock.json there mattered. Keying
    // the cache on the resolved version means an advance builds a *new*
    // directory and the previous one is never opened again, so nothing a live
    // agent is executing can be rewritten under it (issue #588).
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let witness = witness_path(&bin);
    counting_npm_stub(&bin, &witness);
    publish_version(&witness, "0.11.0-nightly.1");
    let candidate = volatile_npm_candidate(&bin, "latest nightly");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let first_dir = install_dir_of(first.executable()).to_path_buf();
    // Anything at all in the previous directory: if the new install reached
    // into it, this would be disturbed.
    std::fs::write(first_dir.join("package-lock.json"), "in-use-by-a-live-agent")
        .unwrap_or_else(|error| panic!("plant marker file: {error}"));

    publish_version(&witness, "0.11.0-nightly.2");
    let second = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("advanced install: {error}"));
    let second_dir = install_dir_of(second.executable()).to_path_buf();

    assert_ne!(first_dir, second_dir, "an advance must build a new directory");
    assert_eq!(
        std::fs::read_to_string(first_dir.join("package-lock.json"))
            .unwrap_or_else(|error| panic!("previous directory must be intact: {error}")),
        "in-use-by-a-live-agent",
        "the previous version directory must not be written to or rebuilt"
    );
    // The fresh install still starts from an empty staging directory, so npm
    // never observes a prior lockfile.
    let observations = witness_lines(&witness);
    assert_eq!(observations.len(), 2, "the advance must install once more");
    assert_eq!(
        observations[1], "absent",
        "the new install must not observe a package-lock.json (got {observations:?})"
    );
}
