//! Typed send-to-agent chooser entry, Git display metadata, and pure label
//! projection (issue #230).
//!
//! The domain has two concerns kept strictly separate:
//!
//! - **Authoritative identity** (name, kind, runtime config): recomputed by
//!   deterministic reducers from `AppState` via pure selectors. Events and
//!   messages do NOT carry identity — only the reducer rebuilds entries from
//!   state so stale/injected/cross-repository identities can never reach the
//!   chooser.
//! - **Effect-derived Git display metadata** (branch, dirty): resolved at the
//!   `app_input` boundary (where git probing is permitted) and carried in
//!   [`AgentChooserGitMetadata`] keyed by [`AgentId`]. Reducers join only the
//!   metadata whose `AgentId` matches a currently eligible agent, so stale
//!   metadata from a removed agent is silently dropped.
//!
//! The label projection ([`agent_chooser_label`]) is the single source of
//! truth for the text rendered in the iocraft `AgentChooser` component and
//! the selection/clipboard overlay, so the two cannot drift.

use super::{AgentId, agent_definition::AgentTypeId};

/// Configured runtime profile or model for display in the chooser.
///
/// For LLxprt agents this is the profile name; for Code Puppy agents this is
/// the model name. When empty, the label projection shows an explicit
/// "runtime default" indicator instead of a blank value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChooserRuntimeConfig {
    /// The configured profile (LLxprt) or model (Code Puppy) string.
    /// Empty means the runtime's own default is in effect.
    pub value: String,
}

impl ChooserRuntimeConfig {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Whether the configured value is empty (runtime default).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.value.trim().is_empty()
    }
}

/// Dirty working-tree status for a chooser entry.
///
/// `None` means dirty status is unknown (e.g. remote repositories where
/// probing is skipped, or non-git directories). `Some(true)` means the
/// working tree has uncommitted changes; `Some(false)` means it is clean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirtyStatus(pub Option<bool>);

impl DirtyStatus {
    #[must_use]
    pub const fn unknown() -> Self {
        Self(None)
    }

    #[must_use]
    pub const fn dirty() -> Self {
        Self(Some(true))
    }

    #[must_use]
    pub const fn clean() -> Self {
        Self(Some(false))
    }

    /// Whether the working tree is known to be dirty.
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self.0, Some(true))
    }
}

/// Effect-derived Git display metadata for a chooser entry, keyed by
/// [`AgentId`].
///
/// Carries ONLY git display info (branch + dirty). Identity (name, kind,
/// config) is NOT included — the deterministic reducer rebuilds identity from
/// `AppState` so the chooser never trusts injected/stale identity.
///
/// `branch` is `None` when unknown (remote repos, non-git dirs). For detached
/// HEAD the caller provides a short-hash string like `"(abc1234)"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChooserGitMetadata {
    pub agent_id: AgentId,
    pub branch: Option<String>,
    pub dirty: DirtyStatus,
}

impl AgentChooserGitMetadata {
    /// Construct a metadata entry with unknown branch and dirty status.
    #[must_use]
    pub fn for_agent(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            branch: None,
            dirty: DirtyStatus::unknown(),
        }
    }
}

/// One selectable entry in the send-to-agent chooser.
///
/// Carries the agent's full identity resolved by the reducer from `AppState`
/// (`agent_id`, `name`, `kind`, `runtime_config`) plus effect-derived Git
/// display metadata (`branch`, `dirty`) joined from
/// [`AgentChooserGitMetadata`].
///
/// The pure label projection ([`agent_chooser_label`]) derives the display
/// text from these fields so rendering and clipboard text share one source of
/// truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChooserEntry {
    pub agent_id: AgentId,
    pub name: String,
    pub type_id: AgentTypeId,
    pub type_display_name: String,
    pub runtime_config_name: String,
    pub runtime_config: ChooserRuntimeConfig,
    pub branch: Option<String>,
    pub dirty: DirtyStatus,
}

