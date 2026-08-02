// Issue #556: cross-process serialization and atomic promotion of the managed
// package install cache.
//
// Included into `package_runtime::tests`; the helpers (`executable`,
// `definition`, `resolve_package`, `base_now`, …) come from
// `package_runtime_tests.rs`.
//
// # Two-process protocol
//
// Serialization cannot be proved from one process, so the contended test
// re-executes the test binary — the repository's established way to obtain a
// real second OS process (`process_tests.rs`, `tests/harness_v1.rs`) — with a
// `--exact` filter naming `managed_install_child_process`. That test is inert
// unless the four `CHILD_*` environment variables are present, so an ordinary
// run executes it as a no-op.
//
// Each child is given a cache root, a directory holding the npm stub, a report
// path, and a barrier directory. It publishes `ready-<report file name>` into
// the barrier directory, blocks until the parent publishes `start`, runs the
// production preparation boundary, and writes a single tab-separated report
// line: `ok	<resolved executable>` or `err	<diagnostic>`.
//
// The barrier is what makes the test deterministic: both children are proved to
// be inside preparation before either is released, so the result does not
// depend on spawn timing or machine load. `InstallerProcesses` owns the
// children and kills and reaps them on drop, so a panicking test cannot leave
// an installer running.

/// Environment channel used to run one test-binary process as a second, real
/// jefe installer. `std::env::current_exe` re-execution is the repository's
/// established way to obtain a genuine second OS process from a test
/// (`process_tests.rs`, `tests/harness_v1.rs`); it needs no extra `[[bin]]`.
#[cfg(unix)]
const CHILD_CACHE_ENV: &str = "JEFE_TEST_INSTALL_CHILD_CACHE";
#[cfg(unix)]
const CHILD_BIN_ENV: &str = "JEFE_TEST_INSTALL_CHILD_BIN";
#[cfg(unix)]
const CHILD_OUT_ENV: &str = "JEFE_TEST_INSTALL_CHILD_OUT";
/// Directory holding the start barrier that makes the two installers overlap.
#[cfg(unix)]
const CHILD_BARRIER_ENV: &str = "JEFE_TEST_INSTALL_CHILD_BARRIER";
#[cfg(unix)]
const CHILD_TEST_PATH: &str = "runtime::package_runtime::tests::managed_install_child_process";

use crate::runtime::commands::shell_escape_single;

/// Version the observing stub reports for a volatile fixture unless a test
/// moves the tag. The install directory is keyed on the resolved version
/// (issue #588), so fixtures must derive paths from it.
#[cfg(unix)]
const SEEDED_VERSION: &str = "1.0.0";

/// Resolved version a fixture's install will be keyed on: the seeded version
/// for a moving dist-tag, and `None` for a pinned selector, which is already a
/// concrete version and keys on itself.
#[cfg(unix)]
fn fixture_resolved(candidate: &ResolvedCandidate) -> Option<&'static str> {
    candidate
        .package()
        .is_some_and(|selection| selection.selector().is_volatile())
        .then_some(SEEDED_VERSION)
}

/// Published install directory for a resolved managed candidate.
#[cfg(unix)]
fn final_install_dir(candidate: &ResolvedCandidate, cache_root: &Path) -> PathBuf {
    let selection = candidate
        .package()
        .unwrap_or_else(|| panic!("candidate carries a package selection"));
    super::managed_install_dir(cache_root, selection, fixture_resolved(candidate))
}

/// Shell-safe single-quoted form of a path, so a stub script cannot be broken
/// by a quote or a space anywhere in the temporary directory.
#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    shell_escape_single(&path.to_string_lossy())
}

