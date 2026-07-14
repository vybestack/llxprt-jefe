//! CLI argument parsing for `jefe-tutorial-capture`.
//!
//! Provides typed option structs, a hand-written argument parser, usage text,
//! and output helpers. Extracted from the main binary to keep file sizes under
//! the project limit.

use std::io::Write;
use std::path::PathBuf;

use jefe_tutorial_capture::RuntimeProfile;

// ── Option structs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Prepare(PrepareOpts),
    CaptureLocal(CaptureLocalOpts),
    CaptureGithub(CaptureGithubOpts),
    PlanGithub(PlanGithubOpts),
    Render(RenderOpts),
    Report(ReportOpts),
    Cleanup(CleanupOpts),
    ValidateRuntime(ValidateRuntimeOpts),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareOpts {
    pub run_id: String,
    pub base_dir: PathBuf,
    pub scenario_name: String,
    pub runtime_profile: String,
    pub shim_availability: jefe_tutorial_capture::ShimAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureLocalOpts {
    pub manifest_path: PathBuf,
    pub scenario_path: PathBuf,
    pub jefe_bin: PathBuf,
    pub keep_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanGithubOpts {
    pub fixture_repo: String,
    pub run_id: String,
    pub allow_merge: bool,
    pub dry_run: bool,
    pub confirm_disposable: bool,
    pub allowlist_file: Option<PathBuf>,
    pub allow_repos: Vec<String>,
    pub manifest_path: Option<PathBuf>,
    pub clone_dest: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOpts {
    pub manifest_path: PathBuf,
}

/// Typed capture mode for `capture-github`.
///
/// Replaces implicit filename-based inference (checking whether the scenario
/// filename contains "merge") with an explicit, typed flag so the capture
/// behavior is always determined by caller intent, never by a filename
/// heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GitHubCaptureMode {
    /// Standard Issues/PR capture without merge. This is the default.
    #[default]
    Standard,
    /// Merge variant: the scenario drives the PR merge flow in addition to
    /// Issues/PR inspection. Requires `--allow-merge` authorization from
    /// `plan-github`.
    Merge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureGithubOpts {
    pub manifest_path: PathBuf,
    pub jefe_bin: PathBuf,
    pub keep_session: bool,
    pub mode: GitHubCaptureMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOpts {
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupOpts {
    pub manifest_path: PathBuf,
    pub purge_evidence: bool,
    pub dry_run: bool,
    pub confirm: bool,
}

/// Options for the `validate-runtime` subcommand.
///
/// **Finding #8**: Real-runtime profiles need a separate explicit validation
/// scenario/contract, not a silent shim-specific scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidateRuntimeOpts {
    pub manifest_path: PathBuf,
    pub jefe_bin: PathBuf,
    pub keep_session: bool,
}

// ── Parse error ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Help,
    Message(String),
}

impl ParseError {
    fn message(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }

    #[cfg(test)]
    pub fn contains(&self, needle: &str) -> bool {
        match self {
            Self::Help => false,
            Self::Message(message) => message.contains(needle),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help => f.write_str("help requested"),
            Self::Message(message) => f.write_str(message),
        }
    }
}

// ── Parser ─────────────────────────────────────────────────────────────────

impl CliArgs {
    pub fn parse<I>(mut args: I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = String>,
    {
        let subcommand = args
            .next()
            .ok_or_else(|| ParseError::message("missing subcommand"))?;
        let command = match subcommand.as_str() {
            "prepare" => Command::Prepare(parse_prepare(&mut args)?),
            "capture-local" => Command::CaptureLocal(parse_capture_local(&mut args)?),
            "capture-github" => Command::CaptureGithub(parse_capture_github(&mut args)?),
            "plan-github" => Command::PlanGithub(parse_plan_github(&mut args)?),
            "render" => Command::Render(parse_render(&mut args)?),
            "report" => Command::Report(parse_report(&mut args)?),
            "cleanup" => Command::Cleanup(parse_cleanup(&mut args)?),
            "validate-runtime" => Command::ValidateRuntime(parse_validate_runtime(&mut args)?),
            "-h" | "--help" => return Err(ParseError::Help),
            other => {
                return Err(ParseError::message(format!("unknown subcommand: {other}")));
            }
        };
        Ok(Self { command })
    }
}

fn parse_prepare<I>(args: &mut I) -> Result<PrepareOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut run_id = None;
    let mut base_dir = None;
    let mut scenario_name = None;
    let mut runtime_profile = None;
    let mut shim_availability = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run-id" => run_id = Some(next_value(args, "--run-id")?),
            "--base-dir" => base_dir = Some(next_value(args, "--base-dir")?),
            "--scenario" => scenario_name = Some(next_value(args, "--scenario")?),
            "--runtime-profile" => runtime_profile = Some(next_value(args, "--runtime-profile")?),
            "--shim-availability" => {
                shim_availability = Some(next_value(args, "--shim-availability")?);
            }
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    let shim_availability = match &shim_availability {
        Some(value) => jefe_tutorial_capture::ShimAvailability::parse(value).ok_or_else(|| {
            ParseError::message(format!(
                "unknown shim availability '{value}'. Valid: llxprt-only, code-puppy-only, both"
            ))
        })?,
        None => jefe_tutorial_capture::ShimAvailability::default(),
    };
    Ok(PrepareOpts {
        run_id: run_id.unwrap_or_else(|| {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            format!("tutorial-{nanos}")
        }),
        base_dir: PathBuf::from(
            base_dir.unwrap_or_else(|| "/tmp/jefe-tutorial-capture".to_string()),
        ),
        scenario_name: scenario_name.unwrap_or_else(|| "tutorial-capture-local".to_string()),
        runtime_profile: runtime_profile.unwrap_or_else(|| "shim".to_string()),
        shim_availability,
    })
}

fn parse_capture_local<I>(args: &mut I) -> Result<CaptureLocalOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    let mut scenario_path = None;
    let mut jefe_bin = None;
    let mut keep_session = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "--scenario" => scenario_path = Some(next_value(args, "--scenario")?),
            "--jefe-bin" => jefe_bin = Some(next_value(args, "--jefe-bin")?),
            "--keep-session" => keep_session = true,
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(CaptureLocalOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
        scenario_path: required_path(scenario_path, "--scenario")?,
        jefe_bin: required_path(jefe_bin, "--jefe-bin")?,
        keep_session,
    })
}

fn parse_plan_github<I>(args: &mut I) -> Result<PlanGithubOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut fixture_repo = None;
    let mut run_id = None;
    let mut allow_merge = false;
    let mut dry_run = false;
    let mut confirm_disposable = false;
    let mut allowlist_file = None;
    let mut allow_repos = Vec::new();
    let mut manifest_path = None;
    let mut clone_dest = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture-repo" => fixture_repo = Some(next_value(args, "--fixture-repo")?),
            "--run-id" => run_id = Some(next_value(args, "--run-id")?),
            "--allow-merge" => allow_merge = true,
            "--dry-run" => dry_run = true,
            "--confirm-disposable" => confirm_disposable = true,
            "--allowlist-file" => allowlist_file = Some(next_value(args, "--allowlist-file")?),
            "--allow-repo" => allow_repos.push(next_value(args, "--allow-repo")?),
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "--clone-dest" => clone_dest = Some(next_value(args, "--clone-dest")?),
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(PlanGithubOpts {
        fixture_repo: required_string(fixture_repo, "--fixture-repo")?,
        run_id: required_string(run_id, "--run-id")?,
        allow_merge,
        dry_run,
        confirm_disposable,
        allowlist_file: allowlist_file.map(PathBuf::from),
        allow_repos,
        manifest_path: manifest_path.map(PathBuf::from),
        clone_dest: clone_dest.map(PathBuf::from),
    })
}

