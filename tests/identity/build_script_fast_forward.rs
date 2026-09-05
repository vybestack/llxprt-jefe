//! Issue #753: a fast-forward pull must re-bake `JEFE_GIT_COMMIT`.
//!
//! The cargo-level test compiles the real, unmodified `build.rs` as the build
//! script of a fixture crate (via `build = "<path>"` in the fixture manifest),
//! so cargo itself makes the rerun decision the issue is about. The seam tests
//! pin the watch-path resolution of `build_support/git_watch.rs` across the
//! ordinary git layouts.

use crate::git_watch::{self, WatchState};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Fixture package name; also the build-script output directory prefix.
const FIXTURE_PACKAGE: &str = "jefe753fixture";
/// Fixture branch name, pinned so the watched ref path is deterministic.
const FIXTURE_BRANCH: &str = "test-main";
/// Branch created for the linked-worktree seam fixture.
const WORKTREE_BRANCH: &str = "wt-branch";

/// The fixture binary prints its compile-time baked commit.
const FIXTURE_MAIN_RS: &str = r#"fn main() {
    println!("{}", env!("JEFE_GIT_COMMIT"));
}
"#;

/// A1: building before and after a fast-forward must re-bake the commit, and
/// A2: the emitted watch list must name the resolved HEAD and branch-ref files.
#[test]
fn fast_forward_rebakes_jefe_git_commit() {
    let _cargo_lock = nested_cargo_lock();
    let fixture = fixture_repo();

    let commit_a = git_short_head(fixture.root.path());
    let baked_a = build_and_bake(&fixture);
    assert_eq!(baked_a, commit_a, "first build must bake commit {commit_a}");

    // Fast-forward exactly like a pull: move the branch ref and the worktree
    // while `.git/HEAD` and the build script stay byte-identical.
    let head_before =
        std::fs::read(head_path(&fixture)).test_unwrap("read .git/HEAD before fast-forward");
    write_file(&fixture.root.path().join("README.md"), "second\n");
    run_git(fixture.root.path(), &["add", "README.md"]);
    run_git(fixture.root.path(), &["commit", "--quiet", "-m", "second"]);
    let head_after =
        std::fs::read(head_path(&fixture)).test_unwrap("read .git/HEAD after fast-forward");
    assert_eq!(
        head_before, head_after,
        "fast-forward must not rewrite .git/HEAD"
    );

    let commit_b = git_short_head(fixture.root.path());
    assert_ne!(
        commit_a, commit_b,
        "fixture must advance to a distinct commit"
    );

    let baked_b = build_and_bake(&fixture);
    assert_eq!(
        baked_b, commit_b,
        "rebuilt binary must report {commit_b}; JEFE_GIT_COMMIT stayed stale at {baked_a}"
    );

    assert_watches_resolved_paths(&fixture);
}

/// A2: the emitted watch list names the resolved HEAD file and the resolved
/// loose branch-ref file.
fn assert_watches_resolved_paths(fixture: &Fixture) {
    let output = build_script_output(fixture);
    let watches = |path: &Path| {
        output.lines().any(|line| {
            line.strip_prefix("cargo:rerun-if-changed=")
                .is_some_and(|watched| Path::new(watched).ends_with(path))
        })
    };
    assert!(
        watches(Path::new(".git/HEAD")),
        "watch list must contain the resolved HEAD file\n{output}"
    );
    let branch_ref = Path::new(".git/refs/heads").join(FIXTURE_BRANCH);
    assert!(
        watches(&branch_ref),
        "watch list must contain the resolved branch-ref file\n{output}"
    );
}

// ── seam tests: watch-path resolution across git layouts ────────────────────

/// A2 seam: an ordinary attached repository watches HEAD plus the loose branch
/// ref, both emitted package-relative.
#[test]
fn seam_attached_watches_head_and_branch_ref() {
    let repo = seam_repo();
    let watch = git_watch::resolve(repo.path());
    let GitWatchAttached { head, branch_ref } = attached(&watch);
    assert!(head.ends_with(Path::new(".git").join("HEAD")));
    assert!(branch_ref.is_file(), "a fresh repo has a loose branch ref");
    let paths = watch.rerun_paths(repo.path());
    assert_eq!(paths.len(), 2, "attached watch list: {paths:?}");
    assert!(watches_path(&paths, Path::new(".git/HEAD")));
    assert!(watches_path(
        &paths,
        &Path::new(".git/refs/heads").join(FIXTURE_BRANCH)
    ));
    assert!(
        paths.iter().all(|path| !Path::new(path).is_absolute()),
        "ordinary repositories emit package-relative paths: {paths:?}"
    );
}

