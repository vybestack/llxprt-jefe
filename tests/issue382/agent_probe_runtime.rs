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
    AnchoredPattern, CapabilityToken, IdentityRecognizer, ProbeFraming, ProbeStream,
    evaluate_capabilities,
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
if [ "$1" = "--help" ]; then
    cat "$dir/help.stdout"
    cat "$dir/help.stderr" >&2
    if [ -f "$dir/help.nonzero" ]; then exit 9; fi
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
    fn new(definition: AgentDefinition, identity: &[u8], help: &[u8]) -> Self {
        Self::with_streams(definition, identity, b"", help, b"")
    }

    fn with_streams(
        definition: AgentDefinition,
        identity_stdout: &[u8],
        identity_stderr: &[u8],
        help_stdout: &[u8],
        help_stderr: &[u8],
    ) -> Self {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let name = direct_candidate_name(&definition);
        let executable = temp.path().join(name);
        write_executable(&executable, FAKE_PROBE.as_bytes());
        write_file(temp.path(), "identity.stdout", identity_stdout);
        write_file(temp.path(), "identity.stderr", identity_stderr);
        write_file(temp.path(), "help.stdout", help_stdout);
        write_file(temp.path(), "help.stderr", help_stderr);
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
        &read_bytes(&fixture.join("help.stdout")),
        &read_bytes(&fixture.join("help.stderr")),
    );
    let expected = expected_capabilities(&install.definition, fixture);
    let result = install.run(generation);
    assert_compatible(&result, generation, &expected);
    assert_eq!(result.definition_sha256(), &install.definition.sha256());
    assert_eq!(
        result.executable_fingerprint(),
        Some(install.resolved().fingerprint())
    );
    assert_eq!(install.invocations(), ["--version", "--help"]);
}

#[cfg(unix)]
fn expected_capabilities(definition: &AgentDefinition, fixture: &Path) -> Vec<String> {
    let probe = definition
        .probe
        .capabilities
        .as_ref()
        .value_or_panic("capability probe");
    let bytes = read_bytes(&fixture.join("help.stdout"));
    let help = std::str::from_utf8(&bytes).unwrap_or_else(|error| panic!("help utf8: {error}"));
    evaluate_capabilities(help, probe, &definition.probe.required).present
}

#[cfg(unix)]
fn assert_compatible(result: &AgentProbeResult, generation: u64, expected: &[String]) {
    match result.availability() {
        Availability::InstalledCompatible {
            identity,
            capabilities,
            generation: actual,
        } => {
            assert!(!identity.is_empty());
            assert_eq!(capabilities, expected);
            assert_eq!(*actual, generation);
        }
        other => panic!("expected compatible, got {other:?}"),
    }
}

