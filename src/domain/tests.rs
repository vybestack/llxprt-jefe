use super::*;
use serde_json::json;

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}
#[test]
fn issue_filter_default_and_open_state_are_not_active() {
    let mut filter = IssueFilter::default();
    assert!(!filter.has_active_non_default_filters());

    filter.state = Some(IssueFilterState::Open);
    assert!(!filter.has_active_non_default_filters());
}

#[test]
fn issue_filter_closed_all_and_extended_fields_are_active() {
    let mut filter = IssueFilter {
        state: Some(IssueFilterState::Closed),
        ..IssueFilter::default()
    };
    assert!(filter.has_active_non_default_filters());

    filter.state = Some(IssueFilterState::All);
    assert!(filter.has_active_non_default_filters());

    filter.state = None;
    filter.updated_after = "2026-01-01".to_string();
    assert!(filter.has_active_non_default_filters());
}

#[test]
fn issue_filter_any_sentinel_is_not_active_but_none_is_active() {
    let mut filter = IssueFilter {
        author: "any".to_string(),
        assignee: FILTER_CHOICE_ANY.to_string(),
        issue_type: "ANY".to_string(),
        milestone: "ANY".to_string(),
        module: "any".to_string(),
        mentioned: "any".to_string(),
        updated_before: "ANY".to_string(),
        updated_after: "Any".to_string(),
        ..IssueFilter::default()
    };
    assert!(!filter.has_active_non_default_filters());

    filter.query_text = "any".to_string();
    assert!(filter.has_active_non_default_filters());

    filter.query_text.clear();
    filter.assignee = FILTER_CHOICE_NONE.to_string();
    assert!(filter.has_active_non_default_filters());

    filter.assignee.clear();
    filter.milestone = FILTER_CHOICE_NONE.to_string();
    assert!(filter.has_active_non_default_filters());
}

#[test]
fn agent_pass_continue_defaults_true() {
    let agent = Agent::new(
        AgentId("test-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/tmp/test"),
    );
    assert!(agent.pass_continue);
    assert!(!agent.code_puppy_quick_resume);
}

#[test]
fn agent_kind_defaults_to_llxprt() {
    let agent = Agent::new(
        AgentId("test-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/tmp/test"),
    );
    assert_eq!(agent.agent_kind, AgentKind::Llxprt);
    assert_eq!(agent.agent_kind.binary_name(), "llxprt");
    assert!(!agent.agent_kind.is_kennel());
}

#[test]
fn code_puppy_kind_has_expected_identity() {
    assert_eq!(AgentKind::CodePuppy.binary_name(), "code-puppy");
    assert_eq!(AgentKind::CodePuppy.label(), "code_puppy");
    assert!(AgentKind::CodePuppy.is_kennel());
    assert_eq!(
        AgentKind::from_form_value("code-puppy"),
        Some(AgentKind::CodePuppy)
    );
}

#[test]
fn persisted_kinds_default_to_llxprt_when_missing() {
    let agent_json = json!({
        "id": "agent-1", "display_id": "#1", "repository_id": "repo-1",
        "name": "Agent", "description": "", "work_dir": "/tmp/a",
        "profile": "", "mode_flags": [], "pass_continue": true,
        "sandbox_enabled": false, "sandbox_engine": "podman",
        "sandbox_flags": DEFAULT_SANDBOX_FLAGS, "status": "Queued",
        "runtime_binding": null
    });
    let agent: Agent = serde_json::from_value(agent_json).value_or_panic("agent serde");
    assert_eq!(agent.agent_kind, AgentKind::Llxprt);

    let repo_json = json!({
        "id": "repo-1", "name": "Repo", "slug": "repo",
        "base_dir": "/tmp/repo", "default_profile": "", "agent_ids": []
    });
    let repo: Repository = serde_json::from_value(repo_json).value_or_panic("repo serde");
    assert_eq!(repo.default_agent_kind, AgentKind::Llxprt);
}

#[test]
fn agent_status_defaults_to_queued() {
    let agent = Agent::new(
        AgentId("test-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/tmp/test"),
    );
    assert_eq!(agent.status, AgentStatus::Queued);
}

