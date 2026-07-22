//! Typed schema-1 scenario parsing (issue #380).
//!
//! Consumes the bounded JSON tree and produces validated contract types.
//! Every object is closed via [`ObjectReader`]; scalar rules live in
//! `validate`; interpolation grammar checks run at parse time so violations
//! fail before any workspace or launch work. Step parsing lives in
//! `parse_step`; cross-field rules live in `semantic`.

use super::contract::{
    DirSpec, EnvVar, FileContent, FileSpec, Platform, ScenarioV1, Size, WorkspaceSpec,
};
use super::error::HarnessError;
use super::fields::{ObjectReader, as_array, as_int_in, as_str, bounded_len};
use super::json::{JsonValue, parse_json};
use super::limits::{COLS_RANGE, MAX_ENV, MAX_STEPS, MAX_WORKSPACE_ENTRIES, ROWS_RANGE};
use super::parse_step::parse_step;
use super::semantic;
use super::validate::{decode_base64, validate_env_name, validate_rel_path, validate_secrets};

/// Parse schema-1 scenario bytes into a validated [`ScenarioV1`].
///
/// # Errors
///
/// `HAR-E001` for syntax/duplicate/unknown/missing-`schema` violations,
/// `HAR-E002` for exceeded bounds, `HAR-E003` for interpolation violations.
pub fn parse_scenario_v1(input: &[u8]) -> Result<ScenarioV1, HarnessError> {
    let root = parse_json(input)?;
    let mut reader = ObjectReader::new("scenario", &root)?;
    check_schema(&mut reader)?;
    let name = required_non_empty(&mut reader, "name")?;
    let platform = parse_platform(reader.require("platform")?)?;
    let terminal = parse_size("scenario.terminal", reader.require("terminal")?)?;
    let workspace = parse_workspace(reader.require("workspace")?)?;
    let steps_value = reader.require("steps")?;
    let secrets = parse_secret_list(reader.require("secrets")?)?;
    reader.finish()?;

    let steps_json = as_array("scenario.steps", steps_value)?;
    if steps_json.is_empty() {
        return Err(HarnessError::syntax("scenario.steps: must not be empty"));
    }
    bounded_len("scenario.steps", steps_json.len(), MAX_STEPS)?;
    let mut steps = Vec::with_capacity(steps_json.len());
    for (index, value) in steps_json.iter().enumerate() {
        steps.push(parse_step(index, value)?);
    }

    let scenario = ScenarioV1 {
        name,
        platform,
        terminal,
        workspace,
        steps,
        secrets,
    };
    semantic::validate(&scenario)?;
    Ok(scenario)
}

/// The schema gate: `schema` must be present and exactly the integer 1.
/// There is no other accepted input format and no fallback.
fn check_schema(reader: &mut ObjectReader<'_>) -> Result<(), HarnessError> {
    let Some(value) = reader.opt("schema") else {
        return Err(HarnessError::syntax(
            "scenario: missing required field 'schema' (schema-1 input is the only accepted format)",
        ));
    };
    if *value != JsonValue::Int(1) {
        return Err(HarnessError::syntax(
            "scenario.schema: must be the integer 1",
        ));
    }
    Ok(())
}

fn required_non_empty(reader: &mut ObjectReader<'_>, field: &str) -> Result<String, HarnessError> {
    let context = format!("{}.{field}", reader.context());
    let text = as_str(&context, reader.require(field)?)?;
    if text.is_empty() {
        return Err(HarnessError::syntax(format!(
            "{context}: must not be empty"
        )));
    }
    Ok(text.to_string())
}

fn parse_platform(value: &JsonValue) -> Result<Platform, HarnessError> {
    match as_str("scenario.platform", value)? {
        "macos" => Ok(Platform::Macos),
        "linux" => Ok(Platform::Linux),
        other => Err(HarnessError::syntax(format!(
            "scenario.platform: '{other}' must be 'macos' or 'linux'"
        ))),
    }
}

/// Parse a `{cols, rows}` size object with contract ranges.
pub(super) fn parse_size(context: &str, value: &JsonValue) -> Result<Size, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let cols = as_int_in(
        &format!("{context}.cols"),
        reader.require("cols")?,
        COLS_RANGE,
    )?;
    let rows = as_int_in(
        &format!("{context}.rows"),
        reader.require("rows")?,
        ROWS_RANGE,
    )?;
    reader.finish()?;
    Ok(Size {
        cols: u16::try_from(cols)
            .map_err(|_| HarnessError::limit(format!("{context}.cols: out of range")))?,
        rows: u16::try_from(rows)
            .map_err(|_| HarnessError::limit(format!("{context}.rows: out of range")))?,
    })
}

