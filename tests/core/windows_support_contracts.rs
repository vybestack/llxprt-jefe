//! Contract tests for native Windows support (issue #264, AC-01..AC-13).
//!
//! These tests validate durable contracts — not implementation details —
//! across the docs, release package contents, install script, and CI
//! workflows. They follow the same `read_repo_text` / `repo_path` pattern
//! used by `tmux_harness_docs_contracts.rs`.

use std::path::{Path, PathBuf};

/// AC-12: the README platform support matrix marks Windows supported and
/// documents the exact portable GitHub Release and psmux Winget commands.
#[test]
fn readme_marks_windows_supported_with_exact_commands() {
    let readme = read_repo_text("README.md");
    assert!(
        readme.contains("| Windows (x86_64) | **Supported**"),
        "README must mark Windows (x86_64) as Supported"
    );
    assert!(
        readme.contains("jefe-vX.Y.Z-x86_64-pc-windows-msvc.zip"),
        "README must document the portable Windows release zip name"
    );
    assert!(
        readme.contains("winget install --id marlocarlo.psmux --exact"),
        "README must document the exact qualified psmux install command"
    );
    assert!(
        readme.contains("jefe-install.ps1"),
        "README must reference the first-party install script"
    );
    assert!(
        readme.contains("docs/windows-support.md"),
        "README must link the Windows support guide"
    );
}

/// AC-12: the Windows support guide exists and covers all required topics.
#[test]
fn windows_support_doc_covers_required_topics() {
    let guide = read_repo_text("docs/windows-support.md");
    for required in [
        "ConPTY",
        "psmux",
        "winget install --id marlocarlo.psmux --exact",
        "Windows Terminal",
        "ConHost",
        "jefe-install.ps1",
        "Upgrade",
        "Uninstall",
        "JEFE_PSMUX_BIN",
        "LongPathsEnabled",
        "MAX_PATH",
        "OSC 52",
        "JEFE_LOG_FILE",
        "jefe doctor",
        "remote",
        "PATH",
        "PATHEXT",
        "WSL, Cygwin, MSYS2",
    ] {
        assert!(
            guide.contains(required),
            "docs/windows-support.md must cover {required:?}"
        );
    }
    // The guide must NOT claim a Jefe Winget package exists. Check several
    // spellings so a rephrased or differently-formatted false claim is caught.
    let jefe_winget_claims = [
        "winget install --id vybestack.jefe",
        "vybestack.jefe",
        "Jefe Winget package",
        "Jefe's Winget package",
    ];
    for claim in &jefe_winget_claims {
        assert!(
            !guide.contains(claim),
            "docs must not claim a Jefe Winget package exists: found {claim:?}"
        );
    }
}

/// AC-12: docs/getting-started.md documents Windows prerequisites.
#[test]
fn getting_started_documents_windows_prerequisites() {
    let guide = read_repo_text("docs/getting-started.md");
    assert!(
        guide.contains("winget install --id marlocarlo.psmux --exact"),
        "getting-started must document the exact psmux install command"
    );
    assert!(
        guide.contains("Windows support"),
        "getting-started must link the Windows support guide"
    );
}

/// AC-12: docs/building.md documents MSVC/PowerShell build.
#[test]
fn building_docs_document_msvc_and_powershell() {
    let doc = read_repo_text("docs/building.md");
    assert!(
        doc.contains("x86_64-pc-windows-msvc"),
        "building.md must document the Windows MSVC target"
    );
    assert!(
        doc.contains("jefe-install.ps1"),
        "building.md must reference the install script"
    );
}

/// AC-12: docs/technical-overview.md reflects cross-platform status.
#[test]
fn technical_overview_reflects_cross_platform_status() {
    let doc = read_repo_text("docs/technical-overview.md");
    assert!(
        doc.contains("psmux"),
        "technical-overview must mention psmux"
    );
    assert!(
        doc.contains("ConPTY"),
        "technical-overview must mention ConPTY"
    );
    assert!(
        doc.contains("native Windows"),
        "technical-overview must mention native Windows support"
    );
}