#[test]
fn agent_sandbox_defaults_match_requirement() {
    let agent = Agent::new(
        AgentId("test-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/tmp/test"),
    );
    assert!(agent.llxprt_debug.is_empty());
    assert!(!agent.sandbox_enabled);
    assert_eq!(agent.sandbox_engine, SandboxEngine::Podman);
    assert_eq!(agent.sandbox_flags, DEFAULT_SANDBOX_FLAGS);
}

#[test]
fn agent_deserializes_missing_llxprt_debug_as_empty() {
    let value = json!({
        "id": "agent-1",
        "display_id": "#1",
        "repository_id": "repo-1",
        "name": "Agent One",
        "description": "",
        "work_dir": "/tmp/agent-1",
        "profile": "",
        "mode_flags": ["--yolo"],
        "pass_continue": true,
        "sandbox_enabled": false,
        "sandbox_engine": "podman",
        "sandbox_flags": DEFAULT_SANDBOX_FLAGS,
        "status": "Queued",
        "runtime_binding": null
    });

    let Ok(agent) = serde_json::from_value::<Agent>(value) else {
        panic!("agent should deserialize");
    };
    assert!(agent.llxprt_debug.is_empty());
    assert!(!agent.code_puppy_quick_resume);
}

#[test]
fn launch_signature_deserializes_missing_llxprt_debug_as_empty() {
    let value = json!({
        "work_dir": "/tmp/agent-1",
        "profile": "",
        "mode_flags": ["--yolo"],
        "pass_continue": true,
        "sandbox_enabled": true,
        "sandbox_engine": "podman",
        "sandbox_flags": DEFAULT_SANDBOX_FLAGS
    });

    let Ok(signature) = serde_json::from_value::<LaunchSignature>(value) else {
        panic!("launch signature should deserialize");
    };
    assert!(signature.llxprt_debug.is_empty());
    assert!(!signature.code_puppy_quick_resume);
    assert_eq!(signature.remote, RemoteRepositorySettings::default());
}

#[test]
fn repository_deserializes_missing_remote_settings_with_defaults() {
    let value = json!({
        "id": "repo-1",
        "name": "Repo One",
        "slug": "repo-one",
        "base_dir": "/tmp/repo-one",
        "default_profile": "",
        "agent_ids": []
    });

    let Ok(repository) = serde_json::from_value::<Repository>(value) else {
        panic!("repository should deserialize");
    };
    assert_eq!(repository.remote, RemoteRepositorySettings::default());
}

#[test]
fn platform_capabilities_macos_supports_all_engines() {
    let caps = PlatformCapabilities::for_os("macos");
    assert!(caps.is_engine_supported(SandboxEngine::Podman));
    assert!(caps.is_engine_supported(SandboxEngine::Docker));
    assert!(caps.is_engine_supported(SandboxEngine::Seatbelt));
    assert_eq!(caps.supported_engines().len(), 3);
}

#[test]
fn platform_capabilities_linux_excludes_seatbelt() {
    let caps = PlatformCapabilities::for_os("linux");
    assert!(caps.is_engine_supported(SandboxEngine::Podman));
    assert!(caps.is_engine_supported(SandboxEngine::Docker));
    assert!(!caps.is_engine_supported(SandboxEngine::Seatbelt));
    assert_eq!(caps.supported_engines().len(), 2);
}

#[test]
fn platform_capabilities_windows_has_no_supported_engines() {
    let caps = PlatformCapabilities::for_os("windows");
    assert!(!caps.is_engine_supported(SandboxEngine::Podman));
    assert!(!caps.is_engine_supported(SandboxEngine::Docker));
    assert!(!caps.is_engine_supported(SandboxEngine::Seatbelt));
    assert!(caps.supported_engines().is_empty());
}

#[test]
fn normalize_engine_returns_none_when_platform_has_no_supported_engines() {
    let caps = PlatformCapabilities::for_os("windows");
    assert_eq!(caps.normalize_engine(SandboxEngine::Seatbelt), None);
}

#[test]
fn next_for_capabilities_returns_self_when_supported_engines_empty() {
    let caps = PlatformCapabilities::for_os("windows");
    assert_eq!(
        SandboxEngine::Docker.next_for_capabilities(&caps),
        SandboxEngine::Docker
    );
}

#[test]
fn platform_capabilities_normalize_unsupported_engine_to_podman() {
    let caps = PlatformCapabilities::for_os("linux");
    assert_eq!(
        caps.normalize_engine(SandboxEngine::Seatbelt),
        Some(SandboxEngine::Podman)
    );
    assert_eq!(
        caps.normalize_engine(SandboxEngine::Docker),
        Some(SandboxEngine::Docker)
    );
}

