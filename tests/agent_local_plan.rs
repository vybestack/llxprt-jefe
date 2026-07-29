//! Issue #382 S7 (CW02-06) focused tests: definition-driven immutable LOCAL
//! `AgentLaunchPlan` generation.
//!
//! Authority: issue #382 CW02-06 acceptance row — "WHEN a supported local
//! operation is submitted, Jefe shall produce the fixture-golden argv/env/cwd
//! plan." The planner is a pure, side-effect-free function over a validated
//! `AgentDefinition` plus typed field values, a chosen `Operation`, a local
//! canonical target, and compatible current probe evidence/generations. It
//! resolves operation/target support before effects: unsupported emits zero
//! effects. There is no remote serialization, execution, stale recheck,
//! preflight process effect, fresh send orchestration, persistence, or
//! migration in this slice.

use std::ffi::OsString;
use std::path::PathBuf;

use jefe::domain::agent_definition::{
    AgentDefinition, Availability, FieldValue, Operation, Preflight, ProbeErrorCode, Target,
};
use jefe::runtime::agent_plan::{
    AgentPlanError, LaunchFieldValues, PlanOutcome, PlanRequest, plan_local_launch,
};

/// A compatible probe result for a definition with the given identity,
/// capabilities, and generation.
fn compatible(identity: &str, capabilities: &[&str], generation: u64) -> Availability {
    Availability::InstalledCompatible {
        identity: identity.to_string(),
        capabilities: capabilities
            .iter()
            .map(|&capability| capability.to_string())
            .collect(),
        generation,
    }
}

/// Locate a shipped definition by display name.
fn shipped(name: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|d| d.display_name == name)
        .unwrap_or_else(|| panic!("shipped definition {name}"))
}

/// Canonical local cwd fixture.
const CWD: &str = "/srv/project";

fn local_target() -> Target {
    Target::Local {
        canonical_cwd: PathBuf::from(CWD),
    }
}

// ---------------------------------------------------------------------------
// LLxprt Normal — fixture-golden argv/env/cwd plan
// ---------------------------------------------------------------------------

#[test]
fn llxprt_normal_golden_plan() {
    let definition = shipped("LLxprt");
    let mut values = LaunchFieldValues::new();
    values.set_repository("profile", FieldValue::String("my-profile".to_string()));
    values.set_repository("yolo", FieldValue::Boolean(true));
    values.set_agent("prompt_interactive", FieldValue::Boolean(true));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/opt/llxprt/bin/llxprt"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/opt/llxprt/bin/llxprt"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(
            "0.10.0",
            &[
                "prompt-interactive",
                "profile",
                "sandbox",
                "yolo",
                "continue",
            ],
            3,
        ),
        probe_generation: 3,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "llxprt normal");
    // Argv emitted element-by-element in declaration order:
    //   Option{--profile-load, profile} -> --profile-load, my-profile
    //   Flag{yolo}                       -> --yolo
    //   Flag{prompt_interactive}         -> --prompt-interactive
    assert_eq!(
        plan.argv,
        vec![
            os("--profile-load"),
            os("my-profile"),
            os("--yolo"),
            os("--prompt-interactive"),
        ],
    );
    assert_eq!(plan.executable, PathBuf::from("/opt/llxprt/bin/llxprt"));
    assert_eq!(plan.cwd, PathBuf::from(CWD));
    assert_eq!(plan.operation, Operation::Normal);
    assert_eq!(plan.type_id, definition.id);
    assert_eq!(plan.definition_sha256, definition.sha256());
    assert_eq!(plan.probe_generation, 3);
    assert_eq!(plan.target_generation, 1);
    // Empty-based env allowlist: only declared typed env emitters (none here).
    assert!(plan.env.is_empty(), "no ambient env vars");
    // Signature excludes secrets/display-only values (contract).
    assert!(plan.signature_excludes_secrets());
}

// ---------------------------------------------------------------------------
// Code Puppy Normal — fixture-golden argv/env/cwd plan
// ---------------------------------------------------------------------------