/// AC-10: the release workflow packages a Windows MSVC zip with the
/// required contents and checksums.
#[test]
fn release_workflow_packages_windows_msvc_zip() {
    let workflow = read_repo_text(".github/workflows/release.yml");
    assert!(
        workflow.contains("x86_64-pc-windows-msvc"),
        "release.yml must include the Windows MSVC matrix entry"
    );
    assert!(
        workflow.contains("Package Windows portable zip"),
        "release.yml must have a Windows packaging step"
    );
    assert!(
        workflow.contains("jefe-install.ps1"),
        "release.yml must include the install script in the zip"
    );
    assert!(
        workflow.contains("LICENSE"),
        "release.yml must include LICENSE in the zip"
    );
    assert!(
        workflow.contains("Generate Windows checksums")
            && workflow.contains("Get-FileHash")
            && workflow.contains("Generate Unix checksums")
            && workflow.contains("shasum -a 256"),
        "release.yml must generate native checksums on every matrix OS"
    );
}

/// AC-10: the release zip must NOT bundle psmux or third-party binaries.
/// The packaging step only copies jefe.exe, LICENSE, and jefe-install.ps1.
#[test]
fn release_zip_excludes_third_party_binaries() {
    let workflow = read_repo_text(".github/workflows/release.yml");
    let zip_step_start = workflow
        .find("Package Windows portable zip")
        .unwrap_or_else(|| panic!("Windows packaging step not found"));
    let zip_section = &workflow[zip_step_start..];
    let step_end = zip_section
        .find("\n      - name:")
        .unwrap_or(zip_section.len());
    let zip_step = &zip_section[..step_end];
    assert!(
        !zip_step.contains("psmux.exe"),
        "release zip must not bundle psmux.exe"
    );
    assert!(
        zip_step.contains("jefe.exe")
            && zip_step.contains("LICENSE")
            && zip_step.contains("jefe-install.ps1"),
        "release zip must contain jefe.exe, LICENSE, and jefe-install.ps1"
    );
}

/// AC-01/AC-11: CI runs a clean package lifecycle in the windows_native job.
#[test]
fn ci_windows_native_runs_clean_package_lifecycle() {
    let workflow = read_repo_text(".github/workflows/ci.yml");
    assert!(
        workflow.contains("Clean package lifecycle (install, doctor, upgrade, uninstall)"),
        "CI must have a clean package lifecycle step"
    );
    assert!(
        workflow.contains("Run real psmux startup-quit against installed binary"),
        "CI must run psmux startup-quit against the installed binary"
    );
    assert!(
        workflow.contains("-Action Install"),
        "CI must exercise the install action"
    );
    assert!(
        workflow.contains("-Action Upgrade"),
        "CI must exercise the upgrade action"
    );
    assert!(
        workflow.contains("-Action Uninstall"),
        "CI must exercise the uninstall action"
    );
    assert!(
        workflow.contains("ownership marker"),
        "CI must assert the ownership marker"
    );
    assert!(
        workflow.contains("Compress-Archive") && workflow.contains("Expand-Archive"),
        "CI must install from a built package archive"
    );
    assert!(
        workflow.contains("Clean package lifecycle residue") && workflow.contains("if: always()"),
        "CI must clean package-owned residue even after a failure"
    );
    assert!(
        workflow.contains("sentinel"),
        "CI must assert a config sentinel survives uninstall"
    );
    assert!(
        workflow.contains("installed jefe uninstall failed with exit")
            && workflow.contains("install dir survived uninstall"),
        "CI must fail when uninstall fails or leaves package-owned files"
    );
    assert!(
        workflow.contains("orphaned psmux sessions remain after uninstall"),
        "CI must reject installed-package psmux sessions that survive uninstall"
    );
}