#[test]
fn platform_capabilities_normalize_is_noop_on_macos() {
    let caps = PlatformCapabilities::for_os("macos");
    assert_eq!(
        caps.normalize_engine(SandboxEngine::Seatbelt),
        Some(SandboxEngine::Seatbelt)
    );
}

#[test]
fn platform_label_returns_readable_names() {
    assert_eq!(
        PlatformCapabilities::for_os("macos").platform_label(),
        "macOS"
    );
    assert_eq!(
        PlatformCapabilities::for_os("linux").platform_label(),
        "Linux"
    );
    assert_eq!(
        PlatformCapabilities::for_os("windows").platform_label(),
        "Windows"
    );
    assert_eq!(
        PlatformCapabilities::for_os("freebsd").platform_label(),
        "Unknown"
    );
}

#[test]
fn seatbelt_deserialization_still_works_across_platforms() {
    // Seatbelt must always deserialize (for persisted state portability).
    // Platform filtering happens at the capabilities layer, not serde.
    let value = json!({
        "id": "agent-seatbelt",
        "display_id": "#1",
        "repository_id": "repo-1",
        "name": "Seatbelt Agent",
        "description": "",
        "work_dir": "/tmp/sb-agent",
        "profile": "",
        "mode_flags": ["--yolo"],
        "pass_continue": true,
        "sandbox_enabled": true,
        "sandbox_engine": "seatbelt",
        "sandbox_flags": DEFAULT_SANDBOX_FLAGS,
        "status": "Queued",
        "runtime_binding": null
    });
    let Ok(agent) = serde_json::from_value::<Agent>(value) else {
        panic!("agent with seatbelt engine should deserialize");
    };
    assert_eq!(agent.sandbox_engine, SandboxEngine::Seatbelt);
}

/// Test 25: issue_base_prompt serializes and deserializes correctly.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-013
/// @pseudocode component-001 lines 190-195
#[test]
fn test_issue_base_prompt_serde_roundtrip() {
    let repo = Repository {
        id: RepositoryId("repo-1".to_string()),
        name: "Test Repo".to_string(),
        slug: "test-repo".to_string(),
        base_dir: PathBuf::from("/tmp/test-repo"),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: "Prioritize diagnosis".to_string(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        transient_agent_dir: PathBuf::new(),
        default_code_puppy_yolo: None,
        transient_max_concurrent: 0,
        agent_ids: vec![],
    };

    let json = serde_json::to_value(&repo).value_or_panic("should serialize");
    let repo2: Repository = serde_json::from_value(json).value_or_panic("should deserialize");

    assert_eq!(repo2.issue_base_prompt, "Prioritize diagnosis");
}

/// Test 26: issue_base_prompt backward compatibility with missing field.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-013
/// @pseudocode component-001 lines 196-200
#[test]
fn test_issue_base_prompt_backward_compat() {
    let value = json!({
        "id": "repo-1",
        "name": "Test Repo",
        "slug": "test-repo",
        "base_dir": "/tmp/test-repo",
        "default_profile": "",
        "remote": {
            "enabled": false,
            "login_user": "",
            "host": "",
            "run_as_user": "",
            "setup_env_default": false
        },
        "agent_ids": []
        // Note: no issue_base_prompt field
    });

    let repo: Repository = serde_json::from_value(value).value_or_panic("should deserialize");
    assert_eq!(repo.issue_base_prompt, "");
}

/// Regression for issue #121: a persisted `state.json` written before the
/// `pid` field was added to `RuntimeBinding` must still deserialize, with
/// `pid` defaulting to `None` (via `#[serde(default)]`).
#[test]
fn runtime_binding_deserializes_missing_pid_as_none() {
    let value = json!({
        "session_name": "jefe-agent-1",
        "launch_signature": {
            "work_dir": "/tmp/agent-1",
            "profile": "",
            "mode_flags": [],
            "pass_continue": true,
            "sandbox_enabled": false,
            "sandbox_engine": "podman",
            "sandbox_flags": DEFAULT_SANDBOX_FLAGS
        },
        "attached": false,
        "last_seen": null
        // Note: no pid field
    });

    let binding: RuntimeBinding =
        serde_json::from_value(value).value_or_panic("binding should deserialize");
    assert!(binding.pid.is_none());
    assert!(binding.process_identity.is_none());
}

#[test]
fn runtime_binding_roundtrips_pid_when_present() {
    let binding = RuntimeBinding {
        session_name: "jefe-agent-2".to_string(),
        launch_signature: LaunchSignature {
            work_dir: PathBuf::from("/tmp/agent-2"),
            profile: String::new(),
            code_puppy_model: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: SandboxEngine::Podman,
            sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: RemoteRepositorySettings::default(),
            agent_kind: crate::domain::AgentKind::Llxprt,
        },
        attached: false,
        last_seen: None,
        pid: Some(42_000),
        process_identity: Some(ProcessIdentity::new(42_000, 123_456)),
    };

    let json = serde_json::to_value(&binding).value_or_panic("should serialize");
    let binding2: RuntimeBinding =
        serde_json::from_value(json).value_or_panic("should deserialize");
    assert_eq!(binding2.pid, Some(42_000));
    assert_eq!(
        binding2.process_identity,
        Some(ProcessIdentity::new(42_000, 123_456))
    );
}

// =============================================================================
// PR review threads (issue #119)
// =============================================================================

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[test]
fn pr_review_thread_constructs_with_thread_id_and_resolved_flag() {
    let thread = PrReviewThread {
        thread_id: "PRRT_kwAAA".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/lib.rs".to_string()),
        line: Some(42),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer1".to_string(),
            created_at: "2026-07-01T10:00:00Z".to_string(),
            edited_at: None,
            body: "Please fix this".to_string(),
        }],
    };
    assert_eq!(thread.thread_id, "PRRT_kwAAA");
    assert!(!thread.is_resolved);
    assert!(!thread.is_outdated);
    assert_eq!(thread.path.as_deref(), Some("src/lib.rs"));
    assert_eq!(thread.line, Some(42));
    assert_eq!(thread.comments.len(), 1);
}

