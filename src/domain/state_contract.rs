//! Durable schema-2 state data shared by pure reducers and persistence.
//!
//! These DTOs contain no file-system or adapter behavior. The persistence
//! boundary owns strict JSON parsing, validation, and serialization.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::{Id, PaneProcessIdentity, TypedMap, WorkerProcessIdentity};

/// Current durable state schema version.
pub const STATE_SCHEMA_V2: u64 = 2;

/// Complete durable schema-2 application state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateV2 {
    pub state_schema: u64,
    pub revision: u64,
    pub repositories: Vec<RepositoryRecord>,
    pub agents: Vec<AgentRecord>,
    pub selection: Selection,
    pub last_selected_agent_by_repo: BTreeMap<Id, Id>,
    pub preferences: Preferences,
    pub dormant_records: Vec<DormantRecord>,
}

/// Durable repository definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryRecord {
    pub id: Id,
    pub location: RepositoryLocation,
    pub display_name: String,
    pub agent_defaults: AgentDefaults,
}

/// Exactly one local or remote repository location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryLocation {
    Local(LocalRepositoryLocation),
    Remote(RemoteRepositoryLocation),
}

/// Local repository path location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRepositoryLocation {
    pub local_path: String,
}

/// Canonical remote repository target location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteRepositoryLocation {
    pub remote_target: String,
}

/// Repository-level default agent values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefaults {
    pub type_id: Id,
    pub values: TypedMap,
}

/// Durable agent definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRecord {
    pub id: Id,
    pub repository_id: Id,
    pub type_id: Id,
    pub values: TypedMap,
    pub launch_signature: LaunchSignatureV1,
    pub runtime: RuntimeRecord,
}

/// Version-one launch signature over definition, values, and target identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSignatureV1 {
    pub version: u64,
    pub definition_hash: Sha256Digest,
    pub typed_value_hash: Sha256Digest,
    pub target_fingerprint: Sha256Digest,
}

impl LaunchSignatureV1 {
    /// Current launch-signature schema version.
    pub const VERSION: u64 = 1;

    /// Construct the canonical version-one signature from definition digests.
    #[must_use]
    pub fn v1(
        definition_hash: super::agent_definition::DefinitionSha256,
        typed_value_hash: super::agent_definition::DefinitionSha256,
        target_fingerprint: super::agent_definition::DefinitionSha256,
    ) -> Self {
        Self {
            version: Self::VERSION,
            definition_hash: Sha256Digest::from_definition(definition_hash),
            typed_value_hash: Sha256Digest::from_definition(typed_value_hash),
            target_fingerprint: Sha256Digest::from_definition(target_fingerprint),
        }
    }
}

impl Default for LaunchSignatureV1 {
    fn default() -> Self {
        Self {
            version: 0,
            definition_hash: Sha256Digest::zero(),
            typed_value_hash: Sha256Digest::zero(),
            target_fingerprint: Sha256Digest::zero(),
        }
    }
}

/// Canonical lowercase SHA-256 digest used by durable contracts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl<'de> Deserialize<'de> for Sha256Digest {
    /// Decode through [`Sha256Digest::parse`] so reading a document applies the
    /// same rule as constructing one: a transparent derive would accept any
    /// string and let a malformed digest into a launch signature.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl Sha256Digest {
    fn zero() -> Self {
        Self("0".repeat(64))
    }

    pub(crate) fn from_definition(value: super::agent_definition::DefinitionSha256) -> Self {
        Self(value.to_hex())
    }

    /// Parse exactly 64 lowercase hexadecimal characters.
    pub fn parse(value: &str) -> Result<Self, StateContractError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(StateContractError::InvalidSha256);
        }
        Ok(Self(value.to_owned()))
    }

    /// Borrow the canonical hexadecimal text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the canonical lowercase hexadecimal text.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.0.clone()
    }
}

/// Durable runtime observation; liveness must be reconciled after startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRecord {
    pub session_id: Option<String>,
    pub invocation_generation: u64,
    pub last_known: LastKnownRuntime,
    /// The pane leader observed for this session (issue #543).
    ///
    /// Defaulted so documents written before the roles were separated still
    /// load; absent means "not recorded", never "same as the worker".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_identity: Option<PaneProcessIdentity>,
    /// The agent worker observed for this session (issue #543).
    ///
    /// Recorded separately from the pane leader because on platforms where the
    /// pane runs a session host they are different processes, and a restore
    /// that inferred one from the other would reintroduce the conflation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_identity: Option<WorkerProcessIdentity>,
    /// Validated worker descendants observed for this session (issue #642).
    ///
    /// These are the anchors the orphan reaper matches against. They must be
    /// durable: after a restart the reaper has no other way to tell a
    /// dead-launcher orphan tree from an ordinary stopped agent, and an empty
    /// set makes `orphan_evidence` return `NoOrphan` before it observes
    /// anything. Omitted when empty so documents written before #642 stay
    /// byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worker_identities: Vec<WorkerProcessIdentity>,
}

/// Last persisted runtime observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastKnownRuntime {
    Stopped,
    Running,
    Unknown,
}

/// Stable selected entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub repository_id: Option<Id>,
    pub agent_id: Option<Id>,
    pub screen_id: Option<Id>,
}

/// Durable global and repository-scoped preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub hide_idle_repositories: bool,
    pub pane_focus: String,
    pub terminal_focused: bool,
    pub repository_preferences: BTreeMap<Id, TypedMap>,
}

/// Unavailable data retained without substituting a runtime owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DormantRecord {
    pub kind: String,
    pub stable_id: Option<Id>,
    pub raw_schema: u64,
    pub reason: String,
    pub raw_value: JsonValue,
}

/// Error returned by durable state value constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateContractError {
    InvalidSha256,
}

impl std::fmt::Display for StateContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SHA-256 must be exactly 64 lowercase hexadecimal characters")
    }
}

impl std::error::Error for StateContractError {}
