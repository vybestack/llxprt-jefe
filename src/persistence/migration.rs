//! One-way schema-1 to schema-2 state migration.
//!
//! Schema-1 DTOs are private to this module. Schema-2 input bypasses migration
//! and is returned semantically unchanged. This reader performs no writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

#[path = "migration_schema1.rs"]
mod schema1;
#[path = "migration_settings.rs"]
mod settings;
#[path = "migration_values.rs"]
mod values;

pub use settings::{SettingsMigration, format_migrated_settings, migrate_settings};

use schema1::{
    Schema1Agent, Schema1Preferences, Schema1RepoPreferences, Schema1Repository, Schema1State,
};
use values::{
    canonical_remote_target, digest_parts, json_map_to_typed, normalize_remote_path, stable_id,
    type_id, typed_map_hash,
};

use super::diagnostic::{CfgCode, Diagnostic, DiagnosticPath, FILE_LIMIT, PATH_LIMIT, Severity};
use super::paths::physical_identity;
use super::state_v2::StateDocument;
use crate::domain::{
    AgentDefaults, AgentRecord, DormantRecord, Id, LastKnownRuntime, LaunchSignatureV1,
    LocalRepositoryLocation, Preferences, RemoteRepositoryLocation, RepositoryLocation,
    RepositoryRecord, RuntimeRecord, Selection, StateV2, TypedMap,
};

const DEFINITION_VERSION: &str = "1";
const DORMANT_REASON: &str = "schema-1 owner or field is unavailable in schema 2";

/// Result of parsing current state or migrating one schema-1 candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMigration {
    state: StateV2,
    diagnostics: Vec<Diagnostic>,
    migrated: bool,
}

impl StateMigration {
    /// Borrow the current durable state.
    #[must_use]
    pub const fn state(&self) -> &StateV2 {
        &self.state
    }

    /// Borrow sorted repair warnings emitted during migration.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Report whether schema-1 bytes were transformed.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.migrated
    }

    /// Serialize the validated current state in canonical durable form.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut encoded = serde_json::to_vec_pretty(&self.state)?;
        encoded.push(b'\n');
        Ok(encoded)
    }
}

/// Parse strict schema 2 or migrate strict schema 1 entirely in memory.
pub fn migrate_state(bytes: &[u8]) -> Result<StateMigration, Vec<Diagnostic>> {
    if bytes.len() > FILE_LIMIT {
        return Err(vec![limit_error("/", bytes.len(), FILE_LIMIT)]);
    }
    if let Err(error) = super::state_json::reject_duplicate_keys(bytes) {
        return Err(vec![malformed(error.to_string())]);
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| vec![malformed(error.to_string())])?;
    super::state_v2::validate_json_bounds(&value, 1, "")?;
    match schema_number(&value, "state_schema") {
        Some(2) => current_state(bytes),
        Some(_) => Err(vec![malformed("unsupported state_schema")]),
        None => migrate_schema1_value(value),
    }
}

fn schema_number(value: &Value, key: &str) -> Option<u64> {
    value.as_object()?.get(key)?.as_u64()
}

fn current_state(bytes: &[u8]) -> Result<StateMigration, Vec<Diagnostic>> {
    let document = StateDocument::parse(bytes)?;
    Ok(StateMigration {
        state: document.state().clone(),
        diagnostics: Vec::new(),
        migrated: false,
    })
}

fn migrate_schema1_value(value: Value) -> Result<StateMigration, Vec<Diagnostic>> {
    let source: Schema1State =
        serde_json::from_value(value).map_err(|error| vec![malformed(error.to_string())])?;
    if source.schema_version != 1 {
        return Err(vec![malformed("unsupported schema_version")]);
    }
    let (state, mut diagnostics) = migrate_schema1(source)?;
    diagnostics.sort();
    let encoded = serde_json::to_vec(&state).map_err(|error| vec![malformed(error.to_string())])?;
    let document = StateDocument::parse(&encoded)?;
    Ok(StateMigration {
        state: document.state().clone(),
        diagnostics,
        migrated: true,
    })
}

struct MigratedRepository {
    source_id: String,
    identity: String,
    remote: bool,
    record: RepositoryRecord,
}