/// AC-01..AC-03/AC-10: the first-party install script exists and enforces
/// the ownership-marker, per-user, PATH-safety, and preservation contracts.
#[test]
fn install_script_enforces_ownership_and_preservation() {
    let script = read_repo_text("scripts/jefe-install.ps1");
    assert!(
        script.contains("$OwnerMarker = '.jefe-installed'"),
        "install script must define an ownership marker"
    );
    assert!(
        script.contains("Read-OwnerMetadata") && script.contains("ConvertFrom-Json"),
        "install script must validate ownership metadata before removal"
    );
    assert!(
        script.contains(".stage-") && script.contains(".backup-"),
        "install and upgrade must publish transactionally"
    );
    assert!(
        script.contains("LOCALAPPDATA"),
        "install script must default to per-user LOCALAPPDATA"
    );
    assert!(
        script.contains("Remove-JefeUserPath") && script.contains("Add-JefeUserPath"),
        "install script must add/remove only its own PATH entry"
    );
    assert!(
        script.contains("[ValidateSet('Install', 'Upgrade', 'Uninstall')]"),
        "install script must support Install, Upgrade, Uninstall actions"
    );
    assert!(
        script.contains("SourceDir") && script.contains("InstallDir"),
        "install script must support SourceDir/InstallDir parameters for CI"
    );
    assert_installer_safety_contracts(&script);
    assert!(
        script.contains("Normalize-PathEntry")
            && script.contains("TrimEnd([IO.Path]::DirectorySeparatorChar"),
        "PATH ownership comparisons must normalize trailing separators"
    );
    assert!(
        script.contains("restoring the previous install also failed")
            && script.contains("backup remains at"),
        "a failed rollback must report both failures and the retained backup"
    );
    // Must not perform privileged/system-wide mutation.
    assert!(
        !script.contains("HKEY_LOCAL_MACHINE")
            && !script.contains("New-Item -ItemType Directory -Force -Path 'C:\\Program Files'"),
        "install script must not perform system-wide mutation"
    );
}

fn assert_installer_safety_contracts(script: &str) {
    assert!(
        script.contains("Invoke-WithInstallLock")
            && script.contains("Local\\jefe-install-")
            && script.contains("Security.Cryptography.SHA256")
            && script.contains("ReleaseMutex"),
        "install lifecycle must serialize each normalized install path with a collision-resistant mutex"
    );
    assert!(
        script.contains("Assert-SafeInstallDir")
            && script.contains("must not be a drive root")
            && script.contains("must not be a protected system directory"),
        "install lifecycle must reject dangerous recursive-removal targets"
    );
    assert!(
        script.contains("$newUserPath.Length -ge 32000"),
        "install must reject a user PATH that would exceed the safe Windows limit"
    );
    assert!(
        script.contains("Normalize-PathEntry")
            && script.contains("TrimEnd([IO.Path]::DirectorySeparatorChar"),
        "PATH ownership comparisons must normalize trailing separators"
    );
    assert!(
        script.contains("restoring the previous install also failed")
            && script.contains("backup remains at"),
        "a failed rollback must report both failures and the retained backup"
    );
    // Must not perform privileged/system-wide mutation.
    assert!(
        !script.contains("HKEY_LOCAL_MACHINE")
            && !script.contains("New-Item -ItemType Directory -Force -Path 'C:\\Program Files'"),
        "install script must not perform system-wide mutation"
    );
}

/// AC-13: a top-level LICENSE exists and is Apache-2.0.
#[test]
fn top_level_license_is_apache_2() {
    let license = read_repo_text("LICENSE");
    assert!(
        license.contains("Apache License") && license.contains("Version 2.0"),
        "LICENSE must be Apache-2.0"
    );
    assert!(
        license.contains("Vybestack LLC"),
        "LICENSE must carry the copyright holder"
    );
}

// ── Helpers (same pattern as tmux_harness_docs_contracts.rs) ──────────────

fn read_repo_text(relative_path: impl AsRef<Path>) -> String {
    let path = repo_path(relative_path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_path(relative_path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path.as_ref())
}
