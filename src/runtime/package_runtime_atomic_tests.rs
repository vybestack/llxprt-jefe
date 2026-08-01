// Issue #556 — atomic managed install: no partial tree at the final path, and
// an interrupted install is retried cleanly. These drive the production
// [`finalize_local_invocation_at`] boundary and reuse the package_runtime test
// helpers (`executable`, `definition`, `resolve_package`).

/// An npm stub that fails on its first invocation and succeeds on the second,
/// recording each call's working directory one level up so the witness survives
/// the atomic rebuild.
fn failing_then_succeeding_npm_stub(bin: &TempDir) {
    executable(
        bin,
        "npm",
        "#!/bin/sh
set -e
count=../.jefe-npm-count
n=$(cat \"$count\" 2>/dev/null || echo 0)
n=$((n + 1))
echo \"$n\" > \"$count\"
if [ \"$n\" = 1 ]; then
  exit 1
fi
mkdir -p node_modules/.bin
printf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
",
    );
}

/// An npm stub that succeeds, recording the working directory it ran in so the
/// test can prove the install built in a sibling temp dir, not the final path.
fn pwd_recording_npm_stub(bin: &TempDir) {
    executable(
        bin,
        "npm",
        "#!/bin/sh
set -e
pwd >> ../.jefe-npm-pwd
mkdir -p node_modules/.bin
printf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
",
    );
}

fn npm_pinned_candidate(bin: &TempDir, selector: &str) -> ResolvedCandidate {
    let definition = definition("LLxprt");
    resolve_package(&definition, bin, selector)
}

/// A2 + A3: an install that fails mid-build leaves nothing at the final path
/// (no marker, no partial directory), and a subsequent install retries cleanly.
#[test]
fn atomic_install_leaves_no_partial_tree_after_failure_and_retries_cleanly() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    failing_then_succeeding_npm_stub(&bin);
    let candidate = npm_pinned_candidate(&bin, "1.2.3");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let install_dir = managed_install_dir(
        cache.path(),
        candidate
            .package()
            .unwrap_or_else(|| panic!("managed candidate carries a package selection")),
    );

    // First attempt fails mid-install (npm exits non-zero).
    let first = finalize_local_invocation_at(&candidate, cache.path(), base_now());
    assert!(
        matches!(first, Err(PackageRuntimeError::InstallFailed(_))),
        "a mid-install failure surfaces InstallFailed: {first:?}"
    );
    assert!(
        !install_dir.exists(),
        "A2/A3: a failed install leaves no directory at the final path"
    );
    assert!(
        !install_dir.join(super::INSTALL_MARKER).exists(),
        "A3: no cache-hit marker is written for a failed install"
    );

    // A failed install also leaves no orphaned building temp behind (the temp is
    // removed on the error path so failed installs do not accumulate).
    let digest = super::selection_digest(
        candidate
            .package()
            .unwrap_or_else(|| panic!("managed candidate carries a package selection")),
    )
    .to_hex();
    let orphaned = std::fs::read_dir(cache.path()).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!(".{digest}.building-"))
        })
    });
    assert!(
        !orphaned,
        "A3: a failed install cleans up its sibling building temp"
    );

    // Retry succeeds: the final tree is complete (marker + binary).
    let resolved = finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("retry install: {error}"));
    assert!(
        install_dir.exists(),
        "the retried install publishes the final tree"
    );
    assert!(
        install_dir.join(super::INSTALL_MARKER).exists(),
        "the retried install writes the completion marker"
    );
    assert!(
        resolved.executable().starts_with(install_dir),
        "the resolved binary lives under the final install path"
    );
}

/// A2: a successful install builds in a sibling temporary directory and is
/// renamed into place, so npm never writes to the final path and no building
/// temp is left behind.
#[test]
fn atomic_install_builds_in_a_sibling_temp_then_appears_complete() {
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    pwd_recording_npm_stub(&bin);
    let candidate = npm_pinned_candidate(&bin, "2.0.0");
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let install_dir = managed_install_dir(
        cache.path(),
        candidate
            .package()
            .unwrap_or_else(|| panic!("managed candidate carries a package selection")),
    );

    finalize_local_invocation_at(&candidate, cache.path(), base_now())
        .unwrap_or_else(|error| panic!("install: {error}"));

    // npm ran inside a `.building-*` sibling, never the final digest directory.
    let pwd = std::fs::read_to_string(cache.path().join(".jefe-npm-pwd"))
        .unwrap_or_else(|error| panic!("read npm pwd witness: {error}"));
    let npm_cwd = pwd
        .lines()
        .next()
        .unwrap_or_else(|| panic!("npm recorded its working dir: {pwd:?}"));
    assert!(
        std::path::Path::new(npm_cwd)
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".building-")),
        "A2: npm builds in a sibling temp dir, not the final path: {npm_cwd:?}"
    );

    // The final tree is complete and no building temp lingers.
    assert!(
        install_dir.join(super::INSTALL_MARKER).exists(),
        "the final tree carries the completion marker"
    );
    let digest = super::selection_digest(
        candidate
            .package()
            .unwrap_or_else(|| panic!("managed candidate carries a package selection")),
    )
    .to_hex();
    let leftover = std::fs::read_dir(cache.path()).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with(&format!(".{digest}.building-")))
    });
    assert!(
        !leftover,
        "A2: no sibling building temp remains after a successful swap"
    );
}
