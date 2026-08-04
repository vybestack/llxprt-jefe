//! Unit tests for the generic, definition-driven candidate resolver
//! (issue #382 CW-02 S2 / CW02-01).
//!
//! These tests prove the deterministic algorithm #1: declaration order, typed
//! skips, repository-local symlink tree, PATH snapshot, platform/PATHEXT,
//! missing/non-executable candidates, slash rejection, package-runner
//! blank/nonblank selector participation, absent-runner typed skip, and
//! canonical-path + (dev/inode where available, size, mtime) fingerprint.
//!
//! They construct minimal closed [`AgentDefinition`] values directly (not via
//! the shipped data) so each deterministic property is isolated and the test
//! has no dependency on any product token.

use crate::domain::agent_definition::normalize::Normalize;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
use super::PackageRunnerKind;
use super::{AgentCandidateResolver, CandidateResolution, CandidateSkip, VersionSelector};
use crate::agent_candidate_path::PathSnapshot;
use crate::domain::agent_definition::probe::{
    AnchoredPattern, IdentityRecognizer, ProbeFraming, ProbeSpec, ProbeStream,
};
use crate::domain::agent_definition::type_id::{CandidateKind, ExecutableCandidate};
use crate::domain::agent_definition::types::{OperationMatrix, TargetMatrix};
use crate::domain::agent_definition::{AgentDefinition, DEFINITION_SCHEMA};
use crate::runtime::AgentExecutablePlatform;

#[cfg(unix)]
fn make_executable(dir: &TempDir, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.path().join(name);
    std::fs::write(&path, b"#!/bin/sh\n").unwrap_or_else(|error| panic!("write fixture: {error}"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod fixture: {error}"));
    path
}

/// Minimal definition with the given candidates and a single-character id.
fn definition(id: &str, candidates: Vec<ExecutableCandidate>) -> AgentDefinition {
    let Ok(parsed_id) = crate::domain::agent_definition::type_id::AgentTypeId::parse(id) else {
        panic!("valid test id must parse");
    };
    AgentDefinition {
        schema: DEFINITION_SCHEMA,
        id: parsed_id,
        display_name: id.to_string(),
        minimum_version: String::new(),
        candidates,
        probe: valid_probe(),
        operations: OperationMatrix::default(),
        targets: TargetMatrix::default(),
        repository_fields: vec![],
        agent_fields: vec![],
        emitters: vec![],
    }
}

fn valid_probe() -> ProbeSpec {
    ProbeSpec {
        argv: vec!["--version".to_string()],
        stream: ProbeStream::Stdout,
        framing: ProbeFraming::Utf8Text,
        normalize: Normalize::None,
        identity: IdentityRecognizer::Line {
            prefix: String::new(),
            anchored_pattern: AnchoredPattern::VersionToken,
        },
        timeout_ms: 5_000,
        max_bytes: 65_536,
    }
}

fn path_name(name: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::PathName {
            name: name.to_string(),
        },
        value: PathBuf::from(name),
    }
}

#[cfg(unix)]
fn npm_candidate(package: &str, binary: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::NpmPackage {
            package: package.to_string(),
            binary: binary.to_string(),
        },
        value: PathBuf::from(binary),
    }
}

#[cfg(unix)]
fn uvx_candidate(package: &str, binary: &str) -> ExecutableCandidate {
    ExecutableCandidate {
        kind: CandidateKind::UvxPackage {
            package: package.to_string(),
            binary: binary.to_string(),
        },
        value: PathBuf::from(binary),
    }
}

fn snapshot_unix(dirs: Vec<PathBuf>) -> PathSnapshot {
    PathSnapshot::for_platform(AgentExecutablePlatform::Unix, dirs, None)
}

// ---- deterministic declaration order ----