/// npm stub that builds a complete tree and then, *while still installing*,
/// records its own working directory and what a concurrent reader would see at
/// the published install directory.
///
/// Observing after the tree exists is what makes the observation meaningful:
/// an install that mutates the published directory in place is caught as
/// `final-partial` (tree present, marker not yet written).
#[cfg(unix)]
fn observing_npm_stub(bin: &TempDir, witness: &Path, final_dir: &Path, settle: &str) {
    executable(
        bin,
        "npm",
        &format!(
            "#!/bin/sh
set -e
final={final_dir}
versions={versions}
# A metadata-only resolve records no observation and installs nothing.
if [ \"$1\" = view ]; then
  if [ \"$#\" -ne 3 ] || [ \"$3\" != version ]; then
    echo \"unexpected npm view invocation: $*\" >&2
    exit 64
  fi
  if [ -f \"$versions\" ]; then cat \"$versions\"; exit 0; else exit 1; fi
fi
mkdir -p node_modules/.bin
printf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt
chmod 755 node_modules/.bin/llxprt
if [ -x \"$final/node_modules/.bin/llxprt\" ] && [ -f \"$final/.jefe-installed\" ]; then
  state=final-complete
elif [ -e \"$final/node_modules\" ] || [ -e \"$final/.jefe-installed\" ]; then
  state=final-partial
else
  state=final-absent
fi
printf '%s\\t%s\\n' \"$(pwd -P)\" \"$state\" >> {witness}
sleep {settle}
",
            final_dir = shell_quote(final_dir),
            versions = shell_quote(&observed_version_path(witness)),
            witness = shell_quote(witness),
            settle = shell_escape_single(settle)
        ),
    );
}

/// File the observing stub reads to answer `npm view ... version`.
#[cfg(unix)]
fn observed_version_path(witness: &Path) -> PathBuf {
    witness.with_file_name("published-version.txt")
}

/// Move the dist-tag so the next preparation re-resolves and reinstalls.
#[cfg(unix)]
fn publish_observed_version(witness: &Path, version: &str) {
    std::fs::write(observed_version_path(witness), format!("{version}\n"))
        .unwrap_or_else(|error| panic!("publish version: {error}"));
}

/// One `cwd\tstate` observation recorded by [`observing_npm_stub`].
#[cfg(unix)]
fn observations(witness: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(witness)
        .unwrap_or_else(|error| panic!("read install observations: {error}"))
        .lines()
        .map(|line| {
            let (cwd, state) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("malformed observation line: {line:?}"));
            (cwd.to_owned(), state.to_owned())
        })
        .collect()
}

/// Physical path of the published install directory, matching the stub's
/// `pwd -P` so a symlinked temp root (macOS `/var` → `/private/var`) cannot
/// make a same-directory install look like a staged one.
#[cfg(unix)]
fn physical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Resolve a managed candidate, install the observing npm stub over the probe
/// stub, and return the candidate plus its published install directory.
///
/// Resolution happens twice on purpose. The observing stub has to be told the
/// published install directory, which is only derivable from a resolved
/// candidate; and the returned candidate must carry a fingerprint of the npm
/// that actually runs, not of the placeholder it replaced.
#[cfg(unix)]
fn observed_candidate(
    bin: &TempDir,
    cache_root: &Path,
    witness: &Path,
    selector: &str,
    settle: &str,
) -> (ResolvedCandidate, PathBuf) {
    let definition = definition("LLxprt");
    executable(bin, "npm", "#!/bin/sh\nexit 0\n");
    let probe = resolve_package(&definition, bin.path(), selector);
    let final_dir = final_install_dir(&probe, cache_root);
    publish_observed_version(witness, SEEDED_VERSION);
    observing_npm_stub(bin, witness, &final_dir, settle);
    (resolve_package(&definition, bin.path(), selector), final_dir)
}

// ── A1: two OS processes, one install ─────────────────────────────────────

/// Second jefe process for [`concurrent_processes_install_once_and_agree`].
///
/// Inert unless the parent test supplies the channel environment, so a normal
/// `cargo test` run executes it as a no-op.
#[cfg(unix)]
#[test]
fn managed_install_child_process() {
    let (Some(cache), Some(bin), Some(out), Some(barrier)) = (
        std::env::var_os(CHILD_CACHE_ENV),
        std::env::var_os(CHILD_BIN_ENV),
        std::env::var_os(CHILD_OUT_ENV),
        std::env::var_os(CHILD_BARRIER_ENV),
    ) else {
        return;
    };
    let definition = definition("LLxprt");
    let candidate = resolve_package(&definition, Path::new(&bin), "2.0.0");
    // Announce readiness, then wait for the parent's start signal, so both
    // installers enter preparation together however the machine is loaded.
    let barrier = Path::new(&barrier);
    let out = Path::new(&out);
    let ready = barrier.join(format!(
        "ready-{}",
        out.file_name()
            .unwrap_or_else(|| panic!("child report has a file name"))
            .to_string_lossy()
    ));
    std::fs::write(&ready, "ready").unwrap_or_else(|error| panic!("publish readiness: {error}"));
    await_barrier(&barrier.join(START_MARKER));

    let report = match finalize_local_invocation_inner(&candidate, Path::new(&cache)) {
        Ok(invocation) => format!("ok\t{}\n", invocation.executable().display()),
        Err(error) => format!("err\t{error}\n"),
    };
    std::fs::write(out, report).unwrap_or_else(|error| panic!("write child report: {error}"));
}

/// Name of the parent-published start signal.
#[cfg(unix)]
const START_MARKER: &str = "start";

/// Block until `path` appears, failing rather than hanging forever.
#[cfg(unix)]
fn await_barrier(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out waiting for {}", path.display());
}

/// Installer children that are always reaped, even if the test panics.
#[cfg(unix)]
struct InstallerProcesses {
    children: Vec<(PathBuf, std::process::Child)>,
}

#[cfg(unix)]
impl InstallerProcesses {
    /// Start two installer processes, each parked on the start barrier.
    fn spawn_pair(
        bin: &TempDir,
        cache: &TempDir,
        scratch: &TempDir,
        barrier: &Path,
    ) -> Self {
        let test_binary =
            std::env::current_exe().unwrap_or_else(|error| panic!("current_exe: {error}"));
        let mut installers = Self {
            children: Vec::new(),
        };
        for index in 0..2 {
            let out = scratch.path().join(format!("child-{index}.report"));
            let child = std::process::Command::new(&test_binary)
                .args(["--exact", CHILD_TEST_PATH, "--nocapture", "--test-threads=1"])
                .env(CHILD_CACHE_ENV, cache.path())
                .env(CHILD_BIN_ENV, bin.path())
                .env(CHILD_OUT_ENV, &out)
                .env(CHILD_BARRIER_ENV, barrier)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(
                    std::fs::File::create(scratch.path().join(format!("child-{index}.stderr")))
                        .unwrap_or_else(|error| panic!("child stderr file: {error}")),
                )
                .spawn()
                .unwrap_or_else(|error| panic!("spawn installer process: {error}"));
            installers.children.push((out, child));
        }
        installers
    }

    /// Release both installers together and return what each reported.
    ///
    /// Waiting for readiness before signalling is what makes the two
    /// installers genuinely overlap: without it the test would depend on spawn
    /// timing and could pass against an unserialized implementation.
    fn release_and_collect(&mut self, barrier: &Path) -> Vec<String> {
        for index in 0..self.children.len() {
            await_barrier(&barrier.join(format!("ready-child-{index}.report")));
        }
        std::fs::write(barrier.join(START_MARKER), "go")
            .unwrap_or_else(|error| panic!("release the start barrier: {error}"));
        let mut reports = Vec::new();
        for (out, child) in &mut self.children {
            let status = child
                .wait()
                .unwrap_or_else(|error| panic!("wait for installer process: {error}"));
            assert!(status.success(), "installer process failed: {status:?}");
            reports.push(std::fs::read_to_string(&*out).unwrap_or_else(|error| {
                panic!(
                    "installer process wrote no report ({}): {error}",
                    out.display()
                )
            }));
        }
        reports
    }
}

#[cfg(unix)]
impl Drop for InstallerProcesses {
    fn drop(&mut self) {
        for (_, child) in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(unix)]
#[test]
fn concurrent_processes_install_once_and_agree() {
    // A1: two jefe processes that both miss the same digest must serialize.
    // Exactly one runs `npm install`; the other observes a complete cache hit
    // and resolves the identical executable.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let barrier = scratch.path().join("barrier");
    std::fs::create_dir_all(&barrier).unwrap_or_else(|error| panic!("barrier dir: {error}"));
    // A one-second install widens the overlap so an unserialized implementation
    // reliably runs npm twice.
    let _ = observed_candidate(&bin, cache.path(), &witness, "2.0.0", "1");

    let mut installers = InstallerProcesses::spawn_pair(&bin, &cache, &scratch, &barrier);
    let reports = installers.release_and_collect(&barrier);

    for report in &reports {
        assert!(
            report.starts_with("ok\t"),
            "both concurrent installers must succeed, got {report:?}"
        );
    }
    assert_eq!(
        reports[0], reports[1],
        "concurrent installers must agree on the resolved managed executable"
    );
    assert_eq!(
        observations(&witness).len(),
        1,
        "exactly one of two concurrent jefe processes may run npm install"
    );
}

// ── A2: staged build, atomic promotion ────────────────────────────────────

#[cfg(unix)]
#[test]
fn install_is_staged_outside_the_cache_entry_and_promoted_atomically() {
    // A2: `npm install` must never build inside the published install
    // directory, so a concurrent reader cannot observe a half-built tree.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let (candidate, final_dir) = observed_candidate(&bin, cache.path(), &witness, "2.0.0", "0");

    let invocation = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("install: {error}"));

    let observed = observations(&witness);
    assert_eq!(observed.len(), 1, "one install for a cold cache");
    let (cwd, state) = &observed[0];
    assert_ne!(
        Path::new(cwd),
        physical(&final_dir).as_path(),
        "npm install must not build inside the published install directory"
    );
    assert_eq!(
        state, "final-absent",
        "a cold install must publish nothing until the complete tree is promoted"
    );
    assert!(
        invocation.executable().starts_with(&final_dir),
        "the promoted install must be published at the digest directory: {}",
        invocation.executable().display()
    );
    assert!(
        final_dir.join(".jefe-installed").exists(),
        "promotion must publish the install marker together with the tree"
    );
    let leftovers: Vec<String> = std::fs::read_dir(cache.path())
        .unwrap_or_else(|error| panic!("read cache root: {error}"))
        .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
        .filter_map(|name| name.to_str().map(str::to_owned))
        .filter(|name| name.starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
        "a completed install must leave no staging or retired directory: {leftovers:?}"
    );
}

#[cfg(unix)]
#[test]
fn promotion_replaces_an_existing_entry_without_exposing_a_partial_tree() {
    // A2 (re-install): while a replacement install is building, the previously
    // published tree must remain complete and resolvable.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let (candidate, final_dir) =
        observed_candidate(&bin, cache.path(), &witness, "latest nightly", "0");

    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    publish_observed_version(&witness, "2.0.0");
    finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("re-install: {error}"));

    let observed = observations(&witness);
    assert_eq!(
        observed.len(),
        2,
        "a moved dist-tag must reinstall the published entry"
    );
    assert_eq!(
        observed[1].1, "final-complete",
        "the previously published tree must stay complete while its replacement builds"
    );
    assert_ne!(
        Path::new(&observed[1].0),
        physical(&final_dir).as_path(),
        "a replacement install must not build inside the published directory"
    );
    assert!(
        final_dir.join(".jefe-installed").exists(),
        "the replacement must be published with its marker"
    );
}

// ── A3: interrupted install ───────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn interrupted_install_leaves_no_cache_hit_and_retries_cleanly() {
    // A3: an install that dies after writing part of the tree must leave no
    // marker and no partially published directory, and must not block a later
    // successful install.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let definition = definition("LLxprt");
    executable(
        &bin,
        "npm",
        "#!/bin/sh\nmkdir -p node_modules/half-written\nexit 7\n",
    );
    let candidate = resolve_package(&definition, bin.path(), "2.0.0");
    let final_dir = final_install_dir(&candidate, cache.path());

    let failure = finalize_local_invocation_inner(&candidate, cache.path())
        .err()
        .unwrap_or_else(|| panic!("a failing npm install must surface as an error"));
    assert!(
        matches!(failure, PackageRuntimeError::InstallFailed(_)),
        "npm exit status must surface as InstallFailed, got {failure:?}"
    );
    assert!(
        !final_dir.join(".jefe-installed").exists(),
        "an interrupted install must not publish an install marker"
    );
    assert!(
        !final_dir.join("node_modules").exists(),
        "an interrupted install must not publish a partial node_modules tree"
    );

    executable(
        &bin,
        "npm",
        "#!/bin/sh\nmkdir -p node_modules/.bin\nprintf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt\nchmod 755 node_modules/.bin/llxprt\n",
    );
    let candidate = resolve_package(&definition, bin.path(), "2.0.0");
    let invocation = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("retry after interrupted install: {error}"));
    assert!(
        invocation.executable().starts_with(&final_dir),
        "the retry must publish a usable managed executable"
    );
    assert!(
        final_dir.join(".jefe-installed").exists(),
        "the retry must publish the install marker"
    );
}

// ── A4/A5: cross-process lock at the preparation boundary ─────────────────

/// Short-ceiling policy so a blocked preparation fails in milliseconds rather
/// than waiting out the production install timeout.
#[cfg(unix)]
fn impatient_policy() -> LockPolicy {
    LockPolicy {
        ceiling: std::time::Duration::from_millis(150),
        poll_interval: std::time::Duration::from_millis(10),
    }
}

#[cfg(unix)]
#[test]
fn a_live_installer_blocks_preparation_with_a_typed_redacted_error() {
    // A4b + A5: while another installer legitimately holds the digest,
    // preparation waits and then fails closed — it never steals the lock, never
    // installs, and never leaks the cache location into the diagnostic.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let definition = definition("LLxprt");
    executable(
        &bin,
        "npm",
        "#!/bin/sh\nmkdir -p node_modules/.bin\nprintf '#!/bin/sh\\nexit 0\\n' > node_modules/.bin/llxprt\nchmod 755 node_modules/.bin/llxprt\n",
    );
    let candidate = resolve_package(&definition, bin.path(), "2.0.0");
    let selection = candidate
        .package()
        .unwrap_or_else(|| panic!("candidate carries a package selection"));
    let digest = super::selection_digest(selection, fixture_resolved(&candidate)).to_hex();
    let final_dir = final_install_dir(&candidate, cache.path());

    let _held = crate::runtime::package_install_lock::acquire(
        &super::install_lock_path(cache.path(), &digest),
        &digest,
        LockPolicy::production(),
    )
    .unwrap_or_else(|error| panic!("hold the digest lock: {error}"));

    let error = super::prepare_managed_npm_with_lock_policy(
        &candidate,
        selection,
        cache.path(),
        impatient_policy(),
    )
    .err()
    .unwrap_or_else(|| panic!("preparation must not proceed while the digest is locked"));

    assert!(
        matches!(error, PackageRuntimeError::InstallLockUnavailable(_)),
        "a held digest must fail closed with a lock error, got {error:?}"
    );
    assert!(
        !final_dir.exists(),
        "a blocked preparation must not create the install directory"
    );
    let diagnostic = error.to_string();
    assert!(
        !diagnostic.contains(&cache.path().display().to_string())
            && !diagnostic.contains(std::path::MAIN_SEPARATOR),
        "the diagnostic must not embed the cache location: {diagnostic:?}"
    );
    assert!(
        diagnostic.chars().count() <= MAX_DIAGNOSTIC_CHARS,
        "the diagnostic must be bounded: {diagnostic:?}"
    );
}

/// Longest a rendered diagnostic may be: the bounded detail plus the fixed
/// `Display` prefix each variant prepends.
#[cfg(unix)]
const MAX_DIAGNOSTIC_CHARS: usize = crate::runtime::package_install_lock::MAX_DETAIL_CHARS + 64;

#[cfg(unix)]
#[test]
fn a_failed_promotion_is_typed_bounded_and_redacted() {
    // A5: publication failure is its own variant, and the previously published
    // entry is restored rather than left missing.
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let published = cache.path().join("digest");
    let retired = cache.path().join(".retired-digest");
    std::fs::create_dir_all(published.join("node_modules"))
        .unwrap_or_else(|error| panic!("seed published entry: {error}"));
    std::fs::write(published.join(".jefe-installed"), "marker\n")
        .unwrap_or_else(|error| panic!("seed marker: {error}"));

    let error = super::promote_staged_install(
        &cache.path().join(".staging-digest"),
        &published,
        &retired,
        PROMOTION_DIGEST,
    )
    .err()
    .unwrap_or_else(|| panic!("publishing an absent staging directory cannot succeed"));

    assert!(
        matches!(error, PackageRuntimeError::InstallPromotionFailed(_)),
        "publication failure must be its own variant, got {error:?}"
    );
    let diagnostic = error.to_string();
    assert!(
        !diagnostic.contains(&cache.path().display().to_string())
            && !diagnostic.contains(std::path::MAIN_SEPARATOR),
        "the diagnostic must not embed the cache location: {diagnostic:?}"
    );
    assert!(
        diagnostic.contains("digest=0123456789ab"),
        "the diagnostic must correlate to the selector digest: {diagnostic:?}"
    );
    assert!(
        diagnostic.chars().count() <= MAX_DIAGNOSTIC_CHARS,
        "the diagnostic must be bounded: {diagnostic:?}"
    );
    assert!(
        published.join(".jefe-installed").exists(),
        "a failed publication must restore the previously published entry"
    );
    assert!(
        !retired.exists(),
        "a failed publication must not strand the retired entry"
    );
}

/// Digest used by the promotion tests that call the boundary directly.
#[cfg(unix)]
const PROMOTION_DIGEST: &str = "0123456789abcdef0123456789abcdef";

#[cfg(unix)]
#[test]
fn a_promotion_interrupted_after_retiring_restores_the_previous_install() {
    // A3 (crash between the two publication renames): the published path is
    // absent while a complete previous tree sits at the retired path. The next
    // preparation must republish it under the lock instead of requiring a new
    // network install.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let (candidate, final_dir) = observed_candidate(&bin, cache.path(), &witness, "2.0.0", "0");

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let selection = candidate
        .package()
        .unwrap_or_else(|| panic!("candidate carries a package selection"));
    let digest = super::selection_digest(selection, fixture_resolved(&candidate)).to_hex();
    let retired = cache.path().join(format!(".retired-{digest}"));
    // Reproduce the crash state exactly: published entry retired, nothing
    // published, staging never renamed into place.
    std::fs::rename(&final_dir, &retired)
        .unwrap_or_else(|error| panic!("simulate interrupted promotion: {error}"));
    assert!(!final_dir.exists(), "setup: the published path is absent");

    let recovered = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("recovery after interrupted promotion: {error}"));

    assert_eq!(
        recovered.executable(),
        first.executable(),
        "the interrupted promotion must republish the previous install"
    );
    assert_eq!(
        observations(&witness).len(),
        1,
        "restoring a retired install must not require another npm install"
    );
    assert!(
        !retired.exists(),
        "the restored install must no longer be retired"
    );
}


