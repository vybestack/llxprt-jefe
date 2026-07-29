//! Fixture byte and scenario path helpers for issue #382 (CW-02) RED tests.
//!
//! This module is the single source of truth for the captured four-agent
//! release evidence and the seventeen scenario ledger. Each helper asserts
//! exact captured fixture bytes so the RED tests prove the recorded
//! provenance exists; the behavioral assertions live in the test bodies.
//!
//! Fixture release provenance (issue #382 fixture-authoring gate): the bytes
//! under `tests/fixtures/agent-definitions/` are deterministic evidence of a
//! real release; they are not a runtime version allow-list.

use std::path::PathBuf;

/// Repository-relative fixture root for the four captured agent releases.
const FIXTURE_ROOT: &str = "tests/fixtures/agent-definitions";

/// Repository-relative scenario root for the seventeen CW-02 scenarios.
pub const SCENARIO_ROOT: &str = "dev-docs/tmux-scenarios/issue382";

/// One captured agent release: identity, version, probe, and mapping evidence.
#[derive(Clone, Copy)]
pub struct AgentFixture {
    pub type_id: &'static str,
    pub display_name: &'static str,
    pub dir: &'static str,
    pub release: &'static str,
    /// The recognizable identity token the probe parser extracts from the raw
    /// captured stream. Some raw streams (e.g. code-puppy) interleave terminal
    /// palette control sequences; the token is what survives parsing.
    pub identity_token: &'static str,
}

/// The four shipped agent definitions with their fixture-release evidence.
pub const AGENTS: &[AgentFixture] = &[
    AgentFixture {
        type_id: "core.llxprt",
        display_name: "LLxprt",
        dir: "llxprt",
        release: "0.10.0-nightly.260720.d69bda66a",
        identity_token: "0.10.0-nightly.260720.d69bda66a",
    },
    AgentFixture {
        type_id: "core.codex",
        display_name: "Codex CLI",
        dir: "codex",
        release: "0.142.0",
        identity_token: "codex-cli 0.142.0",
    },
    AgentFixture {
        type_id: "core.claude-code",
        display_name: "Claude Code",
        dir: "claude",
        release: "2.1.212",
        identity_token: "2.1.212 (Claude Code)",
    },
    AgentFixture {
        type_id: "core.code-puppy",
        display_name: "Code Puppy",
        dir: "code-puppy",
        release: "0.0.634",
        identity_token: "0.0.634",
    },
];

/// Absolute path to a repository-relative resource under `CARGO_MANIFEST_DIR`.
#[must_use]
pub fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Absolute path to a scenario JSON by basename.
#[must_use]
pub fn scenario_path(name: &str) -> PathBuf {
    repo_path(format!("{SCENARIO_ROOT}/{name}").as_str())
}

/// Absolute path to a captured fixture file for an agent.
#[must_use]
pub fn fixture_file(agent: &AgentFixture, leaf: &str) -> PathBuf {
    repo_path(
        format!(
            "{FIXTURE_ROOT}/{}/{release}/{leaf}",
            agent.dir,
            release = agent.release
        )
        .as_str(),
    )
}

/// Read a fixture file as bytes; panics with the path on failure so the RED
/// run reports the exact missing evidence artifact.
pub fn read_fixture(agent: &AgentFixture, leaf: &str) -> Vec<u8> {
    let path = fixture_file(agent, leaf);
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Read a scenario JSON by basename.
pub fn read_scenario(name: &str) -> Vec<u8> {
    let path = scenario_path(name);
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Assert the probe stream for an agent reproduces the recorded identity
/// token. The raw bytes are the exact captured fixture output; the identity
/// token is what the probe parser recognizes (some streams interleave
/// terminal control sequences that the parser strips).
pub fn assert_probe_identity(agent: &AgentFixture) {
    let probe = read_fixture(agent, "probe.stdout");
    let probe_text = String::from_utf8_lossy(&probe);
    assert!(
        probe_text.contains(agent.identity_token),
        "{} probe.stdout must contain the recorded identity token {:?}; raw bytes were {} bytes",
        agent.type_id,
        agent.identity_token,
        probe.len()
    );
    let version = read_fixture(agent, "version.stdout");
    let version_text = String::from_utf8_lossy(&version);
    assert!(
        version_text.contains(agent.identity_token),
        "{} version.stdout must contain the recorded identity token {:?}; raw bytes were {} bytes",
        agent.type_id,
        agent.identity_token,
        version.len()
    );
}

/// Assert provenance records the fixture-authoring release SHA-256 and source.
pub fn assert_provenance(agent: &AgentFixture) {
    let bytes = read_fixture(agent, "provenance.json");
    let text = String::from_utf8(bytes).unwrap_or_else(|err| panic!("provenance utf8: {err}"));
    assert!(
        text.contains("\"agent_type_id\""),
        "{} provenance must declare the typed id",
        agent.type_id
    );
    assert!(
        text.contains(agent.type_id),
        "{} provenance must record {}",
        agent.type_id,
        agent.type_id
    );
    assert!(
        text.contains("\"executable_sha256\""),
        "{} provenance must record the release SHA-256",
        agent.type_id
    );
    assert!(
        text.contains("\"verified_mappings\""),
        "{} provenance must record the fixture-verified argv mappings",
        agent.type_id
    );
}