fn parse_report<I>(args: &mut I) -> Result<ReportOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(ReportOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
    })
}

fn parse_render<I>(args: &mut I) -> Result<RenderOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(RenderOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
    })
}

fn parse_capture_github<I>(args: &mut I) -> Result<CaptureGithubOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    let mut jefe_bin = None;
    let mut keep_session = false;
    let mut mode = GitHubCaptureMode::Standard;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "--jefe-bin" => jefe_bin = Some(next_value(args, "--jefe-bin")?),
            "--keep-session" => keep_session = true,
            "--merge" => mode = GitHubCaptureMode::Merge,
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(CaptureGithubOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
        jefe_bin: required_path(jefe_bin, "--jefe-bin")?,
        keep_session,
        mode,
    })
}

fn parse_cleanup<I>(args: &mut I) -> Result<CleanupOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    let mut purge_evidence = false;
    let mut dry_run = false;
    let mut confirm = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "--purge-evidence" => purge_evidence = true,
            "--dry-run" => dry_run = true,
            "--confirm" => confirm = true,
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(CleanupOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
        purge_evidence,
        dry_run,
        confirm,
    })
}

fn parse_validate_runtime<I>(args: &mut I) -> Result<ValidateRuntimeOpts, ParseError>
where
    I: Iterator<Item = String>,
{
    let mut manifest_path = None;
    let mut jefe_bin = None;
    let mut keep_session = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => manifest_path = Some(next_value(args, "--manifest")?),
            "--jefe-bin" => jefe_bin = Some(next_value(args, "--jefe-bin")?),
            "--keep-session" => keep_session = true,
            "-h" | "--help" => return Err(ParseError::Help),
            other => return Err(ParseError::message(format!("unknown argument: {other}"))),
        }
    }
    Ok(ValidateRuntimeOpts {
        manifest_path: required_path(manifest_path, "--manifest")?,
        jefe_bin: required_path(jefe_bin, "--jefe-bin")?,
        keep_session,
    })
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String, ParseError>
where
    I: Iterator<Item = String>,
{
    let value = args
        .next()
        .ok_or_else(|| ParseError::message(format!("missing value for {flag}")))?;
    if value.is_empty() || value.starts_with('-') {
        return Err(ParseError::message(format!("missing value for {flag}")));
    }
    Ok(value)
}

