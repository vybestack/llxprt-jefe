//! Executable JSP/1 compliance framework (issue 477).
//!
//! This module is a pure, deterministic compliance oracle built on the frozen
//! JSP/1 wire contract from issue 476. It depends only on the validated
//! `domain::observation` types, the existing typed JSP parser, and the
//! standard library plus existing `serde`/`serde_json`. It performs no I/O of
//! its own; the CLI boundary reads fixture files and feeds bytes to these
//! pure functions.
//!
//! Architecture (each submodule has a single responsibility):
//! - [`projection`] owns the normalized client projection and observation
//!   health state.
//! - [`reducer`] owns the deterministic reference reducer that applies
//!   snapshots, events, heartbeats, gaps, and reconnects under current-state
//!   JSP/1 semantics (no replay/history/resume/resync).
//! - [`scenario`] owns scenario manifest loading and the per-step oracle.
//! - [`schema`] owns the schema-artifact completeness oracle.
//! - [`profile`] owns the producer and server profile validators over
//!   language-neutral adapter traces.
//! - [`report`] owns the stable machine-readable failure report.
//!
//! Current-state semantics (issue 477 decision D1 = current-state JSP/1):
//! a stream always begins with a snapshot; events increase by exactly one; a
//! detected gap, an epoch change, or a reconnect requires a fresh
//! snapshot-first stream. There is no replay buffer, cursor negotiation,
//! resume-after-N request, or `resync_required`.

mod adapter_invoker;
pub mod challenge;
mod dto;
mod harness;
pub mod profile;
mod profile_challenge;
pub mod projection;
pub mod reducer;
#[cfg(test)]
mod reducer_tests;
pub mod reference_adapter;
pub mod report;
pub mod scenario;
pub mod schema;
mod schema_utf8;
pub mod server_profile;
mod server_profile_request;
mod server_profile_stream;

pub use adapter_invoker::{
    AdapterInvocationError, AdapterOutput, invoke_adapter, run_reference_adapter,
};

/// Validate the top-level compliance manifest against
/// [`COMPLIANCE_ARTIFACT_VERSION`](report::COMPLIANCE_ARTIFACT_VERSION).
///
/// Checks the manifest's artifact version, schema/scenario/trace paths,
/// scenario count, and profile inventory. Returns an empty `Vec` if the
/// manifest is valid, or a `Vec<String>` of stable payload-free failure
/// reasons if it is not.
#[must_use]
pub fn validate_manifest_contents(manifest: &dto::ComplianceManifestWire) -> Vec<String> {
    const MAX_METADATA_STRING_BYTES: usize = 4096;
    const EXPECTED_PROFILES: [&str; 5] = ["schema", "reducer", "producer", "server", "all"];

    let mut errors = Vec::new();
    if manifest.schema != 1 {
        errors.push("manifest schema version mismatch".to_string());
    }
    if manifest.compliance_artifact_version != report::COMPLIANCE_ARTIFACT_VERSION {
        errors.push("manifest artifact version mismatch".to_string());
    }
    if manifest.description.len() > MAX_METADATA_STRING_BYTES {
        errors.push("manifest description exceeds bound".to_string());
    }
    if manifest.schemas.index != "schemas/manifest.json"
        || manifest.schemas.documents.snapshot != "schemas/snapshot.schema.json"
        || manifest.schemas.documents.event != "schemas/event.schema.json"
        || manifest.schemas.documents.heartbeat != "schemas/heartbeat.schema.json"
    {
        errors.push("manifest schema path inventory mismatch".to_string());
    }
    if manifest.scenarios.index != "scenarios/manifest.json" || manifest.scenarios.count != 15 {
        errors.push("manifest scenario count/index mismatch".to_string());
    }
    if manifest.traces.producer.contract != "producer-contract.md"
        || manifest.traces.producer.trace != "traces/producer-trace.json"
        || manifest.traces.server.contract != "server-contract.md"
        || manifest.traces.server.trace != "traces/server-transcript.json"
    {
        errors.push("manifest trace path inventory mismatch".to_string());
    }
    let actual_profiles: Vec<&str> = manifest
        .profiles
        .iter()
        .map(|profile| match profile {
            dto::ProfileWire::Schema => "schema",
            dto::ProfileWire::Reducer => "reducer",
            dto::ProfileWire::Producer => "producer",
            dto::ProfileWire::Server => "server",
            dto::ProfileWire::All => "all",
        })
        .collect();
    if actual_profiles != EXPECTED_PROFILES {
        errors.push("manifest profile inventory mismatch".to_string());
    }
    errors
}

/// Deserialize a top-level compliance manifest from raw bytes.
///
/// # Errors
/// Returns a stable payload-free error code if the bytes are not valid JSON
/// or do not match the closed manifest shape.
pub fn parse_compliance_manifest(
    bytes: &[u8],
) -> Result<dto::ComplianceManifestWire, &'static str> {
    serde_json::from_slice(bytes).map_err(|_| "JSP-C-MANIFEST-SHAPE")
}
