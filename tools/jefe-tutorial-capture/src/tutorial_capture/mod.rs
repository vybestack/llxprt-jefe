//! Tutorial-capture workflow: reusable agent-driven documentation capture.
//!
//! This module builds on the existing tmux harness to provide a safe,
//! repeatable documentation-capture workflow. It adds:
//!
//! - **Manifest**: typed ownership tracking for every resource a run creates.
//! - **Path shims**: controlled runtime detection via run-scoped PATH shims.
//! - **Allowlist**: explicit fixture repository safety with production-repo
//!   refusal.
//! - **Redaction**: scrub credentials and personal data from artifacts.
//! - **Report**: Markdown evidence report for documentation authors.
//! - **Orchestration**: setup, teardown, and manifest-scoped cleanup.
//!
//! ## Architecture boundaries
//!
//! - `manifest`, `path_shim`, `allowlist`, `redaction`, `report` are pure
//!   data/planning layers with no I/O.
//! - `orchestration` owns filesystem setup/teardown and delegates pure
//!   decisions to the above modules.
//! - The CLI binary (`jefe-tutorial-capture`) composes orchestration with the
//!   harness runner.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-001

pub mod allowlist;
pub mod ansi_svg;
pub mod github_cleanup;
pub mod github_executor;
pub mod manifest;
pub mod orchestration;
pub mod path_shim;
pub mod persistence;
pub mod provenance;
pub mod redaction;
pub mod report;
pub mod scenario_gen;
pub mod state_seed;
pub mod svg_render;
pub mod validate_runtime;

pub use allowlist::{
    AllowlistBuildError, AllowlistDecision, FixtureAllowlist, FixtureMutationPlan,
    build_allowlist_from_sources, build_allowlist_from_sources_checked, build_mutation_plan,
    is_valid_repo_format,
};
pub use github_executor::{
    CommandRunner, GithubCleanupOutcome, GithubCleanupStatus, ParsedGitHubUrl, PlannedCommand,
    RealCommandRunner, TierBContext, TierBError, TierBPlan, TierBScenarioParams,
    TierBValidationError, execute_github_cleanup, execute_github_cleanup_with_allowlist,
    execute_tier_b, extract_scenario_params, generate_tier_b_merge_scenario,
    generate_tier_b_scenario, parse_github_resource_url, plan_github_cleanup, plan_tier_b,
    validate_clone_destination, validate_gh_target, validate_tier_b_resources,
};
pub use manifest::{
    ArtifactEntry, ArtifactKind, GitHubResource, GitHubResourceKind, ManifestError, ObservedAction,
    OwnedPath, OwnedPathKind, RunId, RunManifest, RunOutcome, RuntimeProfile,
};
pub use orchestration::{
    OrchestrationError, RunDirectories, RunSetup, check_fixture_repo, cleanup_manifest,
    cleanup_manifest_with_root, collect_tool_versions, compute_binary_hash, compute_scenario_hash,
    controlled_path_for, detection_path_for, load_manifest, plan_real_runtime_link_for,
    plan_system_tool_links_for, prepare_run, redact_artifacts, redact_artifacts_with_repos,
    save_manifest, save_report,
};
pub use path_shim::{
    PlannedShim, SHIM_MARKER, ShimAvailability, ShimError, SystemToolLink,
    check_tier_a_required_tools, check_tier_b_required_tools, controlled_path, detection_path,
    deterministic_shim, plan_real_runtime_link, plan_shims, plan_system_tool_links,
    validate_real_runtime, which,
};
pub use persistence::{
    CleanupOutcome, CleanupRecord, PersistenceError, cleanup_with_containment,
    create_run_root_exclusive, create_run_root_with_run_id, load_and_validate,
    save_manifest_atomic, validate_artifact_path, verify_sentinel_ownership, write_artifact_atomic,
};
pub use redaction::{
    RedactionRule, RedactionSet, add_privacy_rules, build_redaction_set,
    build_redaction_set_with_repos, common_redactions, redact_line,
};
pub use report::render_report;
pub use state_seed::{
    SeededState, StateSeedError, TierBStateSeed, derive_agent_kind, seed_tier_b_state,
};
pub use svg_render::{SvgRenderMetadata, render_screen_svg, svg_geometry};

pub use ansi_svg::{ColorSvgMetadata, color_svg_geometry, render_color_svg};

pub use validate_runtime::{
    generate_validate_runtime_scenario, prepare_validate_runtime_scenario, runtime_binary_name,
    runtime_label,
};
