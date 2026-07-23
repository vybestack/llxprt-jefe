//! Cross-field semantic validation for a parsed scenario (issue #380).
//!
//! Runs after typed parsing so the whole document is structurally valid
//! first. Enforces name uniqueness after exact normalization, capture count,
//! materialized-file size, env-name uniqueness, and step-order rules that
//! can be decided statically (a `launch` must precede terminal operations).

use std::collections::BTreeSet;

use super::capture::BEHAVIOR_SUFFIX;
use super::contract::{ScenarioV1, Step};
use super::error::HarnessError;
use super::limits::{MAX_BYTES, MAX_CAPTURES};

/// Validate scenario-wide rules.
///
/// # Errors
///
/// `HAR-E001` for structural rule violations, `HAR-E002` for exceeded counts.
pub fn validate(scenario: &ScenarioV1) -> Result<(), HarnessError> {
    validate_workspace(scenario)?;
    validate_steps(scenario)
}

fn validate_workspace(scenario: &ScenarioV1) -> Result<(), HarnessError> {
    let mut paths = BTreeSet::new();
    let file_paths: BTreeSet<&str> = scenario
        .workspace
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    for dir in &scenario.workspace.dirs {
        insert_workspace_path(&mut paths, &file_paths, dir.path.as_str())?;
    }
    for file in &scenario.workspace.files {
        insert_workspace_path(&mut paths, &file_paths, file.path.as_str())?;
        check_file_size(file.path.as_str(), file.content.bytes().len())?;
    }
    let mut env_names = BTreeSet::new();
    for entry in &scenario.workspace.env {
        if !env_names.insert(entry.name.clone()) {
            return Err(HarnessError::syntax(format!(
                "scenario.workspace.env: duplicate name '{}'",
                entry.name
            )));
        }
    }
    Ok(())
}

fn insert_workspace_path(
    paths: &mut BTreeSet<String>,
    file_paths: &BTreeSet<&str>,
    path: &str,
) -> Result<(), HarnessError> {
    if !paths.insert(path.to_string()) {
        return Err(duplicate_path(path));
    }
    let mut prefix = String::new();
    for component in path
        .split('/')
        .take(path.split('/').count().saturating_sub(1))
    {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        if file_paths.contains(prefix.as_str()) {
            return Err(HarnessError::syntax(format!(
                "scenario.workspace: path '{path}' is nested beneath file '{prefix}'"
            )));
        }
    }
    Ok(())
}

#[derive(Default)]
struct StepScan {
    capture_names: BTreeSet<String>,
    capture_paths: BTreeSet<String>,
    occupied_paths: BTreeSet<String>,
    launched: bool,
    finished: bool,
}

fn validate_steps(scenario: &ScenarioV1) -> Result<(), HarnessError> {
    let mut scan = StepScan::default();
    scan.occupied_paths.extend(
        scenario
            .workspace
            .dirs
            .iter()
            .map(|dir| dir.path.as_str().to_string()),
    );
    scan.occupied_paths.extend(
        scenario
            .workspace
            .files
            .iter()
            .map(|file| file.path.as_str().to_string()),
    );
    for (index, step) in scenario.steps.iter().enumerate() {
        if scan.finished {
            return Err(HarnessError::syntax(format!(
                "steps[{index}]: no step may follow 'finish'"
            )));
        }
        check_step(&mut scan, index, step)?;
    }
    Ok(())
}

fn check_step(scan: &mut StepScan, index: usize, step: &Step) -> Result<(), HarnessError> {
    match step {
        Step::Capture { name, path, .. } => check_capture(scan, index, name, path.as_str()),
        Step::Write { file } => {
            check_mutation_collision(scan, index, file.path.as_str())?;
            scan.occupied_paths.insert(file.path.as_str().to_string());
            check_file_size(file.path.as_str(), file.content.bytes().len())
        }
        Step::Mkdir { dir } => {
            check_mutation_collision(scan, index, dir.path.as_str())?;
            scan.occupied_paths.insert(dir.path.as_str().to_string());
            Ok(())
        }
        Step::Remove { path } => {
            check_mutation_collision(scan, index, path.as_str())?;
            let prefix = format!("{}/", path.as_str());
            scan.occupied_paths
                .retain(|entry| entry != path.as_str() && !entry.starts_with(&prefix));
            Ok(())
        }
        Step::Launch { .. } => {
            if scan.launched {
                return Err(HarnessError::syntax(format!(
                    "steps[{index}]: only one 'launch' is allowed; use 'restart'"
                )));
            }
            scan.launched = true;
            Ok(())
        }
        Step::Key { .. }
        | Step::Text { .. }
        | Step::Resize { .. }
        | Step::Wait { .. }
        | Step::AssertFrame { .. }
        | Step::Restart => {
            if scan.launched {
                Ok(())
            } else {
                Err(HarnessError::syntax(format!(
                    "steps[{index}]: '{}' requires a prior 'launch'",
                    step.op_name()
                )))
            }
        }
        Step::AssertCapture { capture } => {
            if scan.capture_names.contains(&capture.name) {
                Ok(())
            } else {
                Err(HarnessError::syntax(format!(
                    "steps[{index}]: assert-capture references unregistered capture '{}'",
                    capture.name
                )))
            }
        }
        Step::Finish => {
            scan.finished = true;
            Ok(())
        }
        Step::AssertFile { .. } => Ok(()),
    }
}

fn check_capture(
    scan: &mut StepScan,
    index: usize,
    name: &str,
    path: &str,
) -> Result<(), HarnessError> {
    if !scan.capture_names.insert(name.to_string()) {
        return Err(HarnessError::syntax(format!(
            "steps[{index}]: duplicate capture name '{name}'"
        )));
    }
    for reserved in [path.to_string(), format!("{path}{BEHAVIOR_SUFFIX}")] {
        if scan.occupied_paths.contains(&reserved) {
            return Err(HarnessError::syntax(format!(
                "steps[{index}]: capture path '{reserved}' conflicts with fixture content"
            )));
        }
        if !scan.capture_paths.insert(reserved.clone()) {
            return Err(HarnessError::syntax(format!(
                "steps[{index}]: duplicate capture path '{reserved}'"
            )));
        }
    }
    if scan.capture_names.len() > MAX_CAPTURES {
        return Err(HarnessError::limit(format!(
            "steps[{index}]: captures exceed {MAX_CAPTURES}"
        )));
    }
    Ok(())
}

fn check_mutation_collision(scan: &StepScan, index: usize, path: &str) -> Result<(), HarnessError> {
    let prefix = format!("{path}/");
    if scan
        .capture_paths
        .iter()
        .any(|reserved| reserved == path || reserved.starts_with(&prefix))
    {
        return Err(HarnessError::syntax(format!(
            "steps[{index}]: mutation path '{path}' conflicts with a capture reservation"
        )));
    }
    Ok(())
}

fn check_file_size(path: &str, len: usize) -> Result<(), HarnessError> {
    if len > MAX_BYTES {
        return Err(HarnessError::limit(format!(
            "file '{path}' content is {len} bytes (max {MAX_BYTES})"
        )));
    }
    Ok(())
}

fn duplicate_path(path: &str) -> HarnessError {
    HarnessError::syntax(format!(
        "scenario.workspace: duplicate path '{path}' after normalization"
    ))
}
