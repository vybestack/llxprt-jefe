//! Contract tests for issue #542 — the Windows session process tree must have
//! exactly one documented owner-lifetime anchor, and the code must reference
//! the document that defines it (V8).
//!
//! This class of defect has been "fixed" three times (#332, #467, #493) and
//! reopened four. Each fix moved the anchor without ever writing down what the
//! anchor *is*, so the next change moved it again. These tests make the model a
//! merged artefact rather than tribal knowledge: the document must name every
//! process in the tree, state who owns it and what its death means, and the
//! enforcing modules must point back at it.

use std::path::Path;

const MODEL_DOC: &str = "dev-docs/standards/windows-session-ownership.md";

/// V8: the ownership model is a merged design artefact, not just code.
#[test]
fn ownership_model_document_is_merged() {
    let doc = read_repo_text(MODEL_DOC);
    assert!(
        doc.len() > 1_000,
        "{MODEL_DOC} must actually describe the model, not stub it"
    );
}

/// Deliverable 1: every process in the tree, who owns it, what its death
/// means, and what must be reaped.
#[test]
fn ownership_model_names_every_process_in_the_tree() {
    let doc = read_repo_text(MODEL_DOC);
    for role in [
        "psmux server",
        "pane process",
        "session host",
        "worker",
        "dashboard",
    ] {
        assert!(
            doc.contains(role),
            "{MODEL_DOC} must state the ownership and death semantics of the \
             `{role}`; a model that omits a process in the tree is how #467 \
             contained the worker and left the host unowned"
        );
    }
}

/// Deliverable 2: the anchor is captured as an identity, before the spawn, and
/// the document must say so — a PID alone is spoofable by reuse (V5).
#[test]
fn ownership_model_states_the_identity_and_ordering_rules() {
    let doc = read_repo_text(MODEL_DOC);
    for rule in ["ProcessIdentity", "before", "PID reuse", "fail open"] {
        assert!(
            doc.contains(rule),
            "{MODEL_DOC} must state the `{rule}` rule; these are the three \
             ways this invariant has previously been lost (late capture, PID \
             spoofing, and converting uncertainty into termination)"
        );
    }
}

/// The issue's explicit non-goal: a periodic sweeper is defence in depth, never
/// the primary mechanism. The document must record that so a later change does
/// not quietly regress to janitorial cleanup.
#[test]
fn ownership_model_rejects_a_sweeper_as_the_primary_mechanism() {
    let doc = read_repo_text(MODEL_DOC);
    assert!(
        doc.contains("sweeper") || doc.contains("janitor"),
        "{MODEL_DOC} must explicitly record that a periodic sweeper is not the \
         ownership mechanism; the invariant has to hold without one"
    );
}

/// V8: "the code references it". A document nobody links to is a document
/// nobody maintains.
#[test]
fn enforcing_modules_reference_the_ownership_model() {
    for module in [
        "src/runtime/owner_anchor.rs",
        "src/runtime/agent_launcher.rs",
        "src/runtime/job_object.rs",
        "src/runtime/session_host.rs",
    ] {
        let source = read_repo_text(module);
        assert!(
            source.contains("windows-session-ownership.md"),
            "{module} enforces part of the ownership model and must reference \
             {MODEL_DOC} so the two cannot drift apart"
        );
    }
}

fn read_repo_text(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}