struct MigratedAgent {
    source_id: String,
    source_repository_id: String,
    record: AgentRecord,
}

fn migrate_schema1(source: Schema1State) -> Result<(StateV2, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut dormant_records = Vec::new();
    let repositories = migrate_repositories(source.repositories, &mut dormant_records)?;
    let agents = migrate_agents(source.agents, &repositories, &mut dormant_records)?;
    let mut diagnostics = Vec::new();
    let selection = migrate_selection(
        source.selected_repository_index,
        source.selected_agent_index,
        &repositories,
        &agents,
        &mut diagnostics,
    );
    let last_selected_agent_by_repo =
        migrate_last_selected(source.last_selected_agent_by_repo, &repositories, &agents)?;
    let repository_preferences =
        migrate_preferences(source.user_preferences, &repositories, &mut dormant_records)?;
    record_unknowns("schema1.root", None, source.unknown, &mut dormant_records)?;
    let state = StateV2 {
        state_schema: 2,
        revision: 1,
        repositories: repositories.into_iter().map(|item| item.record).collect(),
        agents: agents.into_iter().map(|item| item.record).collect(),
        selection,
        last_selected_agent_by_repo,
        preferences: Preferences {
            hide_idle_repositories: source.hide_idle_repositories,
            pane_focus: source.pane_focus,
            terminal_focused: source.terminal_focused,
            repository_preferences,
        },
        dormant_records,
    };
    Ok((state, diagnostics))
}

fn migrate_repositories(
    sources: Vec<Schema1Repository>,
    dormant: &mut Vec<DormantRecord>,
) -> Result<Vec<MigratedRepository>, Vec<Diagnostic>> {
    let mut collisions = BTreeMap::<String, u64>::new();
    let mut claimed_ids = BTreeSet::<Id>::new();
    let mut repositories = Vec::with_capacity(sources.len());
    for source in sources {
        let identity = repository_identity(&source)?;
        let ordinal = collisions.entry(identity.clone()).or_default();
        let ordinal_text = ordinal.to_string();
        // Preserve an already-valid id so references that key off it (agents,
        // remembered selections, per-repository preferences) stay intact. A
        // duplicate schema-1 id still has to be disambiguated, so only the
        // first claimant keeps it.
        let reusable = Id::parse(&source.id)
            .ok()
            .filter(|id| !claimed_ids.contains(id));
        let id = if let Some(id) = reusable {
            claimed_ids.insert(id.clone());
            id
        } else {
            stable_id("repo", &[&identity, &ordinal_text]).map_err(malformed_vec)?
        };
        *ordinal += 1;
        let values = repository_values(&source).map_err(malformed_vec)?;
        let type_id = type_id(source.default_agent_kind.as_deref()).map_err(malformed_vec)?;
        let location = repository_location(&source, &identity);
        record_unknowns("schema1.repository", Some(&id), source.unknown, dormant)?;
        record_unknowns(
            "schema1.repository.remote",
            Some(&id),
            source.remote.unknown,
            dormant,
        )?;
        let _ = source.agent_ids;
        repositories.push(MigratedRepository {
            source_id: source.id,
            identity,
            remote: source.remote.enabled,
            record: RepositoryRecord {
                id,
                location,
                display_name: source.name,
                agent_defaults: AgentDefaults { type_id, values },
            },
        });
    }
    ensure_unique_source_ids(&repositories)?;
    Ok(repositories)
}

