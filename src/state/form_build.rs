//! Repository and agent form-to-domain construction with validation.
//!
//! Extracted from `form_ops.rs` to keep that file under the architecture line
//! limit. These are the pure (or near-pure) constructors that turn form field
//! structs into validated domain objects. The submission glue in `form_ops`
//! calls these and then pushes the result into `AppState` collections.
//!
//! Validation policy:
//!
//! - `github_repo` must be `"owner/repo"` or empty.
//! - An **enabled** remote must have nonempty `login_user` and `host`;
//!   otherwise the submission is rejected via [`crate::domain::target`].

use crate::domain::{
    Agent, AgentKind, PlatformCapabilities, RemoteRepositorySettings, Repository, RepositoryId,
    SandboxEngine, is_valid_github_component,
};
use tracing::warn;

use crate::services::{
    self, CreateAgentParams, expand_tilde, generate_id, normalize_llxprt_debug, normalize_profile,
    normalize_sandbox_flags, resolve_agent_work_dir,
};

use super::AppState;
use super::form_runtime;
use super::types::{AgentFormFields, RepositoryFormFields};

impl AppState {
    /// Validate a `github_repo` field value.
    ///
    /// An empty value is valid (no GitHub integration). A non-empty value must
    /// be exactly `"owner/repo"`: a single forward slash with non-empty parts on
    /// both sides, each containing only valid GitHub name characters
    /// (alphanumerics, hyphens, underscores, dots). Returns `false` for
    /// malformed values like `"foo"`, `"owner/repo/extra"`, `"/repo"`,
    /// `"owner/"`, `"owner /repo"`, or values containing `@` or other
    /// shell/URL metacharacters. Surrounding whitespace on the whole value is
    /// ignored, matching the trimming performed when the value is persisted.
    pub(super) fn validate_github_repo(value: &str) -> bool {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return true;
        }
        match trimmed.split_once('/') {
            Some((owner, repo)) => {
                !owner.is_empty()
                    && !repo.is_empty()
                    && !repo.contains('/')
                    && is_valid_github_component(owner)
                    && is_valid_github_component(repo)
            }
            None => false,
        }
    }

    pub(super) fn validated_agent_work_dir(repository: &Repository, value: &str) -> Option<String> {
        resolve_agent_work_dir(repository, value)
    }

    pub(super) fn remote_settings_from_fields(
        fields: &RepositoryFormFields,
    ) -> RemoteRepositorySettings {
        RemoteRepositorySettings {
            enabled: fields.remote_enabled,
            login_user: fields.login_user.trim().to_owned(),
            host: fields.host.trim().to_owned(),
            run_as_user: fields.run_as_user.trim().to_owned(),
            setup_env_default: fields.setup_env_default,
        }
    }

    pub(super) fn create_repository_from_fields(
        fields: &RepositoryFormFields,
    ) -> Option<Repository> {
        let trimmed_name = fields.name.trim();
        if trimmed_name.is_empty() {
            return None;
        }

        let slug = form_runtime::repository_slug_from_name(trimmed_name);
        if slug.is_empty() {
            return None;
        }

        if !Self::validate_github_repo(&fields.github_repo) {
            warn!(
                github_repo = %fields.github_repo,
                "rejecting repository create: github_repo must be 'owner/repo' or empty"
            );
            return None;
        }

        if let Err(error) =
            crate::domain::normalize_version_selector(&fields.default_llxprt_version)
        {
            warn!(error = %error, "rejecting repository create: invalid default_llxprt_version");
            return None;
        }

        // Reject an enabled-but-incomplete remote config visibly: the user
        // must provide both login_user and host when remote is enabled.
        // This prevents silently persisting a config that would later be
        // treated as local or rejected at launch time.
        let remote_settings = Self::remote_settings_from_fields(fields);
        if let Err(error) = crate::domain::target::validate_remote(&remote_settings) {
            warn!(error = %error, "rejecting repository create: incomplete remote config");
            return None;
        }

        let trimmed_base_dir = fields.base_dir.trim();
        let base_dir = if trimmed_base_dir.is_empty() {
            format!("/tmp/{slug}")
        } else if fields.remote_enabled {
            trimmed_base_dir.to_owned()
        } else {
            expand_tilde(trimmed_base_dir)
        };

        if !fields.remote_enabled
            && let Err(e) = std::fs::create_dir_all(&base_dir)
        {
            warn!(
                base_dir = %base_dir,
                error = %e,
                "could not create local repository base directory"
            );
        }

        Some(Repository {
            id: RepositoryId(generate_id("repo")),
            name: trimmed_name.to_owned(),
            slug,
            base_dir: std::path::PathBuf::from(&base_dir),
            default_profile: normalize_profile(&fields.default_profile),
            default_code_puppy_model: fields.default_code_puppy_model.trim().to_owned(),
            default_llxprt_version: fields.default_llxprt_version.trim().to_owned(),
            github_repo: fields.github_repo.trim().to_owned(),
            remote: remote_settings,
            issue_base_prompt: String::new(),
            default_agent_kind: AgentKind::from_form_value(&fields.default_agent_kind)
                .unwrap_or_default(),
            agent_ids: Vec::new(),
        })
    }

    pub(super) fn update_repository_from_fields(
        repo: &mut Repository,
        fields: &RepositoryFormFields,
    ) -> bool {
        let trimmed_name = fields.name.trim();
        let slug = form_runtime::repository_slug_from_name(trimmed_name);
        if trimmed_name.is_empty() || slug.is_empty() {
            return false;
        }

        if !Self::validate_github_repo(&fields.github_repo) {
            warn!(
                github_repo = %fields.github_repo,
                "rejecting repository update: github_repo must be 'owner/repo' or empty"
            );
            return false;
        }

        if let Err(error) =
            crate::domain::normalize_version_selector(&fields.default_llxprt_version)
        {
            warn!(error = %error, "rejecting repository update: invalid default_llxprt_version");
            return false;
        }

        // Reject an enabled-but-incomplete remote config visibly.
        let remote_settings = Self::remote_settings_from_fields(fields);
        if let Err(error) = crate::domain::target::validate_remote(&remote_settings) {
            warn!(error = %error, "rejecting repository update: incomplete remote config");
            return false;
        }

        trimmed_name.clone_into(&mut repo.name);
        repo.slug = slug;

        let trimmed_base_dir = fields.base_dir.trim();
        if !trimmed_base_dir.is_empty() {
            repo.base_dir = if fields.remote_enabled {
                std::path::PathBuf::from(trimmed_base_dir)
            } else {
                std::path::PathBuf::from(expand_tilde(trimmed_base_dir))
            };
        }

        repo.default_profile = normalize_profile(&fields.default_profile);
        fields
            .default_code_puppy_model
            .trim()
            .clone_into(&mut repo.default_code_puppy_model);
        fields
            .default_llxprt_version
            .trim()
            .clone_into(&mut repo.default_llxprt_version);
        repo.default_agent_kind = AgentKind::from_form_value(&fields.default_agent_kind)
            .unwrap_or(repo.default_agent_kind);
        fields.github_repo.trim().clone_into(&mut repo.github_repo);
        repo.remote = remote_settings;
        true
    }

    /// Build an agent from New Agent form fields via the canonical
    /// [`services::create_agent`] use-case.
    ///
    /// This is a thin state-layer adapter: it delegates all validation,
    /// normalization, and lifecycle policy (including the `Running` initial
    /// status) to the service, then performs the local filesystem side effect
    /// of creating the work directory — which belongs in the state layer, not
    /// the pure creation service.
    pub(super) fn create_agent_from_fields(
        repository: &Repository,
        fields: &AgentFormFields,
        next_display_index: usize,
    ) -> Option<Agent> {
        let agent = services::create_agent(CreateAgentParams {
            repository,
            name: &fields.name,
            description: &fields.description,
            work_dir: &fields.work_dir,
            profile: &fields.profile,
            code_puppy_model: &fields.code_puppy_model,
            llxprt_version: &fields.llxprt_version,
            code_puppy_yolo: fields.code_puppy_yolo,
            code_puppy_quick_resume: fields.code_puppy_quick_resume,
            agent_kind: &fields.agent_kind,
            mode: &fields.mode,
            llxprt_debug: &fields.llxprt_debug,
            pass_continue: fields.pass_continue,
            sandbox_enabled: fields.sandbox_enabled,
            sandbox_engine: &fields.sandbox_engine,
            sandbox_flags: &fields.sandbox_flags,
            shortcut_slot: fields.shortcut_slot,
            next_display_index,
        })?;

        if !repository.remote.enabled
            && let Err(e) = std::fs::create_dir_all(&agent.work_dir)
        {
            warn!(
                work_dir = %agent.work_dir.display(),
                error = %e,
                "could not create local agent work directory"
            );
        }

        Some(agent)
    }

    pub(super) fn update_agent_from_fields(
        agent: &mut Agent,
        repository: &Repository,
        fields: &AgentFormFields,
    ) {
        let trimmed_name = fields.name.trim();
        if trimmed_name.is_empty() {
            return;
        }
        let normalized_version =
            match crate::domain::normalize_version_selector(&fields.llxprt_version) {
                Ok(version) => version,
                Err(error) => {
                    warn!(error = %error, "rejecting agent update: invalid llxprt_version");
                    return;
                }
            };

        trimmed_name.clone_into(&mut agent.name);
        agent.shortcut_slot = fields.shortcut_slot;
        agent.description.clone_from(&fields.description);

        if let Some(new_dir) = Self::validated_agent_work_dir(repository, &fields.work_dir) {
            if !repository.remote.enabled
                && new_dir != agent.work_dir.to_string_lossy()
                && let Err(e) = std::fs::create_dir_all(&new_dir)
            {
                warn!(
                    work_dir = %new_dir,
                    error = %e,
                    "could not create updated local agent work directory"
                );
            }
            agent.work_dir = std::path::PathBuf::from(&new_dir);
        }

        agent.profile = normalize_profile(&fields.profile);
        fields
            .code_puppy_model
            .trim()
            .clone_into(&mut agent.code_puppy_model);
        agent.llxprt_version = normalized_version;
        agent.code_puppy_yolo = Some(fields.code_puppy_yolo);
        agent.code_puppy_quick_resume = fields.code_puppy_quick_resume.enabled();
        agent.agent_kind =
            AgentKind::from_form_value(&fields.agent_kind).unwrap_or(agent.agent_kind);
        // The mode field is the single source of truth for mode flags. An
        // empty mode yields no flags so yolo can be turned off on update; the
        // new-agent form pre-fills --yolo as the create default instead.
        agent.mode_flags = fields.mode.split_whitespace().map(String::from).collect();
        agent.llxprt_debug = normalize_llxprt_debug(&fields.llxprt_debug);
        agent.pass_continue = fields.pass_continue;
        agent.sandbox_enabled = fields.sandbox_enabled;
        let caps = PlatformCapabilities::current();
        agent.sandbox_engine = SandboxEngine::from_form_value(&fields.sandbox_engine)
            .and_then(|engine| caps.normalize_engine(engine))
            .unwrap_or_default();
        agent.sandbox_flags = normalize_sandbox_flags(&fields.sandbox_flags);
    }
}