#[cfg(unix)]
#[test]
fn a_completed_promotion_discards_a_leftover_retired_tree() {
    // The mirror of the interrupted case: publication completed but the retired
    // tree was not discarded. The complete published entry wins and the
    // leftover is reclaimed, without reinstalling.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let (candidate, final_dir) = observed_candidate(&bin, cache.path(), &witness, "2.0.0", "0");

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let retired = retired_path(&candidate, cache.path());
    std::fs::create_dir_all(retired.join("node_modules"))
        .unwrap_or_else(|error| panic!("seed leftover retired entry: {error}"));

    let reconciled = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("reconcile leftover retired tree: {error}"));

    assert_eq!(
        reconciled.executable(),
        first.executable(),
        "the complete published entry must win"
    );
    assert_eq!(
        observations(&witness).len(),
        1,
        "reclaiming a leftover tree must not reinstall"
    );
    assert!(
        final_dir.join(".jefe-installed").exists(),
        "the published entry must be left intact"
    );
    assert!(!retired.exists(), "the leftover retired tree must be gone");
}

#[cfg(unix)]
#[test]
fn an_unusable_published_install_is_replaced_by_the_retired_tree() {
    // A retired tree is the last complete copy of an entry, so it must not be
    // discarded against a published directory that cannot actually run --
    // otherwise an offline user loses the only usable install.
    let bin = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let cache = tempfile::tempdir().unwrap_or_else(|error| panic!("cache: {error}"));
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch: {error}"));
    let witness = scratch.path().join("install-observations.log");
    let (candidate, final_dir) = observed_candidate(&bin, cache.path(), &witness, "2.0.0", "0");

    let first = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("first install: {error}"));
    let retired = retired_path(&candidate, cache.path());
    // The complete tree is retired and the published path holds a directory
    // that exists but carries neither a marker nor the selected binary.
    std::fs::rename(&final_dir, &retired)
        .unwrap_or_else(|error| panic!("retire the complete tree: {error}"));
    std::fs::create_dir_all(final_dir.join("node_modules").join(".bin"))
        .unwrap_or_else(|error| panic!("seed unusable published entry: {error}"));

    let recovered = finalize_local_invocation_inner(&candidate, cache.path())
        .unwrap_or_else(|error| panic!("recovery from an unusable published entry: {error}"));

    assert_eq!(
        recovered.executable(),
        first.executable(),
        "the complete retired tree must be published again"
    );
    assert_eq!(
        observations(&witness).len(),
        1,
        "restoring a retired install must not require another npm install"
    );
    assert!(
        !retired.exists(),
        "the restored install must no longer be retired"
    );
}

/// Retired-path sibling for a resolved managed candidate.
#[cfg(unix)]
fn retired_path(candidate: &ResolvedCandidate, cache_root: &Path) -> PathBuf {
    let selection = candidate
        .package()
        .unwrap_or_else(|| panic!("candidate carries a package selection"));
    cache_root.join(format!(
        ".retired-{}",
        super::selection_digest(selection, fixture_resolved(candidate)).to_hex()
    ))
}