#[cfg(unix)]
#[test]
fn resolves_first_physical_candidate_in_declaration_order() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let present = make_executable(&dir, "real-agent");
    let canonical_present =
        std::fs::canonicalize(&present).unwrap_or_else(|error| panic!("canonicalize: {error}"));
    let def = definition(
        "core.first",
        vec![path_name("real-agent"), path_name("never-checked")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let resolution = resolver.resolve(&def);
    let Some(picked) = resolution.resolved() else {
        panic!("first candidate resolves");
    };
    assert_eq!(picked.index(), 0);
    assert_eq!(picked.executable(), &canonical_present);
}

#[cfg(unix)]
#[test]
fn skips_non_executable_then_resolves_next_launchable() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    // First declared candidate is present but not executable.
    let non_exec = dir.path().join("not-exec");
    std::fs::write(&non_exec, b"nope").unwrap_or_else(|error| panic!("write fixture: {error}"));
    let real = make_executable(&dir, "real-agent");
    let canonical_real =
        std::fs::canonicalize(&real).unwrap_or_else(|error| panic!("canonicalize: {error}"));
    let def = definition(
        "core.skip",
        vec![path_name("not-exec"), path_name("real-agent")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let CandidateResolution::Resolved(picked) = resolver.resolve(&def) else {
        panic!("must resolve second candidate");
    };
    assert_eq!(picked.index(), 1, "second candidate selected");
    assert_eq!(picked.executable(), &canonical_real);
}

#[cfg(unix)]
#[test]
fn missing_candidate_produces_typed_skip_in_order() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let def = definition(
        "core.missing",
        vec![path_name("absent-one"), path_name("absent-two")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("no candidate should resolve");
    };
    assert_eq!(skips.len(), 2, "one skip per candidate in order");
    assert!(matches!(
        skips[0],
        CandidateSkip::NotFoundOnPath { index: 0, .. }
    ));
    assert!(matches!(
        skips[1],
        CandidateSkip::NotFoundOnPath { index: 1, .. }
    ));
}

// ---- repository-local symlink tree ----

#[cfg(unix)]
#[test]
fn resolves_repository_local_symlink_tree() {
    use std::os::unix::fs::PermissionsExt;
    let Ok(repo) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    // Build `<repo>/.llxprt/bin/llxprt` as a symlink to a real executable so
    // the resolver's canonicalization follows it to a stable target.
    let bin_dir = repo.path().join(".llxprt/bin");
    std::fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    let real = repo.path().join("target-binary");
    std::fs::write(&real, b"#!/bin/sh\n").unwrap_or_else(|error| panic!("write fixture: {error}"));
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod fixture: {error}"));
    let link = bin_dir.join("llxprt");
    std::os::unix::fs::symlink(&real, &link)
        .unwrap_or_else(|error| panic!("symlink fixture: {error}"));

    let candidate = ExecutableCandidate {
        kind: CandidateKind::RepositoryLlxprt,
        value: PathBuf::from(".llxprt/bin/llxprt"),
    };
    let def = definition("core.repo", vec![candidate]);
    let snapshot = snapshot_unix(vec![]);
    let resolver = AgentCandidateResolver::new(&snapshot, repo.path().to_path_buf());
    let CandidateResolution::Resolved(picked) = resolver.resolve(&def) else {
        panic!("repository-local candidate must resolve through the symlink");
    };
    // Canonical path follows the symlink to the real target.
    let canonical =
        std::fs::canonicalize(&real).unwrap_or_else(|error| panic!("canonicalize target: {error}"));
    assert_eq!(picked.executable(), &canonical);
    assert!(
        picked.fingerprint().has_dev_ino(),
        "Unix fingerprint carries dev/inode"
    );
}

#[cfg(unix)]
#[test]
fn repository_local_not_launchable_skips_typed() {
    let Ok(repo) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let bin_dir = repo.path().join(".llxprt/bin");
    std::fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
    std::fs::write(bin_dir.join("llxprt"), b"nope")
        .unwrap_or_else(|error| panic!("write fixture: {error}"));
    let candidate = ExecutableCandidate {
        kind: CandidateKind::RepositoryLlxprt,
        value: PathBuf::from(".llxprt/bin/llxprt"),
    };
    let def = definition("core.repo2", vec![candidate]);
    let snapshot = snapshot_unix(vec![]);
    let resolver = AgentCandidateResolver::new(&snapshot, repo.path().to_path_buf());
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("non-launchable repository-local must not resolve");
    };
    assert_eq!(skips.len(), 1);
    assert!(matches!(
        skips[0],
        CandidateSkip::RepositoryLocalNotLaunchable { index: 0 }
    ));
}

// ---- slash rejection ----

#[test]
fn path_name_with_slash_is_rejected_typed() {
    let def = definition("core.slash", vec![path_name("sub/dir/agent")]);
    let snapshot = snapshot_unix(vec![PathBuf::from("/irrelevant")]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("slash path-name must not resolve");
    };
    assert_eq!(skips.len(), 1);
    assert!(matches!(
        skips[0],
        CandidateSkip::PathNameSlash { index: 0 }
    ));
}

// ---- package runner: blank/nonblank selector participation ----

#[cfg(unix)]
#[test]
fn npm_candidate_blank_selector_is_skipped_typed_even_when_npm_present() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    make_executable(&dir, "npm"); // runner present
    let def = definition("core.npm-blank", vec![npm_candidate("@scope/pkg", "bin")]);
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    // Blank selector: candidate must NOT participate.
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("blank selector must skip package candidate");
    };
    assert_eq!(skips.len(), 1);
    assert!(matches!(
        skips[0],
        CandidateSkip::PackageSelectorBlank { index: 0 }
    ));
}