fn expand_legacy_home(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    let windows_suffix = cfg!(windows).then(|| text.strip_prefix(r"~\")).flatten();
    let suffix = if text == "~" {
        ""
    } else {
        text.strip_prefix("~/").or(windows_suffix)?
    };
    // Do not reinterpret an MSYS-style HOME as a native Windows path.
    let mut home = host_home()?;
    if cfg!(windows) && home.to_string_lossy().starts_with('/') {
        return None;
    }
    home.push(suffix);
    Some(home)
}

fn host_home() -> Option<PathBuf> {
    cfg!(windows)
        .then(|| std::env::var_os("USERPROFILE").map(Into::into))
        .flatten()
        .or_else(|| std::env::var_os("HOME").map(Into::into))
}

fn repository_identity(source: &Schema1Repository) -> Result<String, Vec<Diagnostic>> {
    let base_dir = path_text(&source.base_dir)?;
    if source.remote.enabled {
        let run_as = if source.remote.run_as_user.trim().is_empty() {
            source.remote.login_user.trim()
        } else {
            source.remote.run_as_user.trim()
        };
        return Ok(canonical_remote_target(
            &source.remote.login_user,
            &source.remote.host,
            source.remote.port.unwrap_or(22),
            run_as,
            base_dir,
        ));
    }
    let expanded = expand_legacy_home(&source.base_dir);
    let effective = expanded.as_deref().unwrap_or(&source.base_dir);
    let identity = physical_identity(effective).map_err(|error| vec![*error.diagnostic])?;
    path_text(identity.canonical_path()).map(str::to_owned)
}

fn repository_location(source: &Schema1Repository, identity: &str) -> RepositoryLocation {
    if source.remote.enabled {
        RepositoryLocation::Remote(RemoteRepositoryLocation {
            remote_target: identity.to_owned(),
        })
    } else {
        RepositoryLocation::Local(LocalRepositoryLocation {
            local_path: identity.to_owned(),
        })
    }
}

fn repository_values(source: &Schema1Repository) -> Result<TypedMap, String> {
    let remote = json!({
        "enabled": source.remote.enabled,
        "login_user": source.remote.login_user,
        "host": source.remote.host,
        "port": source.remote.port,
        "identity_file": source.remote.identity_file,
        "options": source.remote.options,
        "run_as_user": source.remote.run_as_user,
        "setup_env_default": source.remote.setup_env_default,
    });
    json_map_to_typed(json!({
        "slug": source.slug,
        "default_profile": source.default_profile,
        "default_code_puppy_model": source.default_code_puppy_model,
        "default_code_puppy_version": source.default_code_puppy_version,
        "github_repo": source.github_repo,
        "github_issue_pr_repo": source.github_issue_pr_repo,
        "remote": remote,
        "issue_base_prompt": source.issue_base_prompt,
        "transient_agent_dir": source.transient_agent_dir,
        "default_code_puppy_yolo": source.default_code_puppy_yolo,
        "default_llxprt_mode_flags": source.default_llxprt_mode_flags,
        "transient_max_concurrent": source.transient_max_concurrent,
        "default_llxprt_version": source.default_llxprt_version,
    }))
}

fn migrate_agents(
    sources: Vec<Schema1Agent>,
    repositories: &[MigratedRepository],
    dormant: &mut Vec<DormantRecord>,
) -> Result<Vec<MigratedAgent>, Vec<Diagnostic>> {
    let mut collisions = BTreeMap::<(String, String), u64>::new();
    let mut claimed_ids = BTreeSet::<Id>::new();
    let mut agents = Vec::with_capacity(sources.len());
    for source in sources {
        let repository = resolve_repository(&source.repository_id, repositories)?;
        let (work_target, home_expanded) = agent_work_target(&source, repository)?;
        let source_identity = agent_source_identity(&source, &work_target);
        let key = (repository.record.id.to_string(), source_identity.clone());
        let ordinal = collisions.entry(key).or_default();
        let ordinal_text = ordinal.to_string();
        // A running agent's tmux session name is derived from its id, so an id
        // that is already a valid identifier must survive migration untouched;
        // minting a new one would orphan the live session and demote a healthy
        // agent to Dead on the next start. Schema-1 ids are not guaranteed
        // unique, so only the first claimant keeps its id and any later
        // duplicate falls back to a minted, collision-disambiguated id.
        let reusable = Id::parse(&source.id)
            .ok()
            .filter(|id| !claimed_ids.contains(id));
        let id = if let Some(id) = reusable {
            claimed_ids.insert(id.clone());
            id
        } else {
            stable_id(
                "agent",
                &[
                    repository.record.id.as_str(),
                    &source_identity,
                    &ordinal_text,
                ],
            )
            .map_err(malformed_vec)?
        };
        *ordinal += 1;
        let record =
            migrate_agent_record(&source, repository, &work_target, home_expanded, id.clone())?;
        record_unknowns("schema1.agent", Some(&id), source.unknown, dormant)?;
        agents.push(MigratedAgent {
            source_id: source.id,
            source_repository_id: source.repository_id,
            record,
        });
    }
    Ok(agents)
}

fn migrate_agent_record(
    source: &Schema1Agent,
    repository: &MigratedRepository,
    work_target: &str,
    home_expanded: bool,
    id: Id,
) -> Result<AgentRecord, Vec<Diagnostic>> {
    let type_id = type_id(source.agent_kind.as_deref()).map_err(malformed_vec)?;
    let values =
        agent_values(source, home_expanded.then_some(work_target)).map_err(malformed_vec)?;
    let definition_hash =
        digest_parts(&[type_id.as_str(), DEFINITION_VERSION]).map_err(malformed_vec)?;
    let typed_value_hash = typed_map_hash(&values).map_err(malformed_vec)?;
    let target_fingerprint =
        digest_parts(&[&repository.identity, work_target]).map_err(malformed_vec)?;
    let (session_id, invocation_generation) =
        source
            .runtime_binding
            .as_ref()
            .map_or((None, 0), |binding| {
                let _ = &binding.evidence;
                (
                    Some(binding.session_name.clone()),
                    binding.lifecycle_generation,
                )
            });
    let _ = &source.status;
    Ok(AgentRecord {
        id,
        repository_id: repository.record.id.clone(),
        type_id,
        values,
        launch_signature: LaunchSignatureV1 {
            version: 1,
            definition_hash,
            typed_value_hash,
            target_fingerprint,
        },
        runtime: RuntimeRecord {
            session_id,
            invocation_generation,
            last_known: last_known_runtime(source.status.as_ref()),
        },
    })
}

/// Carry the schema-1 lifecycle status across as last-known runtime.
///
/// Only launched states are meaningful: `Running` keeps the agent eligible for
/// startup reconciliation against live sessions, `Dead` records a session that
/// ended, and everything else was never launched.
fn last_known_runtime(status: Option<&Value>) -> LastKnownRuntime {
    match status.and_then(Value::as_str) {
        Some("Running") => LastKnownRuntime::Running,
        Some("Dead") => LastKnownRuntime::Stopped,
        _ => LastKnownRuntime::Unknown,
    }
}

fn agent_values(
    source: &Schema1Agent,
    work_dir_override: Option<&str>,
) -> Result<TypedMap, String> {
    let work_dir = work_dir_override.map_or(source.work_dir.as_path(), Path::new);
    json_map_to_typed(json!({
        "display_id": source.display_id,
        "shortcut_slot": source.shortcut_slot,
        "name": source.name,
        "description": source.description,
        "work_dir": work_dir,
        "profile": source.profile,
        "code_puppy_model": source.code_puppy_model,
        "code_puppy_version": source.code_puppy_version,
        "code_puppy_yolo": source.code_puppy_yolo,
        "code_puppy_quick_resume": source.code_puppy_quick_resume,
        "mode_flags": source.mode_flags,
        "llxprt_debug": source.llxprt_debug,
        "pass_continue": source.pass_continue,
        "sandbox_enabled": source.sandbox_enabled,
        "sandbox_engine": source.sandbox_engine,
        "sandbox_flags": source.sandbox_flags,
        "llxprt_version": source.llxprt_version,
        "origin": source.origin.as_deref().unwrap_or("persistent"),
    }))
}

fn agent_work_target(
    source: &Schema1Agent,
    repository: &MigratedRepository,
) -> Result<(String, bool), Vec<Diagnostic>> {
    let path = path_text(&source.work_dir)?;
    if repository.remote {
        return Ok((normalize_remote_path(path), false));
    }
    let expanded = expand_legacy_home(&source.work_dir);
    let effective = expanded.as_deref().unwrap_or(&source.work_dir);
    let identity = physical_identity(effective).map_err(|error| vec![*error.diagnostic])?;
    let target = path_text(identity.canonical_path()).map(str::to_owned)?;
    Ok((target, expanded.is_some()))
}

fn agent_source_identity(source: &Schema1Agent, work_target: &str) -> String {
    if source.id.is_empty() {
        format!("{}|{work_target}", source.name.trim().to_ascii_lowercase())
    } else {
        source.id.clone()
    }
}

fn migrate_selection(
    repository_index: Option<usize>,
    agent_index: Option<usize>,
    repositories: &[MigratedRepository],
    agents: &[MigratedAgent],
    diagnostics: &mut Vec<Diagnostic>,
) -> Selection {
    let repository_id = selected_id(
        repository_index,
        repositories.iter().map(|item| &item.record.id).collect(),
        "/selected_repository_index",
        diagnostics,
    );
    let mut agent_id = selected_id(
        agent_index,
        agents.iter().map(|item| &item.record.id).collect(),
        "/selected_agent_index",
        diagnostics,
    );
    if let (Some(repository_id), Some(selected_agent)) = (&repository_id, &agent_id)
        && agents.iter().any(|agent| {
            &agent.record.id == selected_agent && &agent.record.repository_id != repository_id
        })
    {
        diagnostics.push(repair_warning(
            "/selected_agent_index",
            "selected agent belongs to a different selected repository",
        ));
        agent_id = None;
    }
    Selection {
        repository_id,
        agent_id,
        screen_id: None,
    }
}

fn selected_id(
    index: Option<usize>,
    ids: Vec<&Id>,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Id> {
    let index = index?;
    if let Some(id) = ids.get(index) {
        Some((*id).clone())
    } else {
        diagnostics.push(repair_warning(
            path,
            "selected index is outside the source array",
        ));
        None
    }
}

fn migrate_last_selected(
    sources: Vec<(String, String)>,
    repositories: &[MigratedRepository],
    agents: &[MigratedAgent],
) -> Result<BTreeMap<Id, Id>, Vec<Diagnostic>> {
    let mut selected = BTreeMap::new();
    for (repository_id, agent_id) in sources {
        let repository = resolve_repository(&repository_id, repositories)?;
        let agent = resolve_agent(&repository_id, &agent_id, agents)?;
        if selected
            .insert(repository.record.id.clone(), agent.record.id.clone())
            .is_some()
        {
            return Err(vec![reference_error(
                "/last_selected_agent_by_repo",
                "duplicate remembered repository selection",
            )]);
        }
    }
    Ok(selected)
}

fn migrate_preferences(
    source: Schema1Preferences,
    repositories: &[MigratedRepository],
    dormant: &mut Vec<DormantRecord>,
) -> Result<BTreeMap<Id, TypedMap>, Vec<Diagnostic>> {
    let mut preferences = BTreeMap::new();
    for (source_id, values) in source.by_repo {
        let repository = resolve_repository(&source_id, repositories)?;
        let typed = preference_values(&values).map_err(malformed_vec)?;
        if preferences
            .insert(repository.record.id.clone(), typed)
            .is_some()
        {
            return Err(vec![reference_error(
                "/user_preferences/by_repo",
                "duplicate repository preference entry",
            )]);
        }
        record_preference_unknowns(&values, &repository.record.id, dormant)?;
    }
    record_unknowns("schema1.preferences", None, source.unknown, dormant)?;
    Ok(preferences)
}

fn preference_values(source: &Schema1RepoPreferences) -> Result<TypedMap, String> {
    let mut value = serde_json::to_value(source).map_err(|error| error.to_string())?;
    let Value::Object(root) = &mut value else {
        return Err("repository preferences did not serialize as an object".to_owned());
    };
    remove_unknown(root, &source.unknown);
    if let Some(Value::Object(filter)) = root.get_mut("issue_filter") {
        remove_unknown(filter, &source.issue_filter.unknown);
    }
    if let Some(Value::Object(filter)) = root.get_mut("pr_filter") {
        remove_unknown(filter, &source.pr_filter.unknown);
    }
    json_map_to_typed(value)
}

fn remove_unknown(values: &mut serde_json::Map<String, Value>, unknown: &BTreeMap<String, Value>) {
    for key in unknown.keys() {
        values.remove(key);
    }
}

fn record_preference_unknowns(
    source: &Schema1RepoPreferences,
    owner: &Id,
    dormant: &mut Vec<DormantRecord>,
) -> Result<(), Vec<Diagnostic>> {
    record_unknowns(
        "schema1.repository-preferences",
        Some(owner),
        source.unknown.clone(),
        dormant,
    )?;
    record_unknowns(
        "schema1.repository-preferences.issue-filter",
        Some(owner),
        source.issue_filter.unknown.clone(),
        dormant,
    )?;
    record_unknowns(
        "schema1.repository-preferences.pr-filter",
        Some(owner),
        source.pr_filter.unknown.clone(),
        dormant,
    )
}

fn record_unknowns(
    prefix: &str,
    owner: Option<&Id>,
    unknown: BTreeMap<String, Value>,
    dormant: &mut Vec<DormantRecord>,
) -> Result<(), Vec<Diagnostic>> {
    for (field, raw_value) in unknown {
        let kind = format!("{prefix}.{}", field.replace('_', "-"));
        let stable_id = if let Some(owner) = owner {
            Some(owner.clone())
        } else {
            let raw = serde_json::to_string(&raw_value)
                .map_err(|error| vec![malformed(error.to_string())])?;
            Some(stable_id("dormant", &[&kind, &raw]).map_err(malformed_vec)?)
        };
        dormant.push(DormantRecord {
            kind,
            stable_id,
            raw_schema: 1,
            reason: DORMANT_REASON.to_owned(),
            raw_value,
        });
    }
    Ok(())
}

fn resolve_repository<'a>(
    source_id: &str,
    repositories: &'a [MigratedRepository],
) -> Result<&'a MigratedRepository, Vec<Diagnostic>> {
    repositories
        .iter()
        .find(|item| item.source_id == source_id)
        .ok_or_else(|| {
            vec![reference_error(
                "/repositories",
                "schema-1 repository reference does not exist",
            )]
        })
}

fn resolve_agent<'a>(
    repository_id: &str,
    agent_id: &str,
    agents: &'a [MigratedAgent],
) -> Result<&'a MigratedAgent, Vec<Diagnostic>> {
    agents
        .iter()
        .find(|item| item.source_repository_id == repository_id && item.source_id == agent_id)
        .ok_or_else(|| {
            vec![reference_error(
                "/agents",
                "schema-1 agent reference does not exist in repository",
            )]
        })
}

