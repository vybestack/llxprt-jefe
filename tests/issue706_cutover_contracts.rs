//! Source-scan contracts for the issue #706 workbench cutover deletions.
//!
//! The identity flip in slice J1 moved `core.repositories` onto the shared
//! screen runtime; slice J2 deletes the legacy authorities that flip made
//! unreachable. This contract keeps them deleted: every symbol named by
//! `dev-docs/testing/issue706-owner-evidence.json` must stay absent from
//! `src/`, every deleted path must stay absent from the tree, and the five
//! retained modules the maintainer ruled "migrate, never delete" (2026-08-30)
//! must stay byte-identical to the hashes the manifest pins.

use std::fs;
use std::path::{Path, PathBuf};

use jefe::domain::sha256::Sha256;
use serde::Deserialize;

const MANIFEST_PATH: &str = "dev-docs/testing/issue706-owner-evidence.json";
const RETAINED_RULE: &str = "maintainer ruling 2026-08-30: retained, migrate-not-delete";

#[derive(Debug, Deserialize)]
struct Manifest {
    schema: u32,
    issue: u32,
    deleted_paths: Vec<DeletedPath>,
    deleted_symbols: Vec<DeletedSymbol>,
    retained_modules: Vec<RetainedModule>,
    repointed_call_sites: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeletedPath {
    path: String,
}

#[derive(Debug, Deserialize)]
struct DeletedSymbol {
    scope: String,
    symbol: String,
}

#[derive(Debug, Deserialize)]
struct RetainedModule {
    path: String,
    sha256: String,
}

#[test]
fn the_legacy_split_authorities_stay_deleted() {
    let errors = validate_against_the_tree();
    assert!(
        errors.is_empty(),
        "the #706 deletion surface regressed:\n{}",
        errors.join("\n")
    );
}

fn validate_against_the_tree() -> Vec<String> {
    let mut errors = Vec::new();
    let manifest = match load_manifest() {
        Ok(manifest) => manifest,
        Err(error) => return vec![format!("{MANIFEST_PATH} must load: {error}")],
    };
    if manifest.schema != 1 || manifest.issue != 706 {
        errors.push("manifest schema/issue identity differs".to_owned());
    }
    validate_deleted_paths(&manifest, &mut errors);
    validate_deleted_symbols(&manifest, &mut errors);
    validate_retained_modules(&manifest, &mut errors);
    validate_repointed_call_sites(&manifest, &mut errors);
    errors
}

fn load_manifest() -> Result<Manifest, String> {
    let path = repo_path(MANIFEST_PATH);
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

fn validate_deleted_paths(manifest: &Manifest, errors: &mut Vec<String>) {
    if !is_sorted_unique(
        manifest
            .deleted_paths
            .iter()
            .map(|entry| entry.path.as_str()),
    ) {
        errors.push("deleted paths must be sorted and unique".to_owned());
    }
    for entry in &manifest.deleted_paths {
        if repo_path(&entry.path).exists() {
            errors.push(format!("deleted path still exists: {}", entry.path));
        }
    }
}

fn validate_deleted_symbols(manifest: &Manifest, errors: &mut Vec<String>) {
    if !is_sorted_unique(
        manifest
            .deleted_symbols
            .iter()
            .map(|entry| (entry.scope.as_str(), entry.symbol.as_str())),
    ) {
        errors.push("deleted symbols must be sorted and unique".to_owned());
    }
    for entry in &manifest.deleted_symbols {
        let files = match rust_files(&repo_path(&entry.scope)) {
            Ok(files) => files,
            Err(error) => {
                errors.push(format!(
                    "scan {} for {}: {error}",
                    entry.scope, entry.symbol
                ));
                continue;
            }
        };
        for path in files {
            let source = match fs::read_to_string(&path) {
                Ok(source) => source,
                Err(error) => {
                    errors.push(format!("read {}: {error}", path.display()));
                    continue;
                }
            };
            if source.contains(&entry.symbol) {
                errors.push(format!(
                    "deleted symbol {} resurrected in {}",
                    entry.symbol,
                    path.display()
                ));
            }
        }
    }
}

fn validate_retained_modules(manifest: &Manifest, errors: &mut Vec<String>) {
    if !is_sorted_unique(
        manifest
            .retained_modules
            .iter()
            .map(|entry| entry.path.as_str()),
    ) {
        errors.push("retained modules must be sorted and unique".to_owned());
    }
    for entry in &manifest.retained_modules {
        let bytes = match fs::read(repo_path(&entry.path)) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors.push(format!(
                    "retained module {} is absent ({RETAINED_RULE}): {error}",
                    entry.path
                ));
                continue;
            }
        };
        if text_sha256(&bytes) != entry.sha256 {
            errors.push(format!(
                "retained module {} drifted from its pinned bytes ({RETAINED_RULE})",
                entry.path
            ));
        }
    }
}

fn validate_repointed_call_sites(manifest: &Manifest, errors: &mut Vec<String>) {
    if manifest.repointed_call_sites.is_empty()
        || !is_sorted_unique(manifest.repointed_call_sites.iter().map(String::as_str))
    {
        errors.push("repointed call sites must be a sorted, non-empty ledger".to_owned());
    }
    for site in &manifest.repointed_call_sites {
        // Every ledger row names a real file before the dash that starts its
        // rationale, so a typo'd path cannot hide behind prose.
        let Some((path, _)) = site.split_once(" - ") else {
            errors.push(format!("repointed call site must name a path: {site}"));
            continue;
        };
        if !repo_path(path).is_file() {
            errors.push(format!("repointed call site path is absent: {path}"));
        }
    }
}

fn is_sorted_unique<T: PartialOrd>(values: impl Iterator<Item = T>) -> bool {
    let values: Vec<_> = values.collect();
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("read {}: {error}", directory.display()))?
        {
            let entry =
                entry.map_err(|error| format!("read entry in {}: {error}", directory.display()))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn text_sha256(bytes: &[u8]) -> String {
    let mut canonical = Vec::with_capacity(bytes.len());
    let mut offset = 0;
    while let Some(position) = bytes[offset..].windows(2).position(|pair| pair == b"\r\n") {
        let newline = offset + position;
        canonical.extend_from_slice(&bytes[offset..newline]);
        canonical.push(b'\n');
        offset = newline + 2;
    }
    canonical.extend_from_slice(&bytes[offset..]);
    Sha256::digest(&canonical).to_string()
}

fn repo_path(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}