#[test]
fn code_puppy_normal_golden_plan() {
    let definition = shipped("Code Puppy");
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("gpt-4o".to_string()));
    values.set_repository("yolo", FieldValue::OptionalBoolean(Some(true)));
    values.set_agent("interactive", FieldValue::Boolean(true));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/home/user/.local/bin/code-puppy"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/home/user/.local/bin/code-puppy"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("0.0.634", &["interactive", "model", "yolo"], 5),
        probe_generation: 5,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "code-puppy normal");
    // Option{--model, model} -> --model, gpt-4o
    // BooleanOption{--yolo, yolo, true} -> --yolo, true
    // Flag{interactive} -> --interactive
    assert_eq!(
        plan.argv,
        vec![
            os("--model"),
            os("gpt-4o"),
            os("--yolo"),
            os("true"),
            os("--interactive"),
        ],
    );
    assert_eq!(
        plan.executable,
        PathBuf::from("/home/user/.local/bin/code-puppy")
    );
}

// ---------------------------------------------------------------------------
// Codex CLI Normal — fixture-golden argv/env/cwd plan
// ---------------------------------------------------------------------------

#[test]
fn codex_normal_golden_plan() {
    let definition = shipped("Codex CLI");
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("o4-mini".to_string()));
    values.set_repository("profile", FieldValue::String("dev".to_string()));
    values.set_repository("sandbox", FieldValue::String("workspace-write".to_string()));
    values.set_agent("prompt", FieldValue::String("hello world".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/opt/homebrew/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/opt/homebrew/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(
            "codex-cli 0.142.0",
            &["model", "profile", "sandbox", "cwd", "resume"],
            2,
        ),
        probe_generation: 2,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "codex normal");
    // Option{--model, model} -> --model, o4-mini
    // Option{--profile, profile} -> --profile, dev
    // Option{--sandbox, sandbox} -> --sandbox, workspace-write
    // Positional{prompt} -> hello world
    assert_eq!(
        plan.argv,
        vec![
            os("--model"),
            os("o4-mini"),
            os("--profile"),
            os("dev"),
            os("--sandbox"),
            os("workspace-write"),
            os("hello world"),
        ],
    );
}

// ---------------------------------------------------------------------------
// Claude Code Normal — fixture-golden argv/env/cwd plan
// ---------------------------------------------------------------------------

#[test]
fn claude_normal_golden_plan() {
    let definition = shipped("Claude Code");
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("sonnet".to_string()));
    values.set_repository("permission_mode", FieldValue::String("auto".to_string()));
    values.set_agent("prompt", FieldValue::String("hello".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/local/bin/claude"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/local/bin/claude"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible(
            "2.1.212 (Claude Code)",
            &["continue", "resume", "model", "permission-mode"],
            4,
        ),
        probe_generation: 4,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "claude normal");
    // Option{--model, model} -> --model, sonnet
    // Option{--permission-mode, permission_mode} -> --permission-mode, auto
    // Positional{prompt} -> hello
    assert_eq!(
        plan.argv,
        vec![
            os("--model"),
            os("sonnet"),
            os("--permission-mode"),
            os("auto"),
            os("hello"),
        ],
    );
}

// ---------------------------------------------------------------------------
// Empty/optional values skip their emitters
// ---------------------------------------------------------------------------

