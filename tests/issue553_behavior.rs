//! Issue #553 — AGT-E202 probe timeout when sending an issue to an agent.
//!
//! Every test drives the production probe boundary
//! (`jefe::runtime::run_local_agent_probe`) against a real fixture process, so
//! the assertions describe observable launch behavior rather than helper shape.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution, VersionSelector};
use jefe::agent_candidate_path::PathSnapshot;
use jefe::domain::agent_definition::{AgentDefinition, Availability, ProbeErrorCode};
use jefe::runtime::{AgentExecutablePlatform, AgentProbeResult, run_local_agent_probe};

/// Selector used by every package-runner fixture in this file.
const PINNED_SELECTOR: &str = "0.0.600";

/// Authored probe budget: comfortably above fixture process startup on a
/// loaded machine, and comfortably below the fixture delay below, so every
/// timing assertion stays bounded and deterministic.
const SHORT_PROBE_TIMEOUT_MS: u64 = 1_500;

/// Fixture delay, in seconds, written into the delay marker the fixture
/// reads. Comfortably above the authored budget above.
const FIXTURE_DELAY_SECONDS: &[u8] = b"4\n";

/// Fixture runner/agent. It records its full argument vector and dispatches on
/// the final argument, so the same script serves as a direct agent executable
/// and as a `uvx` style package runner that forwards trailing agent arguments.
const FIXTURE_PROCESS: &str = r#"#!/bin/sh
set -eu
dir=${0%/*}
printf '%s\n' "$*" >> "$dir/invocations"
last=""
for argument in "$@"; do
    last="$argument"
done
case "$last" in
--version)
    if [ -f "$dir/identity.sleep" ]; then sleep "$(cat "$dir/identity.sleep")"; fi
    cat "$dir/identity.stdout"
    ;;
--help)
    if [ -f "$dir/help.sleep" ]; then sleep "$(cat "$dir/help.sleep")"; fi
    cat "$dir/help.stdout"
    ;;
*)
    exit 64
    ;;
esac
exit 0
"#;

const IDENTITY_LINE: &[u8] = b"0.0.600\n";
const HELP_TEXT: &[u8] = b"--interactive --model --resume --quick-resume --yolo\n";

/// One resolved fixture installation plus the definition that probes it.
struct Fixture {
    _temp: tempfile::TempDir,
    definition: AgentDefinition,
    resolution: CandidateResolution,
    executable: PathBuf,
}

impl Fixture {
    /// Fixture resolved through the shipped uvx package-runner candidate, so
    /// the probe executes `uvx --from <spec> <binary> <agent arguments>`.
    fn package_runner() -> Self {
        Self::new("uvx", PINNED_SELECTOR)
    }

    /// Fixture resolved through the shipped direct PATH candidate.
    fn direct() -> Self {
        Self::new("code-puppy", "")
    }

    fn new(program: &str, selector: &str) -> Self {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        // Both the runner and the direct agent resolve from the same fixture
        // directory; the selector decides which candidate the resolver picks.
        for name in ["uvx", "code-puppy"] {
            write_executable(&temp.path().join(name), FIXTURE_PROCESS.as_bytes());
        }
        let executable = temp.path().join(program);
        write_file(temp.path(), "identity.stdout", IDENTITY_LINE);
        write_file(temp.path(), "help.stdout", HELP_TEXT);

        let mut definition = shipped("core.code-puppy");
        definition.probe.timeout_ms = SHORT_PROBE_TIMEOUT_MS;

        let snapshot = PathSnapshot::for_platform(
            AgentExecutablePlatform::current(),
            vec![temp.path().to_path_buf()],
            None,
        );
        let resolution = AgentCandidateResolver::new(&snapshot, temp.path().to_path_buf())
            .with_version_selector(normalized_selector(selector))
            .resolve(&definition);
        assert!(
            resolution.is_resolved(),
            "fixture candidate must resolve: {resolution:?}"
        );
        Self {
            _temp: temp,
            definition,
            resolution,
            executable,
        }
    }

    /// Make the named phase sleep past the authored probe budget. The marker
    /// carries the delay, so the fixture script needs no substitution.
    fn delay(&self, marker: &str) -> &Self {
        write_file(
            self.executable
                .parent()
                .unwrap_or_else(|| panic!("fixture parent")),
            marker,
            FIXTURE_DELAY_SECONDS,
        );
        self
    }

    fn run(&self, generation: u64) -> AgentProbeResult {
        run_local_agent_probe(&self.definition, &self.resolution, generation)
    }
}

fn normalized_selector(selector: &str) -> VersionSelector {
    VersionSelector::normalize(selector).unwrap_or_else(|error| panic!("selector: {error}"))
}

fn shipped(id: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .unwrap_or_else(|| panic!("{id} must be shipped"))
}

fn write_executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

fn write_file(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

fn probe_error_reason(result: &AgentProbeResult) -> String {
    match result.availability() {
        Availability::ProbeError { code, reason, .. } => {
            assert_eq!(
                *code,
                ProbeErrorCode::Agte202,
                "probe failures are AGT-E202"
            );
            reason.clone()
        }
        other => panic!("expected a probe error, got {other:?}"),
    }
}

fn assert_compatible(result: &AgentProbeResult, generation: u64) {
    match result.availability() {
        Availability::InstalledCompatible {
            identity,
            generation: actual,
            ..
        } => {
            assert_eq!(identity, "0.0.600", "identity comes from the fixture");
            assert_eq!(*actual, generation, "the requested stamp is preserved");
        }
        other => panic!("expected an installed compatible probe, got {other:?}"),
    }
}

/// A1: a package runner materializes its environment as part of the first
/// process it runs. That download is not agent startup latency, so it must not
/// be charged to the authored probe timeout.
#[test]
fn runner_mediated_identity_probe_is_not_bounded_by_the_authored_probe_timeout() {
    let fixture = Fixture::package_runner();
    fixture.delay("identity.sleep");

    assert_compatible(&fixture.run(1), 1);
}

/// A2: a directly resolved executable performs no materialization, so the
/// authored probe timeout still bounds it exactly as before.
#[test]
fn direct_identity_probe_remains_bounded_by_the_authored_probe_timeout() {
    let fixture = Fixture::direct();
    fixture.delay("identity.sleep");

    let reason = probe_error_reason(&fixture.run(2));
    assert!(reason.contains("timed out"), "reason {reason:?}");
}

/// A3: once identity has run, the package runner environment is materialized,
/// so every later phase is bounded by the ordinary authored probe timeout.
#[test]
fn runner_mediated_capability_probe_uses_the_ordinary_probe_budget() {
    let fixture = Fixture::package_runner();
    fixture.delay("help.sleep");

    let reason = probe_error_reason(&fixture.run(3));
    assert!(reason.contains("timed out"), "reason {reason:?}");
    assert!(
        reason.contains("capability"),
        "the capability phase owns this failure: {reason:?}"
    );
}
/// A4: a timeout must identify the phase, the executable that was run, how long
/// it actually took, and the budget it exceeded.
#[test]
fn probe_timeout_reason_names_phase_executable_elapsed_and_budget() {
    let fixture = Fixture::direct();
    fixture.delay("identity.sleep");

    let reason = probe_error_reason(&fixture.run(4));
    assert!(
        reason.contains("identity"),
        "reason names its phase: {reason:?}"
    );
    assert!(
        reason.contains(&fixture.executable.display().to_string()),
        "reason names the executable that was run: {reason:?}"
    );
    assert!(
        reason.contains(&format!("budget {SHORT_PROBE_TIMEOUT_MS} ms")),
        "reason names the budget it exceeded: {reason:?}"
    );
    assert!(
        reason.contains("ms)") && reason.contains("after "),
        "reason names how long the phase actually took: {reason:?}"
    );
}

/// A5: non-timeout probe failures are equally attributable.
#[test]
fn probe_failure_reasons_name_their_phase_and_executable() {
    let fixture = Fixture::direct();
    write_file(
        fixture
            .executable
            .parent()
            .unwrap_or_else(|| panic!("fixture parent")),
        "identity.stdout",
        b"not a version at all\n",
    );

    let reason = probe_error_reason(&fixture.run(5));
    assert!(
        reason.contains("identity"),
        "reason names its phase: {reason:?}"
    );
    assert!(
        reason.contains(&fixture.executable.display().to_string()),
        "reason names the executable that was run: {reason:?}"
    );
}

/// Fixture agent installed as a repository-local LLxprt, recording every
/// argument vector any probe runs against it.
struct RecordingAgent {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

const RECORDING_AGENT: &str = r#"#!/bin/sh
set -eu
dir=${0%/*}
printf '%s\n' "$*" >> "$dir/invocations"
printf '0.11.0\n'
exit 0
"#;

impl RecordingAgent {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let bin = temp.path().join(".llxprt/bin");
        fs::create_dir_all(&bin).unwrap_or_else(|error| panic!("mkdir: {error}"));
        write_executable(&bin.join("llxprt"), RECORDING_AGENT.as_bytes());
        let root = temp.path().to_path_buf();
        Self { _temp: temp, root }
    }

    fn invocations(&self) -> Vec<String> {
        fs::read_to_string(self.root.join(".llxprt/bin/invocations"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn request(&self) -> jefe::domain::AgentLaunchRequest {
        let mut values = jefe::domain::TypedMap::new();
        for (field, value) in [
            (
                "profile",
                jefe::domain::TypedValue::String("fixture".to_owned()),
            ),
            ("yolo", jefe::domain::TypedValue::Bool(true)),
            ("prompt-interactive", jefe::domain::TypedValue::Bool(true)),
        ] {
            let key = jefe::domain::Id::parse(field)
                .unwrap_or_else(|error| panic!("{field} key: {error}"));
            values.insert(key, value);
        }
        jefe::domain::AgentLaunchRequest {
            type_id: shipped("core.llxprt").id,
            values,
            work_dir: self.root.clone(),
            remote: jefe::domain::RemoteRepositorySettings::default(),
            operation: jefe::domain::agent_definition::Operation::Normal,
        }
    }
}

/// A7: the pre-side-effect guard validates a launch request without executing
/// the agent, so one send costs exactly one authoritative probe.
#[test]
fn validate_launch_accepts_without_executing_a_probe() {
    let agent = RecordingAgent::new();
    let request = agent.request();

    let outcome = jefe::runtime::launch_compose::validate_launch(&request);

    assert!(outcome.is_ok(), "valid request must validate: {outcome:?}");
    assert!(
        agent.invocations().is_empty(),
        "validation must not execute the agent: {:?}",
        agent.invocations()
    );
}

/// A7 contrast: launch-state observation remains process-free; the
/// authoritative preparation boundary performs the single probe.
#[test]
fn launch_preparation_still_probes_the_same_candidate() {
    let agent = RecordingAgent::new();
    let request = agent.request();

    let evidence = jefe::runtime::launch_compose::observe_launch_state(&request);
    assert!(evidence.is_ok(), "fixture must be observable: {evidence:?}");
    assert!(
        agent.invocations().is_empty(),
        "observation must not execute the agent: {:?}",
        agent.invocations()
    );

    let prepared = evidence
        .and_then(|evidence| jefe::runtime::launch_compose::prepare_launch(&request, &evidence));
    assert!(prepared.is_ok(), "fixture must prepare: {prepared:?}");
    assert_eq!(
        agent.invocations(),
        vec!["--version".to_owned()],
        "preparation owns the authoritative probe"
    );
}

/// A6: a rejected request is rejected without executing the agent either.
#[test]
fn validate_launch_rejects_without_executing_a_probe() {
    let agent = RecordingAgent::new();
    let mut request = agent.request();
    request.remote.enabled = true;

    let outcome = jefe::runtime::launch_compose::validate_launch(&request);

    assert!(outcome.is_err(), "an incomplete remote must be rejected");
    assert!(
        agent.invocations().is_empty(),
        "rejection must not execute the agent: {:?}",
        agent.invocations()
    );
}
