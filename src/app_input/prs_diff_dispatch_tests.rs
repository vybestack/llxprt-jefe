//! Local-boundary evidence for the lazy PR Changes full-file blob read.
//!
//! These tests cover the pure classification and argv helpers extracted from
//! [`super::read_local_blob`] plus its end-to-end fallback contract. They use
//! structural argv/output classification rather than global `PATH` mutation.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use jefe::domain::{PrFileBlob, RepositoryId};

use super::{
    LocalSizeProbe, PrBlobLoadParams, cat_file_blob_argv, cat_file_size_argv,
    classify_local_blob_bytes, classify_local_size_probe, read_local_blob,
};

/// Build params bound to `local_dir` (local repository probe path).
fn local_params(local_dir: Option<PathBuf>) -> PrBlobLoadParams {
    PrBlobLoadParams {
        scope_repo_id: RepositoryId("repo".to_string()),
        pr_number: 376,
        owner: "owner".to_string(),
        repo: "repo".to_string(),
        request_id: 1,
        blob_sha: "deadbeef".to_string(),
        local_dir,
    }
}

/// Test-only helper: unwrap a `Result::Ok` or panic with context.
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

/// Test-only helper: unwrap an `Option::Some` or panic with context.
trait TestOptionExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T> TestOptionExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

fn exit_success() -> ExitStatus {
    // `true` exits 0 on all platforms; its exit status is the portable
    // success sentinel without constructing a platform-specific code.
    std::process::Command::new("true")
        .status()
        .value_or_panic("a successful exit status can be produced")
}

fn exit_failure() -> ExitStatus {
    std::process::Command::new("false")
        .status()
        .value_or_panic("a failing exit status can be produced")
}

#[test]
fn classify_local_blob_bytes_text_when_no_nul_byte() {
    let result = classify_local_blob_bytes(b"fn main() {}".to_vec());
    assert_eq!(
        result,
        Some(Ok(PrFileBlob::Text("fn main() {}".to_string())))
    );
}

#[test]
fn classify_local_blob_bytes_binary_when_valid_utf8_contains_nul() {
    // A NUL byte inside an otherwise valid-UTF-8 stream uses Git's binary
    // heuristic; the file must not be misrepresented as text.
    let bytes = b"\0".to_vec();
    let result = classify_local_blob_bytes(bytes);
    assert_eq!(result, Some(Ok(PrFileBlob::Binary)));
}

#[test]
fn classify_local_blob_bytes_local_miss_when_non_utf8() {
    // Lone continuation byte is invalid UTF-8; local must miss so the caller
    // falls back to GitHub's authoritative `isBinary` metadata.
    let result = classify_local_blob_bytes(vec![0xFF]);
    assert!(
        result.is_none(),
        "non-UTF-8 content must signal a local miss"
    );
}

#[test]
fn classify_local_size_probe_present_bytes() {
    let probe = classify_local_size_probe(b"42\n", b"", exit_success());
    assert!(matches!(probe, LocalSizeProbe::Bytes(42)));
}

#[test]
fn classify_local_size_probe_missing_on_nonzero_exit() {
    let probe = classify_local_size_probe(b"", b"fatal: Not a valid object name", exit_failure());
    assert!(matches!(probe, LocalSizeProbe::Missing));
}

#[test]
fn classify_local_size_probe_missing_on_unparseable_size() {
    let probe = classify_local_size_probe(b"not-a-number\n", b"", exit_success());
    assert!(matches!(probe, LocalSizeProbe::Missing));
}

#[test]
fn cat_file_size_argv_keeps_spaces_and_unicode_directory_as_one_arg() {
    let directory = Path::new("/repo with spaces/Ω üñîçødé");
    let argv = cat_file_size_argv(directory, "abc123");
    assert_eq!(
        argv,
        vec![
            "-C".into(),
            directory.as_os_str().to_owned(),
            "cat-file".into(),
            "-s".into(),
            "abc123".into()
        ]
    );
    // The directory and oid must remain exactly two distinct arguments.
    assert_eq!(argv.len(), 5);
}

#[test]
fn cat_file_blob_argv_keeps_spaces_and_unicode_directory_as_one_arg() {
    let directory = Path::new("/repo dir/Ω path with spaces");
    let argv = cat_file_blob_argv(directory, "café123");
    assert_eq!(
        argv,
        vec![
            "-C".into(),
            directory.as_os_str().to_owned(),
            "cat-file".into(),
            "blob".into(),
            "café123".into()
        ]
    );
}