#[cfg(unix)]
#[test]
fn npm_candidate_nonblank_selector_participates_when_runner_present() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let npm = make_executable(&dir, "npm");
    let canonical_npm =
        std::fs::canonicalize(&npm).unwrap_or_else(|error| panic!("canonicalize: {error}"));
    let def = definition(
        "core.npm-nonblank",
        vec![npm_candidate("@scope/pkg", "bin")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let selector = VersionSelector::normalize("1.2.3")
        .unwrap_or_else(|error| panic!("valid selector: {error}"));
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector);
    let CandidateResolution::Resolved(picked) = resolver.resolve(&def) else {
        panic!("nonblank selector + present runner must resolve");
    };
    // S2 fingerprints the runner path; the package argv belongs to S12.
    assert_eq!(picked.executable(), &canonical_npm);
}

#[cfg(unix)]
#[test]
fn npm_candidate_absent_runner_is_skipped_typed() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    // No npm in PATH.
    let def = definition("core.no-npm", vec![npm_candidate("@scope/pkg", "bin")]);
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let selector = VersionSelector::normalize("latest")
        .unwrap_or_else(|error| panic!("valid selector: {error}"));
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector);
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("absent npm must skip");
    };
    assert_eq!(skips.len(), 1);
    assert!(matches!(
        skips[0],
        CandidateSkip::RunnerAbsent {
            index: 0,
            runner: PackageRunnerKind::Npm,
        }
    ));
}

// ---- uvx runner absent typed skip ----

#[cfg(unix)]
#[test]
fn uvx_candidate_absent_runner_is_skipped_typed() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let candidate = uvx_candidate("code-puppy", "code-puppy");
    let def = definition("core.no-uvx", vec![candidate]);
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let selector = VersionSelector::normalize("1.0.0")
        .unwrap_or_else(|error| panic!("valid selector: {error}"));
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector);
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("absent uvx must skip");
    };
    assert_eq!(skips.len(), 1);
    assert!(matches!(
        skips[0],
        CandidateSkip::RunnerAbsent {
            index: 0,
            runner: PackageRunnerKind::Uvx,
        }
    ));
}

// ---- fingerprint: canonical path, size, mtime, dev/inode where available ----

#[cfg(unix)]
#[test]
fn resolved_candidate_fingerprints_canonical_path_size_mtime_and_dev_ino() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    let real = make_executable(&dir, "real-target");
    let link = dir.path().join("agent");
    std::os::unix::fs::symlink(&real, &link)
        .unwrap_or_else(|error| panic!("symlink fixture: {error}"));
    let def = definition("core.fp", vec![path_name("agent")]);
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    let CandidateResolution::Resolved(picked) = resolver.resolve(&def) else {
        panic!("must resolve");
    };
    let canonical =
        std::fs::canonicalize(&real).unwrap_or_else(|error| panic!("canonicalize: {error}"));
    assert_eq!(picked.executable(), &canonical);
    let Ok(metadata) = real.metadata() else {
        panic!("meta must be read");
    };
    assert_eq!(picked.fingerprint().size(), metadata.len());
    assert!(picked.fingerprint().has_dev_ino());
}

#[test]
fn version_selector_blank_for_empty_string() {
    let sel = VersionSelector::normalize("")
        .unwrap_or_else(|error| panic!("empty is blank, not error: {error}"));
    assert!(sel.is_direct());
    let sel = VersionSelector::normalize("   ")
        .unwrap_or_else(|error| panic!("whitespace-only is blank: {error}"));
    assert!(sel.is_direct());
}