impl AgentChooserEntry {
    #[must_use]
    pub fn new(
        agent_id: AgentId,
        name: String,
        type_id: AgentTypeId,
        type_display_name: String,
        runtime_config_name: String,
        runtime_config: ChooserRuntimeConfig,
    ) -> Self {
        Self {
            agent_id,
            name,
            type_id,
            type_display_name,
            runtime_config_name,
            runtime_config,
            branch: None,
            dirty: DirtyStatus::unknown(),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn simple(id: &str, name: &str) -> Self {
        Self::new(
            AgentId(id.to_string()),
            name.to_string(),
            super::shipped_agent_type(3),
            "LLxprt".to_owned(),
            "profile".to_owned(),
            ChooserRuntimeConfig::default(),
        )
    }
}

/// The explicit text shown when a profile or model is empty, indicating the
/// runtime's own default is in effect rather than a blank/unset value.
const DEFAULT_RUNTIME_CONFIG_LABEL: &str = "(default)";

/// Build the display label for a chooser entry.
///
/// Format: `{name} [{kind_label}] {config_label}{branch_suffix}`
///
/// - `kind_label`: `"LLxprt"` or `"Code Puppy"`.
/// - `config_label`: `"profile: {value}"` for LLxprt, `"model: {value}"` for
///   Code Puppy. When the value is empty, `{DEFAULT_RUNTIME_CONFIG_LABEL}` is shown.
/// - `branch_suffix`: `"  @ {branch}"` when a branch is known, with `" *"`
///   appended when the working tree is dirty. Clean trees omit `*`; unknown
///   branch (remote/non-git) omits the suffix entirely; detached HEAD uses
///   the short-hash string the caller provides (e.g. `"(abc1234)"`).
///
/// This is the single source of truth consumed by both the iocraft
/// `AgentChooser` component and the selection/clipboard overlay projection.
#[must_use]
pub fn agent_chooser_label(entry: &AgentChooserEntry) -> String {
    let config_label = chooser_config_label(&entry.runtime_config_name, &entry.runtime_config);
    let branch_suffix = branch_suffix(entry.branch.as_deref(), entry.dirty);
    format!(
        "{} [{}] {}{}",
        entry.name, entry.type_display_name, config_label, branch_suffix
    )
}

#[must_use]
fn chooser_config_label(prefix: &str, config: &ChooserRuntimeConfig) -> String {
    let display_value = if config.is_default() {
        DEFAULT_RUNTIME_CONFIG_LABEL
    } else {
        config.value.trim()
    };
    format!("{prefix}: {display_value}")
}

/// Build the compact branch suffix for the label.
///
/// - `Some(branch)` + dirty: `"  @ {branch} *"`
/// - `Some(branch)` + clean/unknown: `"  @ {branch}"`
/// - `None` (remote/non-git/unknown): `""` (no suffix)
///
/// The dirty `*` is placed adjacent to the branch name, not at the end of
/// the whole label, so the user sees the branch and its dirty state together.
#[must_use]
fn branch_suffix(branch: Option<&str>, dirty: DirtyStatus) -> String {
    match branch {
        Some(b) if !b.is_empty() => {
            if dirty.is_dirty() {
                format!("  @ {b} *")
            } else {
                format!("  @ {b}")
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llxprt_entry(name: &str, profile: &str) -> AgentChooserEntry {
        AgentChooserEntry {
            agent_id: AgentId("a1".to_string()),
            name: name.to_string(),
            type_id: super::super::shipped_agent_type(3),
            type_display_name: "LLxprt".to_owned(),
            runtime_config_name: "profile".to_owned(),
            runtime_config: ChooserRuntimeConfig::new(profile),
            branch: None,
            dirty: DirtyStatus::unknown(),
        }
    }

    fn puppy_entry(name: &str, model: &str) -> AgentChooserEntry {
        let definition = super::super::agent_definition::AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.display_name == "Code Puppy")
            .unwrap_or_else(|| panic!("shipped Code Puppy definition"));
        AgentChooserEntry {
            agent_id: AgentId("a2".to_string()),
            name: name.to_string(),
            type_id: definition.id,
            type_display_name: definition.display_name,
            runtime_config_name: "model".to_owned(),
            runtime_config: ChooserRuntimeConfig::new(model),
            branch: None,
            dirty: DirtyStatus::unknown(),
        }
    }

    #[test]
    fn llxprt_entry_shows_kind_and_profile() {
        let entry = llxprt_entry("alpha", "ops");
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops");
    }

    #[test]
    fn puppy_entry_shows_kind_and_model() {
        let entry = puppy_entry("beta", "gpt-5.6-sol");
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "beta [Code Puppy] model: gpt-5.6-sol");
    }

    #[test]
    fn llxprt_empty_profile_shows_default() {
        let entry = llxprt_entry("alpha", "");
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: (default)");
    }

    #[test]
    fn puppy_empty_model_shows_default() {
        let entry = puppy_entry("beta", "");
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "beta [Code Puppy] model: (default)");
    }

    #[test]
    fn llxprt_whitespace_profile_shows_default() {
        let entry = llxprt_entry("alpha", "   ");
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: (default)");
    }

    // ── Branch suffix tests (Finding 2) ────────────────────────────────────

    #[test]
    fn branch_suffix_local_clean() {
        let mut entry = llxprt_entry("alpha", "ops");
        entry.branch = Some("main".to_string());
        entry.dirty = DirtyStatus::clean();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops  @ main");
    }

    #[test]
    fn branch_suffix_local_dirty() {
        let mut entry = llxprt_entry("alpha", "ops");
        entry.branch = Some("main".to_string());
        entry.dirty = DirtyStatus::dirty();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops  @ main *");
    }

    #[test]
    fn branch_suffix_detached_uses_short_hash() {
        let mut entry = llxprt_entry("alpha", "ops");
        entry.branch = Some("(abc1234)".to_string());
        entry.dirty = DirtyStatus::clean();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops  @ (abc1234)");
    }

    #[test]
    fn branch_suffix_detached_dirty() {
        let mut entry = puppy_entry("beta", "minimax-m3");
        entry.branch = Some("(abc1234)".to_string());
        entry.dirty = DirtyStatus::dirty();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "beta [Code Puppy] model: minimax-m3  @ (abc1234) *");
    }

    #[test]
    fn branch_suffix_remote_unknown_omits_suffix() {
        let entry = llxprt_entry("alpha", "ops");
        // branch is None (remote/non-git) → no suffix
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops");
    }

    #[test]
    fn branch_suffix_dirty_without_branch_omits_marker() {
        let mut entry = llxprt_entry("alpha", "ops");
        entry.dirty = DirtyStatus::dirty();
        // No branch → dirty marker must not appear
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops");
    }

    #[test]
    fn branch_suffix_unknown_dirty_shows_branch_no_star() {
        let mut entry = llxprt_entry("alpha", "ops");
        entry.branch = Some("main".to_string());
        entry.dirty = DirtyStatus::unknown();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "alpha [LLxprt] profile: ops  @ main");
    }

    #[test]
    fn dirty_puppy_with_branch_shows_adjacent_marker() {
        let mut entry = puppy_entry("beta", "minimax-m3");
        entry.branch = Some("feature".to_string());
        entry.dirty = DirtyStatus::dirty();
        let label = agent_chooser_label(&entry);
        assert_eq!(label, "beta [Code Puppy] model: minimax-m3  @ feature *");
    }

    // ── AgentChooserGitMetadata default ─────────────────────────────────────

    #[test]
    fn git_metadata_for_agent_is_empty() {
        let md = AgentChooserGitMetadata::for_agent(AgentId("a1".to_string()));
        assert_eq!(md.agent_id, AgentId("a1".to_string()));
        assert_eq!(md.branch, None);
        assert_eq!(md.dirty, DirtyStatus::unknown());
    }
}
