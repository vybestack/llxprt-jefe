//! Atomic persistence writer contract tests.

use std::fs;
use std::path::{Path, PathBuf};

use super::diagnostic::CfgCode;
use super::sha256::Sha256;
use super::writer::{
    AtomicWrite, BackupPolicy, DraftBytes, ExpectedHash, Freshness, WriteOutcome, WritePhase,
    write, write_failing_at,
};

trait TestResultExt<T, E> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn request(target: &Path, bytes: &[u8], expected: ExpectedHash) -> AtomicWrite {
    AtomicWrite {
        target: target.to_path_buf(),
        draft: DraftBytes::new(bytes.to_vec()),
        expected,
        revision: 7,
        backup: BackupPolicy::None,
    }
}

fn sibling_entries(target: &Path) -> Vec<PathBuf> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::read_dir(parent)
        .value_or_panic("read target parent")
        .map(|entry| entry.value_or_panic("read sibling entry").path())
        .collect()
}

#[test]
fn matching_hash_replaces_target_and_returns_authoritative_hash() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    fs::write(&target, b"before").value_or_panic("seed target");
    let expected = ExpectedHash::Present(Sha256::digest(b"before"));

    let outcome = write(request(&target, b"after", expected), |_| Freshness::Current)
        .value_or_panic("matching write");

    assert_eq!(fs::read(&target).value_or_panic("read target"), b"after");
    assert_eq!(
        outcome,
        WriteOutcome::Authoritative {
            revision: 7,
            hash: Sha256::digest(b"after")
        }
    );
    assert_eq!(sibling_entries(&target), vec![target]);
}
#[test]
fn expected_absent_creates_first_authority() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");

    let outcome = write(
        request(&target, b"first authority", ExpectedHash::Absent),
        |_| Freshness::Current,
    )
    .value_or_panic("first write");

    assert_eq!(
        outcome,
        WriteOutcome::Authoritative {
            revision: 7,
            hash: Sha256::digest(b"first authority")
        }
    );
    assert_eq!(
        fs::read(&target).value_or_panic("read first authority"),
        b"first authority"
    );
}

#[test]
fn expected_absent_conflicts_with_existing_authority() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    fs::write(&target, b"existing authority").value_or_panic("seed authority");

    let error = write(request(&target, b"draft", ExpectedHash::Absent), |_| {
        Freshness::Current
    })
    .err()
    .unwrap_or_else(|| panic!("existing authority must conflict"));

    assert_eq!(error.diagnostic().code, CfgCode::E007);
    assert_eq!(error.draft().as_ref(), b"draft");
    assert_eq!(
        fs::read(&target).value_or_panic("read existing authority"),
        b"existing authority"
    );
}

#[test]
fn changed_disk_returns_conflict_with_immutable_draft_and_no_write() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("settings.toml");
    fs::write(&target, b"changed").value_or_panic("seed changed target");
    let expected = ExpectedHash::Present(Sha256::digest(b"original"));

    let error = write(request(&target, b"draft", expected), |_| Freshness::Current)
        .err()
        .unwrap_or_else(|| panic!("hash mismatch must fail"));

    assert_eq!(error.diagnostic().code, CfgCode::E007);
    assert_eq!(error.draft().as_ref(), b"draft");
    assert_eq!(fs::read(&target).value_or_panic("read target"), b"changed");
    assert_eq!(sibling_entries(&target), vec![target]);
}

#[test]
fn stale_revision_never_replaces_target_and_cleans_owned_temp() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    fs::write(&target, b"current").value_or_panic("seed target");
    let expected = ExpectedHash::Present(Sha256::digest(b"current"));

    let outcome = write(request(&target, b"stale", expected), |revision| {
        assert_eq!(revision, 7);
        Freshness::Stale
    })
    .value_or_panic("stale write is not an error");

    assert_eq!(outcome, WriteOutcome::Stale { revision: 7 });
    assert_eq!(fs::read(&target).value_or_panic("read target"), b"current");
    assert_eq!(sibling_entries(&target), vec![target]);
}

#[test]
fn schema1_replacement_creates_and_reuses_content_addressed_backup() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    let source = b"schema one bytes";
    fs::write(&target, source).value_or_panic("seed schema one target");
    let expected = ExpectedHash::Present(Sha256::digest(source));
    let mut first = request(&target, b"schema two", expected);
    first.backup = BackupPolicy::RetainSchema1;

    write(first, |_| Freshness::Current).value_or_panic("schema one replacement");
    let backup = temp
        .path()
        .join(format!("state.json.schema1.{}.bak", Sha256::digest(source)));
    assert_eq!(fs::read(&backup).value_or_panic("read backup"), source);

    fs::write(&target, source).value_or_panic("restore schema one target");
    let mut second = request(
        &target,
        b"schema two again",
        ExpectedHash::Present(Sha256::digest(source)),
    );
    second.backup = BackupPolicy::RetainSchema1;
    write(second, |_| Freshness::Current).value_or_panic("reuse identical backup");

    assert_eq!(
        fs::read(&backup).value_or_panic("read reused backup"),
        source
    );
}