/// @plan PLAN-PR-REVIEW-THREADS
/// @requirement REQ-PR-009
#[test]
fn pr_review_carries_review_threads_field() {
    let review = PrReview {
        review_id: Some("PRR_kw001".to_string()),
        author_login: "reviewer1".to_string(),
        state: PrReviewState::Commented,
        submitted_at: "2026-07-01T10:00:00Z".to_string(),
        body: Some("Please review".to_string()),
        review_threads: vec![PrReviewThread {
            thread_id: "PRRT_kwBBB".to_string(),
            is_resolved: true,
            is_outdated: false,
            review_id: Some("PRR_kw001".to_string()),
            path: None,
            line: None,
            comments: vec![],
        }],
    };
    assert_eq!(review.review_threads.len(), 1);
    let thread = &review.review_threads[0];
    assert!(thread.is_resolved);
    assert!(thread.path.is_none());
    assert!(thread.line.is_none());
    assert!(thread.comments.is_empty());
}

/// @plan PLAN-20260624-PR-MODE.P03
/// @requirement REQ-PR-009
#[test]
fn pr_review_thread_supports_unresolved_with_location() {
    let thread = PrReviewThread {
        thread_id: "PRRT_kwCCC".to_string(),
        is_resolved: false,
        is_outdated: false,
        review_id: None,
        path: Some("src/main.rs".to_string()),
        line: Some(10),
        comments: vec![
            IssueComment {
                comment_id: 100,
                author_login: "alice".to_string(),
                created_at: "2026-07-01T10:00:00Z".to_string(),
                edited_at: None,
                body: "First reply".to_string(),
            },
            IssueComment {
                comment_id: 101,
                author_login: "bob".to_string(),
                created_at: "2026-07-01T11:00:00Z".to_string(),
                edited_at: Some("2026-07-01T11:30:00Z".to_string()),
                body: "Second reply".to_string(),
            },
        ],
    };
    assert_eq!(thread.comments.len(), 2);
    assert_eq!(thread.comments[0].author_login, "alice");
    assert_eq!(thread.comments[1].author_login, "bob");
    assert_eq!(
        thread.comments[1].edited_at.as_deref(),
        Some("2026-07-01T11:30:00Z")
    );
}

// =============================================================================
// Transient Agent Support (issue #213)
// =============================================================================