#[test]
fn empty_optional_values_skip_emitters() {
    let definition = shipped("Codex CLI");
    let mut values = LaunchFieldValues::new();
    // model and profile empty -> Option emitters skip
    values.set_repository("model", FieldValue::String(String::new()));
    values.set_repository("profile", FieldValue::String(String::new()));
    values.set_repository("sandbox", FieldValue::String("read-only".to_string()));
    values.set_agent("prompt", FieldValue::String("hi".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex-cli", &["sandbox"], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "codex empty-optionals");
    // Only --sandbox read-only and positional hi remain.
    assert_eq!(plan.argv, vec![os("--sandbox"), os("read-only"), os("hi")],);
}

#[test]
fn optional_boolean_none_skips_boolean_option() {
    let definition = shipped("Code Puppy");
    let mut values = LaunchFieldValues::new();
    values.set_repository("model", FieldValue::String("m".to_string()));
    values.set_repository("yolo", FieldValue::OptionalBoolean(None));
    values.set_agent("interactive", FieldValue::Boolean(false));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/code-puppy"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/code-puppy"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("0.0.634", &["interactive", "model"], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "code-puppy optional-none");
    // model emitted; yolo is None so BooleanOption skips; interactive false so Flag skips.
    assert_eq!(plan.argv, vec![os("--model"), os("m")]);
}

// ---------------------------------------------------------------------------
// Unsupported operation/target emit zero effects
// ---------------------------------------------------------------------------

#[test]
fn unsupported_operation_emits_zero_effects() {
    // Codex fresh_issue is unsupported by the shipped definition.
    let definition = shipped("Codex CLI");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::FreshIssue,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Unsupported { reason } => {
            assert!(
                reason.contains("not fixture-verified"),
                "unsupported reason carries the declared reason: {reason}"
            );
        }
        other => panic!("Codex FreshIssue must be unsupported, got {other:?}"),
    }
}

#[test]
fn unsupported_target_emits_zero_effects() {
    // Construct a definition with an unsupported local target.
    let mut definition = shipped("Codex CLI");
    definition.targets.local.supported =
        jefe::domain::agent_definition::Support::unsupported("local target not fixture-verified");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Unsupported { reason } => {
            assert!(
                reason.contains("not fixture-verified"),
                "unsupported target reason: {reason}"
            );
        }
        other => panic!("unsupported local target must be unsupported, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Typed validation errors before any effect
// ---------------------------------------------------------------------------

#[test]
fn unknown_field_value_is_rejected() {
    let definition = shipped("Codex CLI");
    let mut values = LaunchFieldValues::new();
    // 'nonexistent' is not a declared field.
    values.set_repository("nonexistent", FieldValue::String("x".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::UnknownFieldValue { field }) => {
            assert_eq!(field, "nonexistent");
        }
        other => panic!("unknown field value must error, got {other:?}"),
    }
}

#[test]
fn incompatible_probe_is_rejected() {
    let definition = shipped("Codex CLI");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: Availability::InstalledIncompatible {
            reason: "missing required capability: model".to_string(),
            generation: 1,
        },
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::ProbeIncompatible { reason }) => {
            assert!(reason.contains("model"), "reason: {reason}");
        }
        other => panic!("incompatible probe must error, got {other:?}"),
    }
}

#[test]
fn not_found_probe_is_rejected() {
    let definition = shipped("Codex CLI");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: Availability::NotFound,
        probe_generation: 0,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::ProbeNotFound) => {}
        other => panic!("not-found probe must error, got {other:?}"),
    }
}

#[test]
fn probe_generation_mismatch_is_rejected() {
    let definition = shipped("Codex CLI");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 2),
        probe_generation: 1, // mismatch
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::ProbeGenerationMismatch { plan: 1, probe: 2 }) => {}
        other => panic!("generation mismatch must error, got {other:?}"),
    }
}

