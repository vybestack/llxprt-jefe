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
fn agent_new_uses_generic_type_and_values() {
    let type_id =
        crate::domain::AgentTypeId::parse("core.llxprt").value_or_panic("valid shipped type id");
    let mut values = crate::domain::TypedMap::new();
    crate::domain::canonical_values::insert_json(
        &mut values,
        "profile",
        serde_json::Value::String("review".to_owned()),
    )
    .value_or_panic("valid profile value");
    let agent = Agent::new(
        AgentId("agent-1".to_owned()),
        RepositoryId("repo-1".to_owned()),
        type_id.clone(),
        values.clone(),
        "Agent".to_owned(),
        "/tmp/agent".into(),
    );
    assert_eq!(agent.type_id, type_id);
    assert_eq!(agent.values, values);
    assert_eq!(agent.status, AgentStatus::Queued);
    assert_eq!(agent.origin, AgentOrigin::Persistent);
}

#[test]
fn repository_new_uses_generic_defaults() {
    let type_id = crate::domain::AgentTypeId::parse("core.code-puppy")
        .value_or_panic("valid shipped type id");
    let mut defaults = crate::domain::TypedMap::new();
    crate::domain::canonical_values::insert_json(
        &mut defaults,
        "model",
        serde_json::Value::String("fixture-model".to_owned()),
    )
    .value_or_panic("valid model value");
    let repository = Repository::new(
        RepositoryId("repo-1".to_owned()),
        type_id.clone(),
        defaults.clone(),
        "Repo".to_owned(),
        "repo".to_owned(),
        "/tmp/repo".into(),
    );
    assert_eq!(repository.default_type_id, type_id);
    assert_eq!(repository.default_values, defaults);
    assert_eq!(repository.transient_max_concurrent, 0);
    assert!(repository.transient_agent_dir.as_os_str().is_empty());
}

#[test]
fn transient_agent_inherits_generic_repository_defaults_once() {
    let type_id = crate::domain::AgentTypeId::parse("core.code-puppy")
        .value_or_panic("valid shipped type id");
    let mut defaults = crate::domain::TypedMap::new();
    crate::domain::canonical_values::insert_json(
        &mut defaults,
        "yolo",
        serde_json::Value::Bool(true),
    )
    .value_or_panic("valid yolo value");
    let mut repository = Repository::new(
        RepositoryId("repo-1".to_owned()),
        type_id.clone(),
        defaults.clone(),
        "Repo".to_owned(),
        "repo".to_owned(),
        std::env::temp_dir(),
    );
    repository.transient_agent_dir = std::env::temp_dir();
    let agent = Agent::new_transient(
        AgentId("transient-1".to_owned()),
        repository.id.clone(),
        repository.transient_agent_dir.join("transient-1"),
        &repository,
    );
    assert_eq!(agent.type_id, type_id);
    assert_eq!(agent.values, defaults);
    assert_eq!(agent.origin, AgentOrigin::Transient);

    repository.default_values.clear();
    assert!(!agent.values.is_empty());
}

#[test]
fn runtime_binding_roundtrips_launch_signature_v1() {
    let binding = RuntimeBinding {
        session_name: "jefe-agent-1".to_owned(),
        launch_signature: LaunchSignatureV1::default(),
        attached: true,
        last_seen: Some(42),
        pane_identity: Some(PaneProcessIdentity::new(7, 11)),
        worker_identity: Some(WorkerProcessIdentity::new(8, 12)),
        lifecycle_generation: 3,
        worker_identities: Vec::new(),
    };
    let json = serde_json::to_value(&binding).value_or_panic("serialize binding");
    let restored: RuntimeBinding =
        serde_json::from_value(json).value_or_panic("deserialize binding");
    assert_eq!(restored.launch_signature, LaunchSignatureV1::default());
    assert_eq!(
        restored.pane_identity,
        Some(PaneProcessIdentity::new(7, 11)),
        "the pane identity must round-trip in its own role"
    );
    assert_eq!(
        restored.worker_identity,
        Some(WorkerProcessIdentity::new(8, 12)),
        "the worker identity must round-trip distinctly from the pane's"
    );
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

/// Test 25: issue_base_prompt serializes and deserializes correctly.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-013
/// @pseudocode component-001 lines 190-195
#[test]
fn test_issue_base_prompt_serde_roundtrip() {
    let mut repo = Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Test Repo".to_string(),
        "test-repo".to_string(),
        PathBuf::from("/tmp/test-repo"),
    );
    repo.issue_base_prompt = "Prioritize diagnosis".to_string();

    let json = serde_json::to_value(&repo).value_or_panic("should serialize");
    let repo2: Repository = serde_json::from_value(json).value_or_panic("should deserialize");

    assert_eq!(repo2.issue_base_prompt, "Prioritize diagnosis");
}