#[test]
fn repository_new_defaults_transient_fields() {
    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        "Test Repo".into(),
        "test-repo".into(),
        PathBuf::from("/tmp/repo"),
    );
    assert!(repo.transient_agent_dir.as_os_str().is_empty());
    assert_eq!(repo.default_code_puppy_yolo, None);
    assert_eq!(repo.transient_max_concurrent, 0);
}

#[test]
fn repository_effective_transient_dir_defaults_to_system_temp_when_empty() {
    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        "Test Repo".into(),
        "test-repo".into(),
        PathBuf::from("/tmp/repo"),
    );
    assert_eq!(repo.effective_transient_dir(), std::env::temp_dir());
}

#[test]
fn repository_effective_transient_dir_returns_configured_dir_when_set() {
    let mut repo = Repository::new(
        RepositoryId("repo-1".into()),
        "Test Repo".into(),
        "test-repo".into(),
        PathBuf::from("/tmp/repo"),
    );
    repo.transient_agent_dir = PathBuf::from("/var/tmp/jefe-agents");
    assert_eq!(
        repo.effective_transient_dir(),
        PathBuf::from("/var/tmp/jefe-agents")
    );
}

#[test]
fn agent_new_defaults_is_transient_false() {
    let agent = Agent::new(
        AgentId("test-1".into()),
        RepositoryId("repo-1".into()),
        "Test Agent".into(),
        PathBuf::from("/tmp/test"),
    );
    assert!(!agent.is_transient());
}

#[test]
fn agent_new_transient_sets_is_transient_true_and_inherits_repo_defaults() {
    let mut repo = Repository::new(
        RepositoryId("repo-1".into()),
        "My Repo".into(),
        "my-repo".into(),
        PathBuf::from("/tmp/repo"),
    );
    repo.default_profile = "dev".to_string();
    repo.default_code_puppy_model = "gpt-5".to_string();
    repo.default_code_puppy_yolo = Some(true);
    repo.default_agent_kind = AgentKind::CodePuppy;

    let work_dir = repo.effective_transient_dir().join("jefe-transient-1");
    let agent = Agent::new_transient(
        AgentId("transient-1".into()),
        RepositoryId("repo-1".into()),
        work_dir.clone(),
        &repo,
    );

    assert!(agent.is_transient());
    assert_eq!(agent.id, AgentId("transient-1".into()));
    assert_eq!(agent.repository_id, RepositoryId("repo-1".into()));
    assert_eq!(agent.work_dir, work_dir);
    assert_eq!(agent.profile, "dev");
    assert_eq!(agent.code_puppy_model, "gpt-5");
    assert_eq!(agent.code_puppy_yolo, Some(true));
    assert_eq!(agent.agent_kind, AgentKind::CodePuppy);
    assert!(!agent.pass_continue, "transient agents are one-shot");
    assert_eq!(agent.status, AgentStatus::Queued);
    assert!(agent.name.contains("My Repo"));
}

#[test]
fn repository_transient_fields_backward_compat_with_missing_fields() {
    let repo_json = json!({
        "id": "repo-1",
        "name": "Repo",
        "slug": "repo",
        "base_dir": "/tmp/repo",
        "default_profile": "",
        "agent_ids": []
        // Note: no transient_agent_dir, default_code_puppy_yolo, transient_max_concurrent
    });
    let repo: Repository = serde_json::from_value(repo_json).value_or_panic("repo serde");
    assert!(repo.transient_agent_dir.as_os_str().is_empty());
    assert_eq!(repo.default_code_puppy_yolo, None);
    assert_eq!(repo.transient_max_concurrent, 0);
    assert_eq!(repo.effective_transient_dir(), std::env::temp_dir());
}

#[test]
fn agent_is_transient_backward_compat_with_missing_field() {
    let agent_json = json!({
        "id": "agent-1",
        "display_id": "#1",
        "repository_id": "repo-1",
        "name": "Agent",
        "description": "",
        "work_dir": "/tmp/a",
        "profile": "",
        "mode_flags": [],
        "pass_continue": true,
        "sandbox_enabled": false,
        "sandbox_engine": "podman",
        "sandbox_flags": DEFAULT_SANDBOX_FLAGS,
        "status": "Queued",
        "runtime_binding": null
        // Note: no origin field
    });
    let agent: Agent = serde_json::from_value(agent_json).value_or_panic("agent serde");
    assert!(!agent.is_transient());
}