#[test]
fn remote_target_rejected_for_local_planner() {
    // The local planner only accepts local targets; a remote target on a
    // locally-supported definition (LLxprt) is not a local plan.
    let definition = shipped("LLxprt");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: Target::Remote(jefe::domain::agent_definition::RemoteTarget::default()),
        executable: PathBuf::from("/usr/bin/llxprt"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/llxprt"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("0.10.0", &["prompt-interactive"], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::NotLocalTarget) => {}
        other => panic!("remote target on local planner must error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Env allowlist: typed env emitters only; no ambient vars
// ---------------------------------------------------------------------------

#[test]
fn env_emitter_adds_declared_name_only() {
    // Build a minimal definition with one env emitter.
    let mut definition = shipped("Codex CLI");
    definition
        .emitters
        .push(jefe::domain::agent_definition::Emitter::Environment {
            name: "CODEX_LOG".to_string(),
            field: "profile".to_string(),
        });
    let mut values = LaunchFieldValues::new();
    values.set_repository("profile", FieldValue::String("verbose".to_string()));
    values.set_agent("prompt", FieldValue::String("hi".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &["profile"], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "codex env emitter");
    let env: Vec<(String, String)> = plan
        .env
        .iter()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();
    assert_eq!(env, vec![("CODEX_LOG".to_string(), "verbose".to_string())]);
}

// ---------------------------------------------------------------------------
// OsString preservation
// ---------------------------------------------------------------------------

#[test]
fn argv_preserves_osstring() {
    const UNICODE_PROMPT: &str = "unicode: caf\u{e9} \u{2615}";
    let definition = shipped("Codex CLI");
    let mut values = LaunchFieldValues::new();
    // Multi-byte unicode prompt proves argv elements are preserved byte-wise
    // as OsString rather than token-split or re-encoded.
    values.set_agent("prompt", FieldValue::String(UNICODE_PROMPT.to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "codex unicode");
    let last = plan
        .argv
        .last()
        .unwrap_or_else(|| panic!("positional emitted; got {:?}", plan.argv));
    assert_eq!(last, &os(UNICODE_PROMPT));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn os(s: &str) -> OsString {
    OsString::from(s)
}

fn assert_supported(
    request: PlanRequest<'_>,
    label: &str,
) -> jefe::domain::agent_definition::AgentLaunchPlan {
    match plan_local_launch(&request) {
        PlanOutcome::Supported(plan) => *plan,
        other => panic!("{label} must produce a supported plan, got {other:?}"),
    }
}

#[test]
fn probe_error_is_rejected() {
    let definition = shipped("Codex CLI");
    let values = LaunchFieldValues::new();
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: Availability::ProbeError {
            code: ProbeErrorCode::Agte202,
            reason: "invalid UTF-8".to_string(),
            generation: 1,
        },
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    match plan_local_launch(&request) {
        PlanOutcome::Error(AgentPlanError::ProbeError { reason, .. }) => {
            assert!(reason.contains("UTF-8"), "reason: {reason}");
        }
        other => panic!("probe error must error, got {other:?}"),
    }
}

#[test]
fn repeated_option_emits_one_per_element() {
    let mut definition = shipped("Codex CLI");
    definition.repository_fields.push(include_dirs_field());
    definition
        .emitters
        .push(jefe::domain::agent_definition::Emitter::RepeatedOption {
            name: "--include".to_string(),
            field: "include_dirs".to_string(),
        });
    let mut values = LaunchFieldValues::new();
    values.set_repository(
        "include_dirs",
        FieldValue::StringList(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
    );
    values.set_agent("prompt", FieldValue::String("hi".to_string()));
    let request = PlanRequest {
        definition: &definition,
        operation: Operation::Normal,
        target: local_target(),
        executable: PathBuf::from("/usr/bin/codex"),
        executable_fingerprint: jefe::agent_candidate_fingerprint::CandidateFingerprint::new(
            PathBuf::from("/usr/bin/codex"),
            None,
            None,
            0,
            0,
        ),
        executable_wrapper: jefe::agent_candidate_path::AgentWrapperKind::Direct,
        argv_prefix: Vec::new(),
        probe: compatible("codex", &[], 1),
        probe_generation: 1,
        target_generation: 1,
        values: &values,
        activation_generation: 1,
        preflight: Preflight::default(),
    };
    let plan = assert_supported(request, "codex repeated option");
    let as_strings = argv_strings(&plan);
    assert_repeated_option_ordering(&as_strings);
}

fn include_dirs_field() -> jefe::domain::agent_definition::Field {
    jefe::domain::agent_definition::Field {
        id: "include_dirs".to_string(),
        kind: jefe::domain::agent_definition::FieldKind::StringList,
        required: false,
        default: None,
        minimum: None,
        maximum: None,
        choices: Vec::new(),
        visible_when: None,
        launch_signature: false,
    }
}

fn argv_strings(plan: &jefe::domain::agent_definition::AgentLaunchPlan) -> Vec<String> {
    plan.argv
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

fn assert_repeated_option_ordering(as_strings: &[String]) {
    let include_count = as_strings.iter().filter(|s| *s == "--include").count();
    assert_eq!(include_count, 3, "one --include per list element");
    let include_values: Vec<&String> = as_strings
        .iter()
        .filter(|s| matches!(s.as_str(), "a" | "b" | "c"))
        .collect();
    assert_eq!(
        include_values,
        vec![&"a".to_string(), &"b".to_string(), &"c".to_string()],
    );
    let prompt_pos = as_strings
        .iter()
        .position(|s| s == "hi")
        .unwrap_or_else(|| panic!("positional emitted; got {as_strings:?}"));
    let first_include_pos = as_strings
        .iter()
        .position(|s| s == "--include")
        .unwrap_or_else(|| panic!("repeated option emitted; got {as_strings:?}"));
    assert!(
        prompt_pos < first_include_pos,
        "positional precedes the pushed repeated option in declaration order"
    );
}