/// Regression for issue #121: a persisted `state.json` written before any
/// process identity was recorded on `RuntimeBinding` must still deserialize.
/// Since issue #543 split the roles, "absent" must leave *both* the pane and
/// the worker unknown rather than populating either from the other.
#[test]
fn runtime_binding_without_identities_deserializes_with_every_role_absent() {
    let value = json!({
        "session_name": "jefe-agent-1",
        "launch_signature": {
            "version": 0,
            "definition_hash": "0".repeat(64),
            "typed_value_hash": "0".repeat(64),
            "target_fingerprint": "0".repeat(64)
        },
        "attached": false,
        "last_seen": null
    });

    let binding: RuntimeBinding =
        serde_json::from_value(value).value_or_panic("binding should deserialize");
    assert!(binding.pane_identity.is_none());
    assert!(binding.worker_identity.is_none());
    assert!(
        binding.worker_identities.is_empty(),
        "legacy state.json without worker_identities must default to empty"
    );
}

/// Issue #543: the legacy `pid` / `process_identity` keys always held the *pane
/// leader*, never the worker — that was the defect. They must therefore load
/// into `pane_identity`, and must not be promoted into the worker role, because
/// on Windows the pane leader is an ancestor two hops above the agent.
#[test]
fn legacy_process_identity_loads_as_the_pane_identity_not_the_worker() {
    let value = serde_json::json!({
        "session_name": "jefe-agent-legacy",
        "launch_signature": {
            "version": 0,
            "definition_hash": "0".repeat(64),
            "typed_value_hash": "0".repeat(64),
            "target_fingerprint": "0".repeat(64)
        },
        "attached": false,
        "last_seen": null,
        "pid": 42_000,
        "process_identity": { "pid": 42_000, "started_at": 123_456 }
    });

    let binding: RuntimeBinding =
        serde_json::from_value(value).value_or_panic("legacy binding should deserialize");

    assert_eq!(
        binding.pane_identity,
        Some(PaneProcessIdentity::new(42_000, 123_456)),
        "the legacy identity described the pane leader"
    );
    assert!(
        binding.worker_identity.is_none(),
        "a pane identity must never be promoted into the worker role"
    );
}

/// Older files predate `process_identity` entirely and carry only a bare `pid`.
/// That PID is still pane evidence, but without a creation token, so it must
/// load with no `started_at` — which the PID-reuse guard treats as unverifiable
/// rather than as a match.
#[test]
fn legacy_bare_pid_loads_as_pane_evidence_without_a_creation_token() {
    let value = serde_json::json!({
        "session_name": "jefe-agent-ancient",
        "launch_signature": {
            "version": 0,
            "definition_hash": "0".repeat(64),
            "typed_value_hash": "0".repeat(64),
            "target_fingerprint": "0".repeat(64)
        },
        "attached": false,
        "last_seen": null,
        "pid": 42_000
    });

    let binding: RuntimeBinding =
        serde_json::from_value(value).value_or_panic("ancient binding should deserialize");

    let Some(pane) = binding.pane_identity else {
        panic!("a bare legacy pid is still pane evidence and must be retained");
    };
    assert_eq!(pane.pid(), 42_000);
    assert_eq!(
        pane.started_at(),
        None,
        "no creation token was recorded, so PID reuse cannot be ruled out"
    );
    assert!(binding.worker_identity.is_none());
}

