//! Production-connected runtime probe tests for issue #382 S3c/S3d.

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution, ResolvedCandidate};
#[cfg(unix)]
use jefe::agent_candidate_path::PathSnapshot;
#[cfg(unix)]
use jefe::domain::agent_definition::probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeStream,
};
#[cfg(unix)]
use jefe::domain::agent_definition::{AgentDefinition, Availability, ProbeErrorCode};
use jefe::runtime::AgentProbeTarget;
#[cfg(unix)]
use jefe::runtime::{AgentProbeResult, run_local_agent_probe};

#[cfg(unix)]
use super::probe_fixtures::{read_bytes, read_json, retained_fixture_dirs};

#[cfg(unix)]
const FAKE_PROBE: &str = r#"#!/bin/sh
set -eu
dir=${0%/*}
printf '%s\n' "$1" >> "$dir/invocations"
if [ "$1" = "--version" ]; then
    if [ -f "$dir/identity.sleep" ]; then sleep 2; fi
    cat "$dir/identity.stdout"
    cat "$dir/identity.stderr" >&2
    if [ -f "$dir/identity.replace" ]; then mv "$dir/replacement" "$0"; fi
    if [ -f "$dir/identity.signal" ]; then kill -TERM $$; fi
    if [ -f "$dir/identity.nonzero" ]; then exit 7; fi
    exit 0
fi
exit 64
"#;

#[cfg(unix)]
struct FakeInstallation {
    _temp: tempfile::TempDir,
    definition: AgentDefinition,
    resolution: CandidateResolution,
    executable: PathBuf,
}

#[cfg(unix)]
impl FakeInstallation {
    fn new(definition: AgentDefinition, identity: &[u8]) -> Self {
        Self::with_streams(definition, identity, b"")
    }

    fn with_streams(
        definition: AgentDefinition,
        identity_stdout: &[u8],
        identity_stderr: &[u8],
    ) -> Self {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let name = direct_candidate_name(&definition);
        let executable = temp.path().join(name);
        write_executable(&executable, FAKE_PROBE.as_bytes());
        write_file(temp.path(), "identity.stdout", identity_stdout);
        write_file(temp.path(), "identity.stderr", identity_stderr);
        let snapshot = PathSnapshot::for_platform(
            jefe::runtime::AgentExecutablePlatform::current(),
            vec![temp.path().to_path_buf()],
            None,
        );
        let resolver = AgentCandidateResolver::new(&snapshot, temp.path().to_path_buf());
        let resolution = resolver.resolve(&definition);
        assert!(resolution.is_resolved(), "fake executable must resolve");
        Self {
            _temp: temp,
            definition,
            resolution,
            executable,
        }
    }

    fn marker(&self, name: &str) {
        write_file(
            self.executable.parent().value_or_panic("fake parent"),
            name,
            b"",
        );
    }

    fn invocations(&self) -> Vec<String> {
        let path = self.executable.with_file_name("invocations");
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn run(&self, generation: u64) -> AgentProbeResult {
        run_local_agent_probe(&self.definition, &self.resolution, generation)
    }

    fn resolved(&self) -> &ResolvedCandidate {
        self.resolution
            .resolved()
            .value_or_panic("fake candidate resolved")
    }
}

#[cfg(unix)]
trait TestOptionExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

#[cfg(unix)]
impl<T> TestOptionExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

#[cfg(unix)]
fn write_executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

#[cfg(unix)]
fn write_file(root: &Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

#[cfg(unix)]
fn direct_candidate_name(definition: &AgentDefinition) -> &str {
    definition
        .candidates
        .iter()
        .find_map(|candidate| candidate.kind.path_name())
        .value_or_panic("shipped definition needs a direct PATH candidate")
}

#[cfg(unix)]
fn shipped(id: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .value_or_panic("shipped definition")
}

#[cfg(unix)]
fn assert_probe_error(result: &AgentProbeResult, expected: &str) {
    match result.availability() {
        Availability::ProbeError { code, reason, .. } => {
            assert_eq!(*code, ProbeErrorCode::Agte202);
            assert!(reason.contains(expected), "reason {reason:?}");
        }
        other => panic!("expected AGT-E202 containing {expected:?}, got {other:?}"),
    }
}

#[cfg(unix)]
pub fn assert_exact_four_fixture_playback() {
    let definitions = AgentDefinition::shipped();
    let fixtures = retained_fixture_dirs();
    assert_eq!(fixtures.len(), 4);
    for (index, fixture) in fixtures.iter().enumerate() {
        assert_one_fixture(&definitions, fixture, (index + 1) as u64);
    }
}

#[cfg(not(unix))]
pub fn assert_exact_four_fixture_playback() {}

#[cfg(unix)]
fn assert_one_fixture(definitions: &[AgentDefinition], fixture: &Path, generation: u64) {
    let provenance = read_json(&fixture.join("provenance.json"));
    let type_id = provenance["agent_type_id"]
        .as_str()
        .value_or_panic("fixture type id");
    let definition = definitions
        .iter()
        .find(|candidate| candidate.id.as_str() == type_id)
        .value_or_panic("fixture definition")
        .clone();
    let install = FakeInstallation::with_streams(
        definition,
        &read_bytes(&fixture.join("probe.stdout")),
        &read_bytes(&fixture.join("probe.stderr")),
    );
    let result = install.run(generation);
    assert_compatible(&result, generation);
    assert_eq!(result.definition_sha256(), &install.definition.sha256());
    assert_eq!(
        result.executable_fingerprint(),
        Some(install.resolved().fingerprint())
    );
    // Identity is the only probe (#657).
    assert_eq!(install.invocations(), ["--version"]);
}

#[cfg(unix)]
fn assert_compatible(result: &AgentProbeResult, generation: u64) {
    match result.availability() {
        Availability::InstalledCompatible {
            identity,
            generation: actual,
        } => {
            assert!(!identity.is_empty());
            assert_eq!(*actual, generation);
        }
        other => panic!("expected compatible, got {other:?}"),
    }
}

#[test]
fn target_timeout_contract_is_typed() {
    assert_eq!(
        AgentProbeTarget::Local.total_timeout(),
        Duration::from_secs(10)
    );
    assert_eq!(
        AgentProbeTarget::Remote.total_timeout(),
        Duration::from_secs(20)
    );
}

#[cfg(unix)]
#[test]
fn not_found_spawns_zero_processes() {
    let definition = shipped("core.codex");
    let result = run_local_agent_probe(&definition, &CandidateResolution::NotFound(Vec::new()), 3);
    assert!(result.availability().is_not_found());
    assert_eq!(result.executable_fingerprint(), None);
}

#[cfg(unix)]
#[test]
fn drains_stdout_and_stderr_concurrently() {
    let mut stderr = Vec::new();
    for _ in 0..1_000 {
        stderr.extend_from_slice(b"diagnostic diagnostic diagnostic diagnostic diagnostic\n");
    }
    let install =
        FakeInstallation::with_streams(shipped("core.codex"), b"codex-cli 9.8.7\n", &stderr);
    let result = install.run(7);
    assert_compatible(&result, 7);
}

#[cfg(unix)]
#[test]
fn timeout_and_nonzero_exit_are_probe_errors() {
    let mut timeout_definition = shipped("core.codex");
    timeout_definition.probe.timeout_ms = 100;
    let timeout = FakeInstallation::new(timeout_definition, b"codex-cli 9.8.7\n");

    timeout.marker("identity.sleep");
    assert_probe_error(&timeout.run(8), "timed out");

    let nonzero = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n");
    nonzero.marker("identity.nonzero");
    assert_probe_error(&nonzero.run(9), "status 7");
}

#[cfg(windows)]
#[test]
fn sequential_probe_processes_each_receive_the_authored_timeout() {
    use jefe::agent_candidate::{AgentCandidateResolver, CandidateResolution};
    use jefe::agent_candidate_path::PathSnapshot;
    use jefe::domain::agent_definition::{AgentDefinition, Availability};
    use jefe::runtime::{AgentExecutablePlatform, run_local_agent_probe};

    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let executable = temp.path().join("codex.cmd");
    let script = concat!(
        "@echo off\r\n",
        "if \"%~1\"==\"--version\" (\r\n",
        "  ping.exe -n 3 127.0.0.1 >nul\r\n",
        "  echo codex-cli 9.8.7\r\n",
        "  exit /b 0\r\n",
        ")\r\n",
        "if \"%~1\"==\"--help\" (\r\n",
        "  ping.exe -n 3 127.0.0.1 >nul\r\n",
        "  echo --model resume --profile --sandbox --ask-for-approval --dangerously-bypass-approvals-and-sandbox --cd\r\n",
        "  exit /b 0\r\n",
        ")\r\n",
        "exit /b 64\r\n",
    );
    std::fs::write(&executable, script)
        .unwrap_or_else(|error| panic!("write {}: {error}", executable.display()));

    let mut definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == "core.codex")
        .unwrap_or_else(|| panic!("Codex definition must be shipped"));
    definition.probe.timeout_ms = 3_500;
    let snapshot = PathSnapshot::for_platform(
        AgentExecutablePlatform::current(),
        vec![temp.path().to_path_buf()],
        None,
    );
    let resolver = AgentCandidateResolver::new(&snapshot, temp.path().to_path_buf());
    let resolution: CandidateResolution = resolver.resolve(&definition);
    assert!(
        resolution.is_resolved(),
        "Windows command wrapper must resolve"
    );

    let result = run_local_agent_probe(&definition, &resolution, 18);
    assert!(
        matches!(
            result.availability(),
            Availability::InstalledCompatible { .. }
        ),
        "identity finishes within its own 3.5s deadline: {:?}",
        result.availability(),
    );
}

#[cfg(unix)]
#[test]
fn signal_exit_is_a_probe_error() {
    let signaled = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n");
    signaled.marker("identity.signal");
    assert_probe_error(&signaled.run(10), "signal");
}

#[cfg(unix)]
#[test]
fn truncation_invalid_utf8_and_overlong_line_are_probe_errors() {
    let truncated = FakeInstallation::new(shipped("core.codex"), &vec![b'x'; 65_537]);
    assert_probe_error(&truncated.run(10), "truncated");

    let invalid = FakeInstallation::new(shipped("core.codex"), &[0xff]);
    assert_probe_error(&invalid.run(11), "UTF-8");

    let overlong = FakeInstallation::new(shipped("core.codex"), &vec![b'x'; 4_097]);
    assert_probe_error(&overlong.run(12), "line");
}

#[cfg(unix)]
#[test]
fn malformed_framing_is_a_probe_error_and_identity_mismatch_is_incompatible() {
    let mut malformed_definition = shipped("core.codex");
    malformed_definition.probe.framing = ProbeFraming::SingleJson;
    malformed_definition.probe.identity = IdentityRecognizer::JsonPointer {
        pointer: "/version".to_string(),
        anchored_pattern: AnchoredPattern::VersionToken,
    };
    let malformed = FakeInstallation::new(
        malformed_definition,
        br#"{"version":"1.2.3","version":"2.3.4"}"#,
    );
    assert_probe_error(&malformed.run(13), "framing");

    let mismatch = FakeInstallation::new(shipped("core.codex"), b"different 1.2.3\n");
    match mismatch.run(14).availability() {
        Availability::InstalledIncompatible { reason, generation } => {
            assert_eq!(*generation, 14);
            assert!(reason.contains("identity mismatch"), "reason {reason:?}");
        }
        other => panic!("expected incompatible identity, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn single_json_and_json_lines_framing_parse_identity() {
    let single = FakeInstallation::new(
        json_definition(ProbeFraming::SingleJson),
        br#"{"version":"1.2.3"}"#,
    );
    assert_compatible(&single.run(20), 20);

    let lines = FakeInstallation::new(
        json_definition(ProbeFraming::JsonLines),
        b"{\"version\":\"bad\"}\n{\"version\":\"2.3.4\"}\n",
    );
    assert_compatible(&lines.run(21), 21);
}

#[cfg(unix)]
fn json_definition(framing: ProbeFraming) -> AgentDefinition {
    let mut definition = shipped("core.codex");
    definition.probe.framing = framing;
    definition.probe.identity = IdentityRecognizer::JsonPointer {
        pointer: "/version".to_string(),
        anchored_pattern: AnchoredPattern::VersionToken,
    };
    definition
}

#[cfg(unix)]
#[test]
fn fingerprint_change_is_stale_and_generations_are_preserved() {
    let install = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n");
    write_executable(
        &install.executable.with_file_name("replacement"),
        FAKE_PROBE.as_bytes(),
    );
    install.marker("identity.replace");
    match install.run(41).availability() {
        Availability::ProbeError {
            code, generation, ..
        } => {
            assert_eq!(*code, ProbeErrorCode::Agte203);
            assert_eq!(*generation, 41);
        }
        other => panic!("expected stale probe error, got {other:?}"),
    }

    let stable = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n");
    assert_eq!(stable.run(42).availability().generation(), Some(42));
    assert_eq!(stable.run(43).availability().generation(), Some(43));
}

#[cfg(unix)]
#[test]
fn stream_selection_is_deterministic() {
    let mut definition = shipped("core.codex");
    definition.probe.stream = ProbeStream::Stderr;
    let install = FakeInstallation::with_streams(definition, b"wrong\n", b"codex-cli 9.8.7\n");
    assert_compatible(&install.run(44), 44);
}

// Kept beside the other probe tests so the parent target stays within the
// 1000-line source-size limit.

/// Issue #657: the `--help` capability probe is deleted, so every shipped
/// definition probes with exactly one subprocess regardless of agent.
///
/// Help is never spawned, so a help stream that would previously have produced
/// `AGT-E202` cannot fail a launch that identity accepted.
#[cfg(unix)]
#[test]
fn every_shipped_agent_probes_with_exactly_one_subprocess() {
    for definition in AgentDefinition::shipped() {
        let id = definition.id.as_str().to_owned();
        let identity = identity_line_for(&id);
        // A help stream that the old gate would have rejected outright.
        let install = FakeInstallation::new(definition, identity.as_bytes());
        let result = install.run(1);
        assert!(
            matches!(
                result.availability(),
                jefe::domain::agent_definition::Availability::InstalledCompatible { .. }
            ),
            "{id} must be compatible from identity alone, got {:?}",
            result.availability()
        );
        assert_eq!(
            install.invocations(),
            ["--version"],
            "{id} must spawn only the identity probe"
        );
    }
}

/// Identity bytes each shipped definition's recognizer accepts.
#[cfg(unix)]
fn identity_line_for(type_id: &str) -> String {
    match type_id {
        "core.claude-code" => "2.1.220 (Claude Code)\n".to_owned(),
        "core.codex" => "codex-cli 0.146.0\n".to_owned(),
        _ => "0.0.634\n".to_owned(),
    }
}