#[test]
fn conflicting_schema1_backup_blocks_replacement_and_retains_draft() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    let source = b"schema one bytes";
    fs::write(&target, source).value_or_panic("seed schema one target");
    let backup = temp
        .path()
        .join(format!("state.json.schema1.{}.bak", Sha256::digest(source)));
    fs::write(&backup, b"conflicting backup").value_or_panic("seed conflict");
    let mut operation = request(
        &target,
        b"schema two draft",
        ExpectedHash::Present(Sha256::digest(source)),
    );
    operation.backup = BackupPolicy::RetainSchema1;

    let error = write(operation, |_| Freshness::Current)
        .err()
        .unwrap_or_else(|| panic!("backup conflict must fail"));

    assert_eq!(error.diagnostic().code, CfgCode::E104);
    assert_eq!(error.draft().as_ref(), b"schema two draft");
    assert_eq!(fs::read(&target).value_or_panic("read target"), source);
    assert_eq!(
        fs::read(&backup).value_or_panic("read conflicting backup"),
        b"conflicting backup"
    );
}

#[cfg(unix)]
#[test]
fn created_target_temp_and_backup_are_user_only() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    let source = b"schema one bytes";
    fs::write(&target, source).value_or_panic("seed target");
    let mut operation = request(
        &target,
        b"schema two",
        ExpectedHash::Present(Sha256::digest(source)),
    );
    operation.backup = BackupPolicy::RetainSchema1;

    write(operation, |_| Freshness::Current).value_or_panic("mode write");
    let backup = temp
        .path()
        .join(format!("state.json.schema1.{}.bak", Sha256::digest(source)));
    let target_mode = fs::metadata(&target)
        .value_or_panic("target metadata")
        .permissions()
        .mode()
        & 0o777;
    let backup_mode = fs::metadata(&backup)
        .value_or_panic("backup metadata")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(target_mode, 0o600);
    assert_eq!(backup_mode, 0o600);
}

#[test]
fn every_pre_replace_phase_failure_retains_complete_old_authority_and_draft() {
    let phases = [
        WritePhase::CreateParent,
        WritePhase::ReadTarget,
        WritePhase::CreateBackup,
        WritePhase::WriteBackup,
        WritePhase::SyncBackup,
        WritePhase::SyncBackupParent,
        WritePhase::CreateTemp,
        WritePhase::WriteTemp,
        WritePhase::SyncTemp,
        WritePhase::CheckFreshness,
        WritePhase::Replace,
    ];
    for phase in phases {
        assert_pre_replace_phase_failure(phase);
    }
}

fn assert_pre_replace_phase_failure(phase: WritePhase) {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    let source = b"schema one authority";
    fs::write(&target, source).value_or_panic("seed authority");
    let mut operation = request(
        &target,
        b"schema two draft",
        ExpectedHash::Present(Sha256::digest(source)),
    );
    operation.backup = BackupPolicy::RetainSchema1;

    let error = write_failing_at(operation, |_| Freshness::Current, phase)
        .err()
        .unwrap_or_else(|| panic!("phase {phase:?} must fail"));

    assert_eq!(error.diagnostic().code, CfgCode::E104, "{phase:?}");
    assert_eq!(error.draft().as_ref(), b"schema two draft", "{phase:?}");
    assert_eq!(
        fs::read(&target).value_or_panic("read authority"),
        source,
        "{phase:?}"
    );
    assert_no_owned_temp(&target, phase);
}

#[test]
fn final_parent_sync_failure_reports_write_error_after_complete_replacement() {
    let temp = tempfile::tempdir().value_or_panic("temporary writer directory");
    let target = temp.path().join("state.json");
    fs::write(&target, b"old authority").value_or_panic("seed authority");
    let operation = request(
        &target,
        b"new complete authority",
        ExpectedHash::Present(Sha256::digest(b"old authority")),
    );

    let error = write_failing_at(operation, |_| Freshness::Current, WritePhase::SyncParent)
        .err()
        .unwrap_or_else(|| panic!("parent sync phase must fail"));

    assert_eq!(error.diagnostic().code, CfgCode::E104);
    assert_eq!(error.draft().as_ref(), b"new complete authority");
    assert_eq!(
        fs::read(&target).value_or_panic("read authority"),
        b"new complete authority"
    );
    assert_no_owned_temp(&target, WritePhase::SyncParent);
}

fn assert_no_owned_temp(target: &Path, phase: WritePhase) {
    let has_temp = sibling_entries(target).iter().any(|path| {
        path.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name.contains(".jefe-tmp-"))
    });
    assert!(!has_temp, "{phase:?} must not leave an owned temp file");
}