#[test]
fn target_timeout_contract_is_typed() {
    assert_eq!(
        AgentProbeTarget::Local.total_timeout(),
        Duration::from_secs(5)
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
    let install = FakeInstallation::with_streams(
        shipped("core.codex"),
        b"codex-cli 9.8.7\n",
        &stderr,
        b"--model resume --profile --sandbox --ask-for-approval --dangerously-bypass-approvals-and-sandbox --cd\n",
        b"",
    );
    let result = install.run(7);
    assert_compatible(&result, 7, &expected_from_help(&install));
}

#[cfg(unix)]
fn expected_from_help(install: &FakeInstallation) -> Vec<String> {
    let probe = install
        .definition
        .probe
        .capabilities
        .as_ref()
        .value_or_panic("capability probe");
    let bytes = fs::read(install.executable.with_file_name("help.stdout"))
        .unwrap_or_else(|error| panic!("read help: {error}"));
    let text = std::str::from_utf8(&bytes).unwrap_or_else(|error| panic!("utf8: {error}"));
    evaluate_capabilities(text, probe, &install.definition.probe.required).present
}

#[cfg(unix)]
#[test]
fn timeout_and_nonzero_exit_are_probe_errors() {
    let mut timeout_definition = shipped("core.codex");
    timeout_definition.probe.timeout_ms = 100;
    let timeout = FakeInstallation::new(timeout_definition, b"codex-cli 9.8.7\n", b"");

    timeout.marker("identity.sleep");
    assert_probe_error(&timeout.run(8), "timed out");

    let nonzero = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n", b"");
    nonzero.marker("identity.nonzero");
    assert_probe_error(&nonzero.run(9), "status 7");
}

#[cfg(unix)]
#[test]
fn signal_exit_is_a_probe_error() {
    let signaled = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n", b"");
    signaled.marker("identity.signal");
    assert_probe_error(&signaled.run(10), "signal");
}
#[cfg(unix)]
#[test]
fn truncation_invalid_utf8_and_overlong_line_are_probe_errors() {
    let truncated = FakeInstallation::new(shipped("core.codex"), &vec![b'x'; 65_537], b"");
    assert_probe_error(&truncated.run(10), "truncated");

    let invalid = FakeInstallation::new(shipped("core.codex"), &[0xff], b"");
    assert_probe_error(&invalid.run(11), "UTF-8");

    let overlong = FakeInstallation::new(shipped("core.codex"), &vec![b'x'; 4_097], b"");
    assert_probe_error(&overlong.run(12), "line");
}

#[cfg(unix)]
#[test]
fn malformed_framing_and_identity_mismatch_are_probe_errors() {
    let mut malformed_definition = shipped("core.codex");
    malformed_definition.probe.framing = ProbeFraming::SingleJson;
    malformed_definition.probe.identity = IdentityRecognizer::JsonPointer {
        pointer: "/version".to_string(),
        anchored_pattern: AnchoredPattern::VersionToken,
    };
    let malformed = FakeInstallation::new(
        malformed_definition,
        br#"{"version":"1.2.3","version":"2.3.4"}"#,
        b"",
    );
    assert_probe_error(&malformed.run(13), "framing");

    let mismatch = FakeInstallation::new(shipped("core.codex"), b"different 1.2.3\n", b"");
    assert_probe_error(&mismatch.run(14), "identity");
}

#[cfg(unix)]
#[test]
fn single_json_and_json_lines_framing_parse_identity() {
    let single = FakeInstallation::new(
        json_definition(ProbeFraming::SingleJson),
        br#"{"version":"1.2.3"}"#,
        b"",
    );
    assert_compatible(&single.run(20), 20, &[]);

    let lines = FakeInstallation::new(
        json_definition(ProbeFraming::JsonLines),
        b"{\"version\":\"bad\"}\n{\"version\":\"2.3.4\"}\n",
        b"",
    );
    assert_compatible(&lines.run(21), 21, &[]);
}

#[cfg(unix)]
fn json_definition(framing: ProbeFraming) -> AgentDefinition {
    let mut definition = shipped("core.codex");
    definition.probe.framing = framing;
    definition.probe.identity = IdentityRecognizer::JsonPointer {
        pointer: "/version".to_string(),
        anchored_pattern: AnchoredPattern::VersionToken,
    };
    definition.probe.capabilities = None;
    definition.probe.required.clear();
    definition
}
#[cfg(unix)]
#[test]
fn required_missing_is_exact_and_optional_missing_is_compatible() {
    let required = FakeInstallation::new(
        shipped("core.code-puppy"),
        b"9.8.7\n",
        b"--model --resume --quick-resume --yolo\n",
    );
    match required.run(15).availability() {
        Availability::InstalledIncompatible { reason, generation } => {
            assert_eq!(reason, "missing required capability: interactive");
            assert_eq!(*generation, 15);
        }
        other => panic!("expected incompatible, got {other:?}"),
    }

    let mut optional_definition = shipped("core.codex");
    let probe = optional_definition
        .probe
        .capabilities
        .as_mut()
        .value_or_panic("capability probe");
    probe.tokens.push(CapabilityToken {
        id: "optional-unknown".to_string(),
        token: "--future-optional".to_string(),
    });
    let optional = FakeInstallation::new(
        optional_definition,
        b"codex-cli 9.8.7\n",
        b"--model resume --profile --sandbox --ask-for-approval --dangerously-bypass-approvals-and-sandbox --cd --not-authored\n",
    );
    let result = optional.run(16);
    assert_compatible(&result, 16, &expected_from_help(&optional));
}

#[cfg(unix)]
#[test]
fn fingerprint_change_is_stale_and_generations_are_preserved() {
    let install = FakeInstallation::new(shipped("core.codex"), b"codex-cli 9.8.7\n", b"");
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

    let stable = FakeInstallation::new(
        shipped("core.codex"),
        b"codex-cli 9.8.7\n",
        b"--model resume --profile --sandbox --ask-for-approval --dangerously-bypass-approvals-and-sandbox --cd\n",
    );
    assert_eq!(stable.run(42).availability().generation(), Some(42));
    assert_eq!(stable.run(43).availability().generation(), Some(43));
}

#[cfg(unix)]
#[test]
fn stream_selection_is_deterministic() {
    let mut definition = shipped("core.codex");
    definition.probe.stream = ProbeStream::Stderr;
    let install = FakeInstallation::with_streams(
        definition,
        b"wrong\n",
        b"codex-cli 9.8.7\n",
        b"--model resume --profile --sandbox --ask-for-approval --dangerously-bypass-approvals-and-sandbox --cd\n",
        b"",
    );
    assert_compatible(&install.run(44), 44, &expected_from_help(&install));
}