/// Every identity role survives a round trip in its own slot, so a restarted
/// jefe recovers the pane leader and the worker as separate facts (issue #543).
#[test]
fn runtime_binding_roundtrips_each_identity_role_separately() {
    let binding = RuntimeBinding {
        session_name: "jefe-agent-2".to_string(),
        launch_signature: LaunchSignatureV1::default(),
        attached: false,
        last_seen: None,
        pane_identity: Some(PaneProcessIdentity::new(42_000, 123_456)),
        worker_identity: Some(WorkerProcessIdentity::new(42_010, 123_460)),
        lifecycle_generation: 0,
        worker_identities: vec![
            WorkerProcessIdentity::new(42_001, 123_457),
            WorkerProcessIdentity::new(42_002, 123_458),
        ],
    };

    let json = serde_json::to_value(&binding).value_or_panic("should serialize");
    let binding2: RuntimeBinding =
        serde_json::from_value(json).value_or_panic("should deserialize");
    assert_eq!(
        binding2.pane_identity,
        Some(PaneProcessIdentity::new(42_000, 123_456))
    );
    assert_eq!(
        binding2.worker_identity,
        Some(WorkerProcessIdentity::new(42_010, 123_460)),
        "the worker identity must round-trip independently of the pane identity"
    );
    assert_eq!(
        binding2.worker_identities,
        vec![
            WorkerProcessIdentity::new(42_001, 123_457),
            WorkerProcessIdentity::new(42_002, 123_458),
        ],
        "worker_identities must round-trip through serde"
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
        anchor: None,
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
            anchor: None,
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
        anchor: None,
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

#[test]
fn high_precision_decimals_survive_json_encoding() {
    use crate::domain::canonical_values::typed_to_json;
    use crate::domain::{CanonicalDecimal, TypedValue};

    let text = "1.2345678901234567890123";
    let decimal = CanonicalDecimal::parse(text).value_or_panic("a canonical decimal");

    let encoded = typed_to_json(&TypedValue::Decimal(decimal));

    let rendered = match &encoded {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(value) => value.clone(),
        other => panic!("a decimal should encode as a number or its exact text, got {other}"),
    };
    assert_eq!(
        rendered, text,
        "every significant digit must survive the encoding"
    );

    // Values a double represents exactly still encode as JSON numbers.
    let exact = CanonicalDecimal::parse("0.5").value_or_panic("an exact decimal");
    assert!(
        typed_to_json(&TypedValue::Decimal(exact)).is_number(),
        "an exactly representable decimal stays a JSON number"
    );
}

/// SemVer 2.0.0 allows hyphens inside prerelease identifiers, so a version such
/// as `1.0.0-rc-beta` is valid and must not be rejected (issue #381).
#[test]
fn semver_accepts_hyphens_inside_the_prerelease() {
    use crate::domain::CanonicalSemver;

    let parsed = CanonicalSemver::parse("1.0.0-rc-beta");

    assert!(
        parsed.is_ok(),
        "a hyphenated prerelease identifier is valid SemVer, got {parsed:?}"
    );
    assert_eq!(
        parsed.value_or_panic("the parsed version").to_string(),
        "1.0.0-rc-beta",
        "the version renders back to its canonical text"
    );

    // Build metadata still cannot repeat its separator.
    assert!(
        CanonicalSemver::parse("1.0.0+build+again").is_err(),
        "a repeated build separator stays invalid"
    );
}

/// The durable contract only accepts canonical digests. Deserialization must
/// enforce the same rule as `parse`, otherwise a malformed launch signature
/// enters schema-2 documents through the strict parser (issue #381).
#[test]
fn digests_reject_non_canonical_text_when_deserialized() {
    use crate::domain::Sha256Digest;

    for malformed in [
        "\"not-a-digest\"",
        "\"ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"",
        "\"abc\"",
    ] {
        let decoded: Result<Sha256Digest, _> = serde_json::from_str(malformed);
        assert!(
            decoded.is_err(),
            "{malformed} is not a canonical digest and must be refused"
        );
    }

    let canonical = "\"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"";
    let decoded: Sha256Digest =
        serde_json::from_str(canonical).value_or_panic("a canonical digest");
    assert_eq!(
        serde_json::to_string(&decoded).value_or_panic("re-encode the digest"),
        canonical,
        "a canonical digest round-trips unchanged"
    );
}

/// Both process roles survive a durable round trip in their own slots, so a
/// restore never has to infer one identity from the other (issue #543).
#[test]
fn runtime_record_roundtrips_pane_and_worker_identities_separately() {
    let record = crate::domain::state_contract::RuntimeRecord {
        session_id: Some("jefe-agent-1".to_owned()),
        invocation_generation: 3,
        last_known: crate::domain::state_contract::LastKnownRuntime::Running,
        pane_identity: Some(PaneProcessIdentity::new(1111, 7)),
        worker_identity: Some(WorkerProcessIdentity::new(2222, 9)),
        worker_identities: Vec::new(),
    };

    let encoded = serde_json::to_string(&record)
        .unwrap_or_else(|error| panic!("runtime record must serialize: {error}"));
    let decoded: crate::domain::state_contract::RuntimeRecord = serde_json::from_str(&encoded)
        .unwrap_or_else(|error| panic!("runtime record must deserialize: {error}"));

    assert_eq!(
        decoded.pane_identity,
        Some(PaneProcessIdentity::new(1111, 7))
    );
    assert_eq!(
        decoded.worker_identity,
        Some(WorkerProcessIdentity::new(2222, 9)),
        "the worker must come back as the worker, not as the pane leader"
    );
}

/// A document written before the roles were separated still loads, and leaves
/// both identities unrecorded rather than inventing one (issue #543).
#[test]
fn a_runtime_record_without_identities_loads_with_both_roles_absent() {
    let legacy =
        r#"{"session_id":"jefe-agent-1","invocation_generation":2,"last_known":"running"}"#;

    let decoded: crate::domain::state_contract::RuntimeRecord = serde_json::from_str(legacy)
        .unwrap_or_else(|error| panic!("a pre-split document must still load: {error}"));

    assert_eq!(decoded.pane_identity, None);
    assert_eq!(
        decoded.worker_identity, None,
        "an unrecorded worker must stay unknown, not be filled in from the pane"
    );
}