#[test]
fn read_local_blob_returns_none_when_local_dir_is_none() {
    // Remote-configured repositories skip the local probe entirely.
    assert!(read_local_blob(&local_params(None)).is_none());
}

// ── End-to-end local Git object evidence ────────────────────────────────────
//
// These tests create a real temporary Git repository and blob objects so the
// full `read_local_blob` path — including the read-only `git -C <dir>
// cat-file` calls — is exercised exactly. They depend only on a local `git`
// binary, never on `PATH` mutation or a test-only subprocess seam.

fn skip_if_no_git() -> Option<()> {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return Some(());
    }
    None
}

fn init_repo_with_blob(content: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().value_or_panic("create temp git repo dir");
    let git_env = [
        ("GIT_AUTHOR_NAME", "jefe"),
        ("GIT_AUTHOR_EMAIL", "jefe@example.com"),
        ("GIT_COMMITTER_NAME", "jefe"),
        ("GIT_COMMITTER_EMAIL", "jefe@example.com"),
    ];
    let status = std::process::Command::new("git")
        .arg("init")
        .arg(dir.path())
        .envs(git_env.iter().copied())
        .status()
        .value_or_panic("git init");
    assert!(status.success(), "git init must succeed in temp dir");
    // `git hash-object -w --stdin` writes the blob to the object store and
    // prints its immutable SHA.
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["hash-object", "-w", "--stdin"])
        .envs(git_env.iter().copied())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .value_or_panic("piped stdin")
                .write_all(content)?;
            child.wait_with_output()
        })
        .value_or_panic("git hash-object");
    assert!(
        output.status.success(),
        "git hash-object must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (dir, oid)
}

#[test]
fn read_local_blob_present_text_object() {
    if skip_if_no_git().is_some() {
        return;
    }
    let (dir, oid) = init_repo_with_blob(b"fn main() {}\n");
    let params = local_params(Some(dir.path().to_path_buf()));
    let mut params = params;
    params.blob_sha = oid;
    let result =
        read_local_blob(&params).value_or_panic("a present text object is not a local miss");
    let blob = result.value_or_panic("text object classifies without error");
    assert_eq!(blob, PrFileBlob::Text("fn main() {}\n".to_string()));
}

#[test]
fn read_local_blob_missing_object_signals_fallback() {
    if skip_if_no_git().is_some() {
        return;
    }
    let dir = tempfile::tempdir().value_or_panic("create temp git repo dir");
    let status = std::process::Command::new("git")
        .arg("init")
        .arg(dir.path())
        .status()
        .value_or_panic("git init");
    assert!(status.success(), "git init must succeed in temp dir");
    let params = local_params(Some(dir.path().to_path_buf()));
    let mut params = params;
    params.blob_sha = "0".repeat(40);
    // A missing object must signal a local miss (None) so the caller falls
    // back to GitHub's authoritative blob read without an error.
    assert!(read_local_blob(&params).is_none());
}

#[test]
fn read_local_blob_oversized_object_is_truncated() {
    if skip_if_no_git().is_some() {
        return;
    }
    // Create a blob larger than MAX_FULL_FILE_BYTES so the size probe short
    // circuits to Truncated without reading the content.
    let big = vec![b'A'; usize::try_from(super::MAX_FULL_FILE_BYTES + 1).unwrap_or(0)];
    let (dir, oid) = init_repo_with_blob(&big);
    let params = local_params(Some(dir.path().to_path_buf()));
    let mut params = params;
    params.blob_sha = oid;
    let result = read_local_blob(&params).value_or_panic("oversized object is not a local miss");
    match result {
        Ok(PrFileBlob::Truncated { byte_size }) => {
            assert_eq!(byte_size, super::MAX_FULL_FILE_BYTES + 1);
        }
        other => panic!("expected Truncated, got {other:?}"),
    }
}

#[test]
fn read_local_blob_binary_object_with_nul_byte() {
    if skip_if_no_git().is_some() {
        return;
    }
    // A valid-UTF-8 stream containing a NUL byte uses Git's binary heuristic.
    let (dir, oid) = init_repo_with_blob(b"\0");
    let params = local_params(Some(dir.path().to_path_buf()));
    let mut params = params;
    params.blob_sha = oid;
    let result = read_local_blob(&params).value_or_panic("binary object is not a local miss");
    let blob = result.value_or_panic("binary object classifies without error");
    assert_eq!(blob, PrFileBlob::Binary);
}