/// A3 seam: a detached HEAD watches the resolved HEAD file only.
#[test]
fn seam_detached_watches_head_only() {
    let repo = seam_repo();
    run_git(repo.path(), &["checkout", "--quiet", "--detach"]);
    let watch = git_watch::resolve(repo.path());
    let WatchState::Detached { head } = &watch else {
        panic!("detached repo must classify as detached: {watch:?}");
    };
    assert!(head.ends_with(Path::new(".git").join("HEAD")));
    let paths = watch.rerun_paths(repo.path());
    assert_eq!(paths.len(), 1, "detached watch list: {paths:?}");
    assert!(watches_path(&paths, Path::new(".git/HEAD")));
}

/// A4 seam: the loose branch-ref path stays watched even while packed; its
/// absent-to-present transition is the next ref update's trigger.
#[test]
fn seam_packed_branch_still_watches_loose_ref_path() {
    let repo = seam_repo();
    run_git(repo.path(), &["pack-refs", "--all", "--prune"]);
    let watch = git_watch::resolve(repo.path());
    let GitWatchAttached { branch_ref, .. } = attached(&watch);
    assert!(
        !branch_ref.exists(),
        "fixture must pack the branch away: {}",
        branch_ref.display()
    );
    let paths = watch.rerun_paths(repo.path());
    assert!(watches_path(
        &paths,
        &Path::new(".git/refs/heads").join(FIXTURE_BRANCH)
    ));
}

/// A5 seam: a linked worktree (gitfile `.git`) resolves HEAD in its
/// per-worktree gitdir and the branch ref in the common dir, both emitted
/// absolute because they sit outside the worktree package root.
#[test]
fn seam_linked_worktree_resolves_gitfile_layout() {
    let main_repo = seam_repo();
    let parent = tempfile::tempdir().test_unwrap("create linked worktree parent");
    let worktree = parent.path().join("wt");
    run_git(
        main_repo.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            WORKTREE_BRANCH,
            worktree.to_string_lossy().as_ref(),
        ],
    );
    let watch = git_watch::resolve(&worktree);
    let GitWatchAttached { head, branch_ref } = attached(&watch);
    assert!(head.ends_with("HEAD"));
    assert!(
        head.components()
            .any(|component| component.as_os_str() == "worktrees"),
        "HEAD must resolve inside the per-worktree gitdir: {}",
        head.display()
    );
    assert!(
        branch_ref.ends_with(Path::new("refs/heads").join(WORKTREE_BRANCH)),
        "branch ref must resolve in the common dir: {}",
        branch_ref.display()
    );
    let paths = watch.rerun_paths(&worktree);
    assert_eq!(paths.len(), 2, "worktree watch list: {paths:?}");
    assert!(
        paths.iter().all(|path| Path::new(path).is_absolute()),
        "worktree watch paths sit outside the package: {paths:?}"
    );
}

/// A6 seam: outside any repository the watch list stays today's floor
/// (`.git/HEAD`, package-relative).
#[test]
fn seam_non_repo_falls_back_to_floor_watch() {
    let plain = tempfile::tempdir().test_unwrap("create non-repo tempdir");
    let watch = git_watch::resolve(plain.path());
    assert_eq!(watch, WatchState::Fallback);
    let paths = watch.rerun_paths(plain.path());
    assert_eq!(paths.len(), 1, "floor watch list: {paths:?}");
    assert!(watches_path(&paths, Path::new(".git/HEAD")));
}

/// Destructure an attached classification, naming the failure for its layout.
fn attached(watch: &WatchState) -> GitWatchAttached<'_> {
    match watch {
        WatchState::Attached { head, branch_ref } => GitWatchAttached { head, branch_ref },
        other => panic!("expected an attached classification: {other:?}"),
    }
}

/// Borrowed attached fields, so tests can pattern-match without moving.
struct GitWatchAttached<'a> {
    head: &'a Path,
    branch_ref: &'a Path,
}

/// Component-wise watch check: platform-neutral and Windows-compatible.
fn watches_path(paths: &[String], expected: &Path) -> bool {
    paths.iter().any(|path| Path::new(path).ends_with(expected))
}

struct Fixture {
    /// Package root and repository root (identical for the fixture).
    root: tempfile::TempDir,
    /// Isolated cargo target directory so the nested builds never contend for
    /// the jefe target lock or observe its artifacts.
    target: tempfile::TempDir,
}

/// Create the fixture repository at commit A: a standalone cargo package whose
/// build script is the real jefe `build.rs`, on branch `test-main` with one
/// committed `README.md`.
fn fixture_repo() -> Fixture {
    let root = tempfile::tempdir().test_unwrap("create fixture repository");
    let target = tempfile::tempdir().test_unwrap("create isolated fixture target dir");
    init_git_repo(root.path());
    write_file(&root.path().join("Cargo.toml"), &fixture_manifest());
    write_file(&root.path().join("src/main.rs"), FIXTURE_MAIN_RS);
    Fixture { root, target }
}

