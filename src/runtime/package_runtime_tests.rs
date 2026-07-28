use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{PackageExecutionTarget, finalize_local_invocation, package_invocation};
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
fn resolve_package(
    definition: &AgentDefinition,
    bin: &TempDir,
    selector: &str,
) -> ResolvedCandidate {
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![bin.path().to_path_buf()],
        None,
    );
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
    let candidate = resolve_package(&definition, &bin, "0.0.634");
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
    let candidate = resolve_package(&definition, &bin, "2.0.0");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let plan = local_base_plan(&definition, &candidate, 8, cache.path());
    assert!(plan.argv.is_empty(), "managed binary receives only emitted argv");
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
    let candidate = resolve_package(&definition, &bin, "latest nightly");
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
    let candidate = resolve_package(&definition, &bin, "1.2.3");
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
    let candidate = resolve_package(&definition, &bin, "latest");
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
    let candidate = resolve_package(&definition, &bin, "latest");
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