fn required_path(value: Option<String>, flag: &str) -> Result<PathBuf, ParseError> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| ParseError::message(format!("missing required {flag}")))
}

fn required_string(value: Option<String>, flag: &str) -> Result<String, ParseError> {
    value.ok_or_else(|| ParseError::message(format!("missing required {flag}")))
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Parse a runtime profile string into a typed `RuntimeProfile`.
///
/// Unknown values are an error, not a silent default to shim.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
///
/// # Errors
///
/// Returns a `ParseError` if the value is not a recognized runtime profile.
pub fn parse_runtime_profile(value: &str) -> Result<RuntimeProfile, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "shim" => Ok(RuntimeProfile::Shim),
        "real-llxprt" | "real_llxprt" | "realllxprt" => Ok(RuntimeProfile::RealLlxprt),
        "real-code-puppy" | "real_code_puppy" | "realcodepuppy" => {
            Ok(RuntimeProfile::RealCodePuppy)
        }
        other => Err(format!(
            "unknown runtime profile '{other}'. Valid: shim, real-llxprt, real-code-puppy"
        )),
    }
}

pub fn usage() -> String {
    "\
jefe-tutorial-capture - agent-driven tutorial capture workflow

Usage: jefe-tutorial-capture <SUBCOMMAND> [OPTIONS]

Subcommands:
  prepare           Create isolated local fixtures and manifest
  capture-local     Run the deterministic tmux capture scenario (shim-specific)
  capture-github    Run the opt-in GitHub Issues/PR capture scenario
  plan-github       Plan opt-in GitHub fixture mutations (allowlist-gated)
  render            Convert screen captures to SVG images
  report            Generate a Markdown evidence report
  cleanup           Remove only manifest-owned resources
  validate-runtime  Validate a real-runtime profile by launching Jefe via tmux and asserting runtime detection

Common options:
  -h, --help      Print this help message and exit

prepare:
  --run-id <ID>              Run identifier (default: auto-generated)
  --base-dir <DIR>           Base directory for run artifacts (default: /tmp/jefe-tutorial-capture)
  --scenario <NAME>          Scenario name (default: tutorial-capture-local)
  --runtime-profile <P>      Runtime profile: shim, real-llxprt, real-code-puppy (default: shim)
  --shim-availability <A>    Shim availability: llxprt-only, code-puppy-only, both (default: both)

capture-local:
  --manifest <FILE>    Path to the run manifest JSON
  --scenario <FILE>    Path to the tmux scenario JSON
  --jefe-bin <PATH>    Path to the jefe binary under test
  --keep-session       Keep tmux session alive after completion

capture-github:
  --manifest <FILE>    Path to the run manifest JSON; scenario is generated from it
  --jefe-bin <PATH>    Path to the jefe binary under test
  --keep-session       Keep tmux session alive after completion
  --merge              Use the merge variant (requires --allow-merge authorization from plan-github)

plan-github:
  --fixture-repo <REPO>     GitHub owner/repo for fixture (must be allowlisted)
  --run-id <ID>             Run ID for unique resource naming
  --manifest <FILE>         Path to prepared run manifest (optional)
  --allow-merge             Allow merging the fixture PR
  --dry-run                 Plan only; print the mutation plan without executing
  --confirm-disposable      Confirm fixture is disposable (required for actual execution)
  --allowlist-file <FILE>   Path to allowlist file (one owner/repo per line)
  --allow-repo <REPO>       Allowlist an additional repo (repeatable)
  --clone-dest <DIR>        Destination for gh repo clone

Environment:
  JEFE_TUTORIAL_FIXTURE_ALLOWLIST  Colon-separated allowlist of owner/repo fixtures

render:
  --manifest <FILE>    Path to the run manifest JSON

report:
  --manifest <FILE>    Path to the run manifest JSON

cleanup:
  --manifest <FILE>    Path to the run manifest JSON
  --purge-evidence     Also remove artifact/evidence directories (default: preserved)
  --dry-run            Preview what would be cleaned without modifying
  --confirm            Required to perform actual cleanup

validate-runtime:
  --manifest <FILE>    Path to the run manifest JSON
  --jefe-bin <PATH>    Path to the jefe binary under test
  --keep-session       Keep tmux session alive after completion
"
    .to_string()
}

pub fn write_stdout(message: &str) {
    let _ = std::io::stdout().write_all(message.as_bytes());
}

pub fn write_stderr(message: &str) {
    let _ = std::io::stderr().write_all(message.as_bytes());
}