/// A git repository on a deterministically named branch with one commit, for
/// the seam tests (mirrors the tests/git_info conventions).
fn seam_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().test_unwrap("create seam test repository");
    init_git_repo(dir.path());
    dir
}

fn init_git_repo(dir: &Path) {
    run_git(dir, &["init", "--quiet"]);
    run_git(
        dir,
        &[
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{FIXTURE_BRANCH}"),
        ],
    );
    run_git(dir, &["config", "user.email", "test@test.test"]);
    run_git(dir, &["config", "user.name", "Test"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);
    write_file(&dir.join("README.md"), "first\n");
    run_git(dir, &["add", "README.md"]);
    run_git(dir, &["commit", "--quiet", "-m", "first"]);
}

/// Standalone workspace (not jefe's) whose `build =` points at the real,
/// absolute jefe `build.rs`; cargo compiles that exact file as the script.
fn fixture_manifest() -> String {
    let build_script = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let baked = build_script.to_string_lossy().replace('\\', "/");
    format!(
        "[workspace]\n\n[package]\nname = \"{FIXTURE_PACKAGE}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"{baked}\"\n"
    )
}

fn build_and_bake(fixture: &Fixture) -> String {
    build(fixture);
    run_baked_commit(fixture)
}

fn build(fixture: &Fixture) {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut command = Command::new(cargo);
    command
        .args(["build", "--offline"])
        .arg("--target-dir")
        .arg(fixture.target.path())
        .current_dir(fixture.root.path());
    for key in inherited_build_env_keys() {
        command.env_remove(&key);
    }
    let output = command.output().test_unwrap("spawn nested cargo build");
    assert!(
        output.status.success(),
        "fixture build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Cargo- and rustc-inherited variables that could redirect the nested build
/// away from the fixture's isolated target directory.
fn inherited_build_env_keys() -> Vec<String> {
    let mut keys: Vec<String> = std::env::vars()
        .map(|(key, _)| key)
        .filter(|key| {
            (key.starts_with("CARGO_") && key != "CARGO_HOME")
                || matches!(
                    key.as_str(),
                    "RUSTFLAGS" | "RUSTDOCFLAGS" | "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER"
                )
        })
        .collect();
    keys.sort_unstable();
    keys
}

/// Run the compiled fixture binary and return its baked commit.
fn run_baked_commit(fixture: &Fixture) -> String {
    let binary = fixture
        .target
        .path()
        .join("debug")
        .join(format!("{FIXTURE_PACKAGE}{}", std::env::consts::EXE_SUFFIX));
    let output = Command::new(&binary)
        .output()
        .test_unwrap("run fixture binary");
    assert!(
        output.status.success(),
        "fixture binary failed\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).test_unwrap("fixture binary stdout");
    stdout.trim().to_owned()
}

/// Read cargo's build-script output file, which records the emitted
/// `cargo:` directives from the fixture's build script runs.
fn build_script_output(fixture: &Fixture) -> String {
    let build_root = fixture.target.path().join("debug").join("build");
    let entries = std::fs::read_dir(&build_root).test_unwrap("read fixture build directory");
    for entry in entries {
        let entry = entry.test_unwrap("read fixture build directory entry");
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(FIXTURE_PACKAGE)
        {
            continue;
        }
        let candidate = entry.path().join("output");
        if let Ok(text) = std::fs::read_to_string(&candidate)
            && text.contains("JEFE_GIT_COMMIT")
        {
            return text;
        }
    }
    panic!(
        "no build-script output naming JEFE_GIT_COMMIT under {}",
        build_root.display()
    );
}

fn git_short_head(dir: &Path) -> String {
    run_git(dir, &["rev-parse", "--short", "HEAD"])
}

fn head_path(fixture: &Fixture) -> PathBuf {
    fixture.root.path().join(".git").join("HEAD")
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .test_unwrap(&format!("spawn git {args:?}"));
    assert!(
        output.status.success(),
        "git {args:?} failed in {}\n{}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .test_unwrap(&format!("mkdir parent for {}", path.display()));
    }
    std::fs::write(path, content).test_unwrap(&format!("write {}", path.display()));
}

// ── per-target test support (mirrors tests/git_info/support.rs convention) ──

/// Shared result diagnostics so test code never calls `unwrap`/`expect`.
trait TestResultExt<T> {
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E> TestResultExt<T> for Result<T, E>
where
    E: std::fmt::Debug,
{
    fn test_unwrap(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// Serialize nested cargo invocations across tests (issue #753 follows the
/// `tests/support/mod.rs` convention).
fn nested_cargo_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .test_unwrap("nested Cargo command lock should not be poisoned")
}
