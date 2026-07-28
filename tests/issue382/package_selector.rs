use std::path::Path;

use jefe::agent_candidate::{
    AgentCandidateResolver, CandidateResolution, CandidateSkip, VersionSelector,
    next_probe_generation,
};
use jefe::agent_candidate_path::PathSnapshot;
use jefe::domain::agent_definition::{AgentDefinition, AgentLaunchPlan, Operation};
use jefe::runtime::AgentExecutablePlatform;
use jefe::runtime::agent_plan::LaunchFieldValues;
use jefe::runtime::package_runtime::{
    PackageExecutionTarget, finalize_local_invocation, package_invocation,
};

pub fn assert_runtime_matrix(
    definitions: &[AgentDefinition],
    local_plan: impl Fn(&AgentDefinition, Operation, &str, &LaunchFieldValues) -> AgentLaunchPlan,
) {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("package bin: {error}"));
    write_fixtures(&bin);
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![bin.path().to_path_buf()],
        None,
    );
    let candidates = candidate_matrix(definitions, &snapshot, bin.path());
    assert_absent_runner_no_fallback(definitions);
    assert_invocations(&candidates);
    let (definition, candidate) = candidates
        .iter()
        .find(|(definition, _)| definition.display_name == "LLxprt")
        .unwrap_or_else(|| panic!("LLxprt package candidate"));
    assert_generation_change(definition, candidate, &snapshot, bin.path());
    assert_preparation_order(definition, candidate, local_plan);
}

fn write_fixtures(bin: &tempfile::TempDir) {
    use std::os::unix::fs::PermissionsExt;

    for name in ["claude", "code-puppy", "codex", "llxprt", "npm", "uvx"] {
        let path = bin.path().join(name);
        let script = if name == "npm" {
            "#!/bin/sh\nmkdir -p node_modules/.bin\nprintf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt\nchmod 755 node_modules/.bin/llxprt\n"
        } else {
            "#!/bin/sh\nexit 0\n"
        };
        std::fs::write(&path, script).unwrap_or_else(|error| panic!("write {name}: {error}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("chmod {name}: {error}"));
    }
}

fn candidate_matrix<'a>(
    definitions: &'a [AgentDefinition],
    snapshot: &PathSnapshot,
    root: &Path,
) -> Vec<(
    &'a AgentDefinition,
    jefe::agent_candidate::ResolvedCandidate,
)> {
    let selected =
        VersionSelector::normalize("latest").unwrap_or_else(|error| panic!("selector: {error}"));
    definitions
        .iter()
        .map(|definition| {
            let direct =
                AgentCandidateResolver::new(snapshot, root.to_path_buf()).resolve(definition);
            assert!(
                direct
                    .resolved()
                    .is_some_and(|candidate| candidate.package().is_none())
            );
            let package = AgentCandidateResolver::new(snapshot, root.to_path_buf())
                .with_version_selector(selected.clone())
                .resolve(definition);
            let candidate = package
                .resolved()
                .unwrap_or_else(|| panic!("{} package runner resolves", definition.id));
            assert!(candidate.package().is_some());
            (definition, candidate.clone())
        })
        .collect()
}

fn assert_absent_runner_no_fallback(definitions: &[AgentDefinition]) {
    let empty = tempfile::tempdir().unwrap_or_else(|error| panic!("empty bin: {error}"));
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::Unix,
        vec![empty.path().to_path_buf()],
        None,
    );
    let selected =
        VersionSelector::normalize("latest").unwrap_or_else(|error| panic!("selector: {error}"));
    for definition in definitions {
        let resolution = AgentCandidateResolver::new(&snapshot, empty.path().to_path_buf())
            .with_version_selector(selected.clone())
            .resolve(definition);
        let CandidateResolution::NotFound(skips) = resolution else {
            panic!("{} absent runner cannot fall back", definition.id);
        };
        assert!(
            skips
                .iter()
                .any(|skip| matches!(skip, CandidateSkip::RunnerAbsent { .. }))
        );
        assert!(
            skips
                .iter()
                .any(|skip| matches!(skip, CandidateSkip::DirectSuppressedBySelector { .. }))
        );
    }
}

fn assert_invocations(candidates: &[(&AgentDefinition, jefe::agent_candidate::ResolvedCandidate)]) {
    let cache = Path::new("/unused");
    let (_, npm) = candidates
        .iter()
        .find(|(definition, _)| definition.display_name == "LLxprt")
        .unwrap_or_else(|| panic!("npm candidate"));
    let remote = package_invocation(npm, PackageExecutionTarget::Remote, cache)
        .unwrap_or_else(|error| panic!("remote npm invocation: {error}"))
        .unwrap_or_else(|| panic!("npm invocation"));
    assert_eq!(remote.executable(), Path::new("npm"));
    assert_eq!(
        remote.prefix(),
        [
            "exec",
            "--yes",
            "--package=@vybestack/llxprt-code@latest",
            "--",
            "llxprt"
        ]
        .map(std::ffi::OsString::from)
    );
    let (_, uvx) = candidates
        .iter()
        .find(|(definition, _)| definition.display_name == "Code Puppy")
        .unwrap_or_else(|| panic!("uvx candidate"));
    let local = package_invocation(uvx, PackageExecutionTarget::Local, cache)
        .unwrap_or_else(|error| panic!("local uvx invocation: {error}"))
        .unwrap_or_else(|| panic!("uvx invocation"));
    assert_eq!(
        local.prefix(),
        ["--from", "code-puppy", "code-puppy"].map(std::ffi::OsString::from)
    );
}

fn assert_generation_change(
    definition: &AgentDefinition,
    candidate: &jefe::agent_candidate::ResolvedCandidate,
    snapshot: &PathSnapshot,
    root: &Path,
) {
    let first = candidate.generation_key(definition);
    let changed = AgentCandidateResolver::new(snapshot, root.to_path_buf())
        .with_version_selector(
            VersionSelector::normalize("2.0.0")
                .unwrap_or_else(|error| panic!("changed selector: {error}")),
        )
        .resolve(definition);
    let changed = changed
        .resolved()
        .unwrap_or_else(|| panic!("changed candidate"))
        .generation_key(definition);
    assert_eq!(next_probe_generation(Some(&first), &changed, 10), Ok(11));
}

fn assert_preparation_order(
    definition: &AgentDefinition,
    candidate: &jefe::agent_candidate::ResolvedCandidate,
    local_plan: impl Fn(&AgentDefinition, Operation, &str, &LaunchFieldValues) -> AgentLaunchPlan,
) {
    let executable = candidate
        .executable()
        .to_str()
        .unwrap_or_else(|| panic!("UTF-8 fixture path"));
    let plan = local_plan(
        definition,
        Operation::Normal,
        executable,
        &LaunchFieldValues::new(),
    );
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    assert_ne!(plan.probe_generation, 2);
    assert!(
        std::fs::read_dir(cache.path())
            .unwrap_or_else(|error| panic!("read cache: {error}"))
            .next()
            .is_none()
    );
    assert_eq!(plan.probe_generation, 1);
    let managed = finalize_local_invocation(candidate, cache.path())
        .unwrap_or_else(|error| panic!("managed npm preparation: {error}"));
    assert!(managed.executable().starts_with(cache.path()));
    assert!(managed.prefix().is_empty());
}