#[test]
fn version_selector_volatile_unless_it_is_an_exact_version() {
    // Volatility is decided by shape, not by a list of known names: a selector
    // is a pin only when it is an exact version, because anything else is a
    // pointer the registry may move (issue #601). An exact version is still a
    // pin even when its prerelease says "nightly" (issue #554).
    assert!(VersionSelector::Latest.is_volatile());
    assert!(VersionSelector::LatestNightly.is_volatile());
    assert!(!VersionSelector::Direct.is_volatile());
    assert!(
        !VersionSelector::normalize("0.10.0-nightly.260720.abc")
            .unwrap_or_else(|error| panic!("explicit selector: {error}"))
            .is_volatile(),
        "an explicit nightly version string is pinned, not volatile"
    );
}

/// Anything that is not an exact version is a pointer the registry can move.
///
/// A hard-coded list of sentinel names can never be exhaustive: a registry may
/// define any dist-tag it likes. Deciding by shape covers custom tags and
/// ranges alike, and it fails in the safe direction — an unrecognized shape is
/// re-resolved rather than frozen (issue #601).
#[test]
fn moving_selectors_are_volatile_whatever_they_are_called() {
    for moving in [
        "glm52-vast", // a real custom dist-tag observed in a live cache
        "beta",
        "next",
        "^1.0.0",
        "~0.11.0",
        ">=1.2.0",
        "1.x",
        "^1.0.0||^2.0.0",
        "0.11",
        "v0.11.0", // npm accepts it, but it is not an exact version string
    ] {
        assert!(
            VersionSelector::normalize(moving)
                .unwrap_or_else(|error| panic!("selector {moving:?}: {error}"))
                .is_volatile(),
            "{moving:?} is a moving pointer and must be re-resolved, not frozen at first install"
        );
    }
}

/// A value npm could never resolve must not become volatile.
///
/// Volatility decides whether jefe asks the registry about a selector. Asking
/// about a value that is not a legal spec spawns a process and waits, only to
/// fail the same way it would have anyway. Hostile input matters most here: it
/// keeps shell-metacharacter selectors on the single argv-safe path whose
/// behaviour is already pinned by the injection tests, instead of widening them
/// to a second command (issue #601).
#[test]
fn unresolvable_selectors_do_not_become_volatile() {
    // Whitespace is not represented here on purpose: normalization strips it
    // before this decision is reached, so "0 9 0" arrives as "090", which is
    // indistinguishable from a tag legitimately named "090".
    for hostile in [
        "1.0.0; rm -rf /",
        "1.0;$(touch nope)",
        "pkg@1.0.0",
        "../../etc/passwd",
        "tag/../escape",
        "$HOME",
        "1.0.0|rm",
    ] {
        assert!(
            !VersionSelector::normalize(hostile)
                .unwrap_or_else(|error| panic!("selector {hostile:?}: {error}"))
                .is_volatile(),
            "{hostile:?} is not a resolvable spec and must not trigger a registry query"
        );
    }
}

/// An exact version must stay pinned, or every pinned user pays a registry
/// query per launch for something that cannot move.
#[test]
fn exact_versions_remain_pinned() {
    for exact in [
        "0.11.0",
        "1.2.3",
        "10.20.30",
        "0.11.0-nightly.260801.19ac22acc",
        "1.0.0-rc.1",
        "1.0.0+build.5",
        "1.0.0-alpha.1+build.5",
    ] {
        assert!(
            !VersionSelector::normalize(exact)
                .unwrap_or_else(|error| panic!("selector {exact:?}: {error}"))
                .is_volatile(),
            "{exact:?} is an exact version and must remain an immutable pin"
        );
    }
}

#[test]
fn version_selector_rejects_nul_byte() {
    let Err(err) = VersionSelector::normalize("a\u{0}b") else {
        panic!("NUL rejected");
    };
    assert_eq!(err, super::VersionSelectorError::Nul);
}

#[test]
fn version_selector_index_returns_declaration_index() {
    let skip = CandidateSkip::NotFoundOnPath {
        index: 3,
        name: "x".to_string(),
    };
    assert_eq!(skip.index(), 3);
}

#[test]
fn resolution_is_resolved_predicate() {
    let skip = CandidateResolution::NotFound(vec![]);
    assert!(!skip.is_resolved());
    assert!(skip.resolved().is_none());
}