fn parse_workspace(value: &JsonValue) -> Result<WorkspaceSpec, HarnessError> {
    let mut reader = ObjectReader::new("scenario.workspace", value)?;
    parse_mode("scenario.workspace.mode", reader.require("mode")?, &[0o700])?;
    let dirs_value = reader.require("dirs")?;
    let files_value = reader.require("files")?;
    let env_value = reader.require("env")?;
    reader.finish()?;

    let dirs_json = as_array("scenario.workspace.dirs", dirs_value)?;
    bounded_len(
        "scenario.workspace.dirs",
        dirs_json.len(),
        MAX_WORKSPACE_ENTRIES,
    )?;
    let files_json = as_array("scenario.workspace.files", files_value)?;
    bounded_len(
        "scenario.workspace.files",
        files_json.len(),
        MAX_WORKSPACE_ENTRIES,
    )?;
    let mut dirs = Vec::with_capacity(dirs_json.len());
    for (index, dir) in dirs_json.iter().enumerate() {
        dirs.push(parse_dir(
            &format!("scenario.workspace.dirs[{index}]"),
            dir,
        )?);
    }
    let mut files = Vec::with_capacity(files_json.len());
    for (index, file) in files_json.iter().enumerate() {
        files.push(parse_file(
            &format!("scenario.workspace.files[{index}]"),
            file,
        )?);
    }
    let env = parse_env_list("scenario.workspace.env", env_value)?;
    Ok(WorkspaceSpec { dirs, files, env })
}

/// Parse a `{path, mode}` directory spec (mode 448 or 493).
pub(super) fn parse_dir(context: &str, value: &JsonValue) -> Result<DirSpec, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let path_text = as_str(&format!("{context}.path"), reader.require("path")?)?;
    let path = validate_rel_path(&format!("{context}.path"), path_text)?;
    let mode = parse_mode(
        &format!("{context}.mode"),
        reader.require("mode")?,
        &[0o700, 0o755],
    )?;
    reader.finish()?;
    Ok(DirSpec { path, mode })
}

/// Parse a `{path, content, mode}` file spec (mode 384/420/448/493).
pub(super) fn parse_file(context: &str, value: &JsonValue) -> Result<FileSpec, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let path_text = as_str(&format!("{context}.path"), reader.require("path")?)?;
    let path = validate_rel_path(&format!("{context}.path"), path_text)?;
    let body = parse_content(&format!("{context}.content"), reader.require("content")?)?;
    let mode = parse_mode(
        &format!("{context}.mode"),
        reader.require("mode")?,
        &[0o600, 0o644, 0o700, 0o755],
    )?;
    reader.finish()?;
    Ok(FileSpec {
        path,
        content: body,
        mode,
    })
}

/// Parse a `{utf8: string}` or `{base64: string}` content object.
pub(super) fn parse_content(context: &str, value: &JsonValue) -> Result<FileContent, HarnessError> {
    let mut reader = ObjectReader::new(context, value)?;
    let utf8 = reader.opt("utf8");
    let base64 = reader.opt("base64");
    reader.finish()?;
    match (utf8, base64) {
        (Some(text), None) => Ok(FileContent::Utf8(
            as_str(&format!("{context}.utf8"), text)?.to_string(),
        )),
        (None, Some(encoded)) => {
            let raw = as_str(&format!("{context}.base64"), encoded)?;
            Ok(FileContent::Base64(decode_base64(
                &format!("{context}.base64"),
                raw,
            )?))
        }
        _ => Err(HarnessError::syntax(format!(
            "{context}: exactly one of 'utf8' or 'base64' is required"
        ))),
    }
}

/// Parse a list of `{name, value}` env objects with the env-name rule.
pub(super) fn parse_env_list(
    context: &str,
    value: &JsonValue,
) -> Result<Vec<EnvVar>, HarnessError> {
    let entries = as_array(context, value)?;
    bounded_len(context, entries.len(), MAX_ENV)?;
    let mut env = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let entry_context = format!("{context}[{index}]");
        let mut reader = ObjectReader::new(&entry_context, entry)?;
        let name = as_str(&format!("{entry_context}.name"), reader.require("name")?)?;
        validate_env_name(&format!("{entry_context}.name"), name)?;
        let value_text = as_str(&format!("{entry_context}.value"), reader.require("value")?)?;
        reader.finish()?;
        env.push(EnvVar {
            name: name.to_string(),
            value: value_text.to_string(),
        });
    }
    Ok(env)
}

fn parse_secret_list(value: &JsonValue) -> Result<Vec<String>, HarnessError> {
    let entries = as_array("scenario.secrets", value)?;
    let mut secrets = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        secrets.push(as_str(&format!("scenario.secrets[{index}]"), entry)?.to_string());
    }
    validate_secrets(&secrets)?;
    Ok(secrets)
}

/// Parse a mode integer constrained to `allowed` values.
fn parse_mode(context: &str, value: &JsonValue, allowed: &[u32]) -> Result<u32, HarnessError> {
    let JsonValue::Int(raw) = value else {
        return Err(HarnessError::syntax(format!(
            "{context}: expected an integer"
        )));
    };
    let mode = u32::try_from(*raw)
        .map_err(|_| HarnessError::syntax(format!("{context}: invalid mode {raw}")))?;
    if !allowed.contains(&mode) {
        let listed: Vec<String> = allowed.iter().map(ToString::to_string).collect();
        return Err(HarnessError::syntax(format!(
            "{context}: mode {mode} must be one of {}",
            listed.join("|")
        )));
    }
    Ok(mode)
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod parse_tests;