fn ensure_unique_source_ids(repositories: &[MigratedRepository]) -> Result<(), Vec<Diagnostic>> {
    let mut seen = BTreeSet::new();
    for repository in repositories {
        if !seen.insert(&repository.source_id) {
            return Err(vec![reference_error(
                "/repositories",
                "duplicate schema-1 repository id",
            )]);
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, Vec<Diagnostic>> {
    let text = path
        .to_str()
        .ok_or_else(|| vec![malformed("schema-1 path is not Unicode")])?;
    if text.len() > PATH_LIMIT {
        return Err(vec![limit_error("/", text.len(), PATH_LIMIT)]);
    }
    Ok(text)
}

fn malformed_vec(detail: String) -> Vec<Diagnostic> {
    vec![malformed(detail)]
}

fn malformed(detail: impl Into<String>) -> Diagnostic {
    diagnostic(
        CfgCode::E103,
        Severity::Error,
        "/",
        "provide a supported schema-1 or schema-2 state document",
        detail,
    )
}

fn reference_error(path: &str, detail: impl Into<String>) -> Diagnostic {
    diagnostic(
        CfgCode::E006,
        Severity::Error,
        path,
        "reference a unique schema-1 repository or agent",
        detail,
    )
}

fn repair_warning(path: &str, detail: impl Into<String>) -> Diagnostic {
    diagnostic(
        CfgCode::W004,
        Severity::Warning,
        path,
        "review the repaired absent selection after migration",
        detail,
    )
}

fn limit_error(path: &str, actual: usize, limit: usize) -> Diagnostic {
    diagnostic(
        CfgCode::E008,
        Severity::Error,
        path,
        format!("reduce the value to at most {limit}"),
        format!("observed {actual}; inclusive limit {limit}"),
    )
}

fn diagnostic(
    code: CfgCode,
    severity: Severity,
    path: &str,
    correction: impl Into<String>,
    detail: impl Into<String>,
) -> Diagnostic {
    let mut diagnostic =
        Diagnostic::new(code, severity, DiagnosticPath::new(path), None, correction);
    diagnostic.redacted_detail = detail.into();
    diagnostic
}