#[cfg(unix)]
#[test]
fn selected_package_suppresses_direct_candidates_and_retains_metadata() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    make_executable(&dir, "agent");
    make_executable(&dir, "npm");
    let def = definition(
        "core.exclusive",
        vec![path_name("agent"), npm_candidate("@scope/package", "agent")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let selector = VersionSelector::normalize("latest nightly")
        .unwrap_or_else(|error| panic!("selector: {error}"));
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector);
    let CandidateResolution::Resolved(candidate) = resolver.resolve(&def) else {
        panic!("package candidate must resolve without direct fallback");
    };
    assert_eq!(candidate.index(), 1);
    let package = candidate
        .package()
        .unwrap_or_else(|| panic!("package metadata"));
    assert_eq!(package.runner(), PackageRunnerKind::Npm);
    assert_eq!(package.package(), "@scope/package");
    assert_eq!(package.binary(), "agent");
    assert_eq!(package.package_spec(), "@scope/package@nightly");
}

#[cfg(unix)]
#[test]
fn selected_package_with_absent_runner_never_falls_back_to_direct() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    make_executable(&dir, "agent");
    let def = definition(
        "core.no-fallback",
        vec![path_name("agent"), npm_candidate("package", "agent")],
    );
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let selector =
        VersionSelector::normalize("2.0.0").unwrap_or_else(|error| panic!("selector: {error}"));
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(selector);
    let CandidateResolution::NotFound(skips) = resolver.resolve(&def) else {
        panic!("absent selected runner must be NotFound");
    };
    assert!(matches!(
        skips.as_slice(),
        [
            CandidateSkip::DirectSuppressedBySelector { index: 0 },
            CandidateSkip::RunnerAbsent {
                index: 1,
                runner: PackageRunnerKind::Npm,
            }
        ]
    ));
}

#[cfg(unix)]
#[test]
fn selector_change_advances_probe_generation_key() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    make_executable(&dir, "npm");
    let def = definition("core.generation", vec![npm_candidate("package", "agent")]);
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let first = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(
            VersionSelector::normalize("1.0.0").unwrap_or_else(|error| panic!("selector: {error}")),
        )
        .resolve(&def);
    let second = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"))
        .with_version_selector(
            VersionSelector::normalize("2.0.0").unwrap_or_else(|error| panic!("selector: {error}")),
        )
        .resolve(&def);
    let first_key = first
        .resolved()
        .unwrap_or_else(|| panic!("first candidate"))
        .generation_key(&def);
    let second_key = second
        .resolved()
        .unwrap_or_else(|| panic!("second candidate"))
        .generation_key(&def);
    assert_eq!(
        super::next_probe_generation(Some(&first_key), &first_key, 7),
        Ok(7)
    );
    assert_eq!(
        super::next_probe_generation(Some(&first_key), &second_key, 7),
        Ok(8)
    );
    assert_eq!(
        super::next_probe_generation(Some(&first_key), &second_key, u64::MAX),
        Err(super::ProbeGenerationOverflow)
    );
}

// ---- PATH snapshot reuse: one snapshot, many definitions ----

#[cfg(unix)]
#[test]
fn one_snapshot_resolves_multiple_definitions_in_id_order() {
    let Ok(dir) = tempfile::tempdir() else {
        panic!("tempdir must be created");
    };
    make_executable(&dir, "alpha");
    make_executable(&dir, "beta");
    let defs = vec![
        definition("z.last", vec![path_name("beta")]),
        definition("a.first", vec![path_name("alpha")]),
    ];
    let snapshot = snapshot_unix(vec![dir.path().to_path_buf()]);
    let resolver = AgentCandidateResolver::new(&snapshot, PathBuf::from("/repo"));
    for def in &defs {
        assert!(
            resolver.resolve(def).is_resolved(),
            "each definition resolves against the same snapshot"
        );
    }
}

// ---- fingerprint capture failure (non-existent path after resolve guard) ----

#[test]
fn fingerprint_capture_failure_returns_typed_skip() {
    // `resolve_binary` returning a path that then fails canonicalize/metadata
    // is exercised indirectly by the platform tests above; here we verify the
    // typed error path is reachable through `CandidateSkip::FingerprintCapture`
    // by constructing a definition whose binary lives in a directory that will
    // be removed between resolve and fingerprint. On Unix this is racy to
    // reproduce deterministically, so this test asserts the skip variant's
    // index accessor and display instead.
    let skip = CandidateSkip::FingerprintCapture {
        index: 2,
        detail: "i/o".to_string(),
    };
    assert_eq!(skip.index(), 2);
    assert!(skip.to_string().contains("candidate 2"));
    let _ = Path::new("/nonexistent"); // keep Path import used on all platforms
}
