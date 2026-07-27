//! GitHub boundary for pull-request changed files.
//!
//! This module owns REST path construction, strict response decoding, and the
//! bounded page loop for GitHub's documented 3,000-file limit.

use serde::Deserialize;

use crate::domain::{PrFileChange, PrFileStatus};

use super::{GhClient, GhError};

const FILES_PER_PAGE: u32 = 100;
const MAX_FILE_PAGES: u32 = 30;

#[derive(Debug, Deserialize)]
struct ApiPrFile {
    sha: String,
    filename: String,
    #[serde(default)]
    previous_filename: Option<String>,
    status: String,
    additions: u64,
    deletions: u64,
    changes: u64,
    #[serde(default)]
    patch: Option<String>,
}

/// Result of reading all changed-file pages permitted by GitHub's endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFilesResponse {
    /// Changed files in GitHub response order.
    pub files: Vec<PrFileChange>,
    /// Whether the final allowed page was full and more files may exist.
    pub truncated: bool,
}

/// Build the REST path for one pull-request files page.
#[must_use]
pub fn build_pr_files_api_path(
    owner: &str,
    name: &str,
    number: u64,
    page: u32,
    per_page: u32,
) -> String {
    format!("repos/{owner}/{name}/pulls/{number}/files?per_page={per_page}&page={page}")
}

/// Parse one bare JSON array returned by the pull-request files endpoint.
pub fn parse_pr_files_json(json: &str) -> Result<Vec<PrFileChange>, GhError> {
    let api_files: Vec<ApiPrFile> = serde_json::from_str(json)
        .map_err(|error| GhError::ParseError(format!("pull-request files: {error}")))?;
    Ok(api_files
        .into_iter()
        .map(|file| PrFileChange {
            blob_sha: file.sha,
            path: file.filename,
            previous_path: file.previous_filename,
            status: PrFileStatus::from_api(&file.status),
            additions: file.additions,
            deletions: file.deletions,
            changes: file.changes,
            patch: file.patch,
        })
        .collect())
}

/// Parse a GraphQL immutable blob response into explicit display content.
pub fn parse_pr_blob_json(json: &str) -> Result<crate::domain::PrFileBlob, GhError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| GhError::ParseError(format!("pull-request blob: {error}")))?;
    let blob = value.pointer("/data/repository/object").ok_or_else(|| {
        GhError::ParseError("pull-request blob: response object missing".to_string())
    })?;
    let byte_size = blob
        .get("byteSize")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if blob
        .get("isBinary")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(crate::domain::PrFileBlob::Binary);
    }
    if blob
        .get("isTruncated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(crate::domain::PrFileBlob::Truncated { byte_size });
    }
    blob.get("text")
        .and_then(serde_json::Value::as_str)
        .map(|text| crate::domain::PrFileBlob::Text(text.to_string()))
        .ok_or_else(|| GhError::ParseError("pull-request blob: text missing".to_string()))
}

impl GhClient {
    /// Read one immutable Git blob through GitHub's bounded GraphQL text fields.
    pub fn get_pr_file_blob(
        &self,
        owner: &str,
        name: &str,
        oid: &str,
    ) -> Result<crate::domain::PrFileBlob, GhError> {
        let query = "query($owner:String!,$repo:String!,$oid:GitObjectID!){repository(owner:$owner,name:$repo){object(oid:$oid){... on Blob{byteSize isBinary isTruncated text}}}}";
        let args = vec![
            "api".to_owned(),
            "graphql".to_owned(),
            "-f".to_owned(),
            format!("query={query}"),
            "-F".to_owned(),
            format!("owner={owner}"),
            "-F".to_owned(),
            format!("repo={name}"),
            "-F".to_owned(),
            format!("oid={oid}"),
        ];
        parse_pr_blob_json(&Self::run_gh(&args)?)
    }

    /// Read changed files for a pull request up to GitHub's 3,000-file limit.
    pub fn list_pr_files(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Result<PrFilesResponse, GhError> {
        let mut files = Vec::new();
        let mut final_page_full = false;
        for page in 1..=MAX_FILE_PAGES {
            let path = build_pr_files_api_path(owner, name, number, page, FILES_PER_PAGE);
            let args = vec!["api".to_owned(), path];
            let stdout = Self::run_gh(&args)?;
            let page_files = parse_pr_files_json(&stdout)?;
            final_page_full = page_files.len() == FILES_PER_PAGE as usize;
            files.extend(page_files);
            if !final_page_full {
                return Ok(PrFilesResponse {
                    files,
                    truncated: false,
                });
            }
        }
        Ok(PrFilesResponse {
            files,
            truncated: final_page_full,
        })
    }
}

#[cfg(test)]
#[path = "pr_diff_tests.rs"]
mod tests;
