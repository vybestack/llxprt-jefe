//! JSP/1 compliance CLI with stable JSON stdout on every reportable failure.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jefe::jsp::v1::compliance::challenge::{AdapterKind, RunnerChallenge};
use jefe::jsp::v1::compliance::profile::{ProfileError, validate_producer_trace_with_challenge};
use jefe::jsp::v1::compliance::report::{
    COMPLIANCE_ARTIFACT_VERSION, ReportOutcome, StabilityFailure, StabilityReport,
};
use jefe::jsp::v1::compliance::scenario::{ScenarioOracle, load_scenario, load_scenario_package};
use jefe::jsp::v1::compliance::schema::{
    default_fixtures_dir, default_schemas_dir, run_schema_oracle,
};
use jefe::jsp::v1::compliance::server_profile::validate_server_transcript_with_challenge;
use jefe::jsp::v1::compliance::{invoke_adapter, run_reference_adapter};

fn main() -> ExitCode {
    match parse_args(std::env::args_os()) {
        Ok(ParsedArgs::Help) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Ok(ParsedArgs::Run(arguments)) => {
            let report = match run(&arguments) {
                Ok(report) => report,
                Err(error) => error_report("cli", &error),
            };
            emit_report_and_exit(report)
        }
        Err(error) => emit_report_and_exit(error_report("cli", &error)),
    }
}

fn print_usage() {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = writeln!(lock, "{}", usage());
}

fn emit_report_and_exit(report: StabilityReport) -> ExitCode {
    let success = report.outcome == ReportOutcome::Pass;
    match emit_report(&report) {
        Ok(()) if success => ExitCode::SUCCESS,
        Ok(()) => ExitCode::from(1),
        Err(_) => ExitCode::from(2),
    }
}

struct Arguments {
    profile: String,
    root: PathBuf,
    input: Option<PathBuf>,
    adapter: Option<Vec<String>>,
    reference_adapter: bool,
    nonce: u64,
}

/// Distinguishes an explicit help request (stdout, exit 0) from a real run or
/// a closed/invalid invocation (stable JSON, nonzero exit).
enum ParsedArgs {
    Run(Arguments),
    Help,
}

fn run(arguments: &Arguments) -> Result<StabilityReport, String> {
    match arguments.profile.as_str() {
        "schema" => {
            no_input(arguments.input.as_ref()).and_then(|()| run_schema_profile(&arguments.root))
        }
        "reducer" => {
            no_input(arguments.input.as_ref()).and_then(|()| run_reducer_profile(&arguments.root))
        }
        "producer" => run_producer_profile(arguments),
        "server" => run_server_profile(arguments),
        "all" => no_input(arguments.input.as_ref()).map(|()| run_all_profiles(&arguments.root)),
        _ => Err("unknown profile; expected schema|reducer|producer|server|all".to_string()),
    }
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<ParsedArgs, String> {
    let mut arguments = arguments.into_iter();
    let _program = arguments.next();
    let first = arguments.next();
    if first.as_ref().is_some_and(is_help_flag) {
        return Ok(ParsedArgs::Help);
    }
    let profile = utf8_option(first, "profile")?;
    let root = std::env::current_dir().map_err(|_| "current directory unavailable".to_string())?;
    let mut builder = ArgBuilder::new(root);
    while let Some(argument) = arguments.next() {
        if is_help_flag(&argument) {
            return Ok(ParsedArgs::Help);
        }
        let flag = argument
            .to_str()
            .ok_or_else(|| "argument is not valid UTF-8".to_string())?;
        builder.apply_flag(flag, &mut arguments)?;
    }
    Ok(ParsedArgs::Run(builder.finish(profile)))
}

struct ArgBuilder {
    root: PathBuf,
    input: Option<PathBuf>,
    adapter: Option<Vec<String>>,
    reference_adapter: bool,
    nonce: u64,
}

impl ArgBuilder {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            input: None,
            adapter: None,
            reference_adapter: false,
            nonce: 0,
        }
    }

    fn finish(self, profile: String) -> Arguments {
        Arguments {
            profile,
            root: self.root,
            input: self.input,
            adapter: self.adapter,
            reference_adapter: self.reference_adapter,
            nonce: self.nonce,
        }
    }

    fn apply_flag(
        &mut self,
        flag: &str,
        arguments: &mut impl Iterator<Item = OsString>,
    ) -> Result<(), String> {
        match flag {
            "--root" => {
                self.root = PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--root requires a path".to_string())?,
                );
            }
            "--input" => {
                if self.input.is_some() {
                    return Err("--input may be specified once".to_string());
                }
                self.input = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| "--input requires a file".to_string())?,
                ));
            }
            "--adapter" => {
                if self.adapter.is_some() || self.reference_adapter {
                    return Err("--adapter may be specified once".to_string());
                }
                let spec = arguments
                    .next()
                    .ok_or_else(|| "--adapter requires a command".to_string())?
                    .into_string()
                    .map_err(|_| "adapter command is not valid UTF-8".to_string())?;
                self.adapter = Some(
                    spec.split_whitespace()
                        .map(std::string::ToString::to_string)
                        .collect(),
                );
            }
            "--reference-adapter" => {
                if self.adapter.is_some() || self.reference_adapter {
                    return Err("--reference-adapter may be specified once".to_string());
                }
                self.reference_adapter = true;
            }
            "--nonce" => {
                let raw = arguments
                    .next()
                    .ok_or_else(|| "--nonce requires a value".to_string())?
                    .into_string()
                    .map_err(|_| "--nonce value is not valid UTF-8".to_string())?;
                self.nonce = raw
                    .parse()
                    .map_err(|_| "--nonce must be a non-negative integer".to_string())?;
            }
            _ => return Err("unknown argument".to_string()),
        }
        Ok(())
    }
}

/// Whether an OsString is the `--help` or `-h` flag (UTF-8 exact match only,
/// so non-UTF-8 arguments are never mistaken for a help request).
fn is_help_flag(argument: &OsString) -> bool {
    matches!(argument.to_str(), Some("--help" | "-h"))
}

fn utf8_option(value: Option<OsString>, label: &str) -> Result<String, String> {
    value
        .ok_or_else(|| "missing required profile argument".to_string())?
        .into_string()
        .map_err(|_| format!("{label} is not valid UTF-8"))
}

fn usage() -> String {
    "usage: jefe-jsp-compliance <schema|reducer|producer|server|all> [--root DIR] [--input FILE] [--adapter COMMAND] [--reference-adapter] [--nonce N]"
        .to_string()
}

fn no_input(input: Option<&PathBuf>) -> Result<(), String> {
    if input.is_some() {
        Err("--input is supported only by producer and server profiles".to_string())
    } else {
        Ok(())
    }
}

fn run_schema_profile(root: &Path) -> Result<StabilityReport, String> {
    let report = run_schema_oracle(&default_schemas_dir(root), &default_fixtures_dir(root))
        .map_err(|error| format!("schema oracle: {error}"))?;
    let checks = report.positive_count + report.negative_count;
    if report.passed {
        Ok(StabilityReport::pass(
            "schema",
            COMPLIANCE_ARTIFACT_VERSION,
            checks,
        ))
    } else {
        Ok(StabilityReport::fail(
            "schema",
            COMPLIANCE_ARTIFACT_VERSION,
            checks,
            report
                .findings
                .into_iter()
                .map(|finding| {
                    failure(
                        &finding.kind,
                        &format!("{}: {}", finding.document, finding.detail),
                    )
                })
                .collect(),
        ))
    }
}

fn run_reducer_profile(root: &Path) -> Result<StabilityReport, String> {
    let directory = root.join("dev-docs/jsp/v1/compliance/scenarios");
    let entries = load_scenario_package(&directory)
        .map_err(|_| "scenario package: JSP-C-SCENARIO-PACKAGE".to_string())?;
    let mut failures = Vec::new();
    let mut checks = 0;
    for entry in entries {
        let scenario =
            load_scenario(&entry.path).map_err(|_| "scenario: JSP-C-SCENARIO-LOAD".to_string())?;
        let result = ScenarioOracle::evaluate(&scenario);
        checks += result.steps.len();
        for step in result.steps.into_iter().filter(|step| !step.passed) {
            failures.push(StabilityFailure {
                invariant: "scenario_step".to_string(),
                scenario: Some(result.id.clone()),
                step: Some(step.index),
                expected_sequence: step.expected_sequence,
                actual_sequence: step.actual_sequence,
                detail: match step.failure {
                    Some(detail) => detail,
                    None => "scenario step failed".to_string(),
                },
            });
        }
    }
    Ok(report_from_failures("reducer", checks, failures))
}

fn run_producer_profile(arguments: &Arguments) -> Result<StabilityReport, String> {
    let challenge = RunnerChallenge::reference(AdapterKind::Producer, arguments.nonce);
    let bytes = if arguments.reference_adapter {
        run_reference_adapter_for_challenge(&challenge)?
    } else if let Some(adapter) = &arguments.adapter {
        invoke_adapter_for_challenge(adapter, &challenge)?
    } else {
        return Err("producer qualification requires --adapter or --reference-adapter".to_string());
    };
    let report = validate_producer_trace_with_challenge(&bytes, &challenge);
    Ok(profile_report(
        "producer",
        report.document_count,
        report.findings,
    ))
}

fn run_server_profile(arguments: &Arguments) -> Result<StabilityReport, String> {
    let challenge = RunnerChallenge::reference(AdapterKind::Server, arguments.nonce);
    let bytes = if arguments.reference_adapter {
        run_reference_adapter_for_challenge(&challenge)?
    } else if let Some(adapter) = &arguments.adapter {
        invoke_adapter_for_challenge(adapter, &challenge)?
    } else {
        return Err("server qualification requires --adapter or --reference-adapter".to_string());
    };
    let report = validate_server_transcript_with_challenge(&bytes, &challenge);
    Ok(profile_report(
        "server",
        report.interaction_count,
        report.findings,
    ))
}

fn challenge_json(challenge: &RunnerChallenge) -> Result<Vec<u8>, String> {
    serde_json::to_vec(challenge).map_err(|_| "challenge serialization failed".to_string())
}

fn run_reference_adapter_for_challenge(challenge: &RunnerChallenge) -> Result<Vec<u8>, String> {
    let bytes = challenge_json(challenge)?;
    let output = run_reference_adapter(&bytes)
        .map_err(|error| format!("reference adapter: {}", error.code()))?;
    Ok(output.stdout)
}

fn invoke_adapter_for_challenge(
    adapter: &[String],
    challenge: &RunnerChallenge,
) -> Result<Vec<u8>, String> {
    let bytes = challenge_json(challenge)?;
    let output = invoke_adapter(adapter, &bytes)
        .map_err(|error| format!("adapter invocation: {}", error.code()))?;
    Ok(output.stdout)
}

fn run_all_profiles(root: &Path) -> StabilityReport {
    // `all` aggregates every profile even when one returns a fatal I/O or shape
    // error: each error is converted into a per-profile failure rather than
    // short-circuiting the remaining profiles. This guarantees a single
    // deterministic combined report regardless of which profile fails first.
    let manifest = validate_top_manifest(root);
    let schema = match run_schema_profile(root) {
        Ok(report) => report,
        Err(error) => profile_fatal_report("schema", &error),
    };
    let reducer = match run_reducer_profile(root) {
        Ok(report) => report,
        Err(error) => profile_fatal_report("reducer", &error),
    };
    let default_args = Arguments {
        profile: "producer".to_string(),
        root: root.to_path_buf(),
        input: None,
        adapter: None,
        reference_adapter: true,
        nonce: 477,
    };
    let producer = match run_producer_profile(&default_args) {
        Ok(report) => report,
        Err(error) => profile_fatal_report("producer", &error),
    };
    let server = match run_server_profile(&default_args) {
        Ok(report) => report,
        Err(error) => profile_fatal_report("server", &error),
    };
    let reports = [schema, reducer, producer, server, manifest];
    let checks = reports.iter().map(|report| report.checks_total).sum();
    let failures = reports
        .into_iter()
        .flat_map(|report| report.failures)
        .collect();
    report_from_failures("all", checks, failures)
}

/// Validate the top-level compliance manifest's artifact version, paths,
/// scenario count, and profile inventory.
fn validate_top_manifest(root: &Path) -> StabilityReport {
    const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
    let manifest_path = root.join("dev-docs/jsp/v1/compliance/manifest.json");
    let Ok(bytes) = std::fs::read(&manifest_path) else {
        return fail_manifest("manifest read failed");
    };
    if bytes.len() > MAX_MANIFEST_BYTES {
        return fail_manifest("manifest artifact exceeds bound");
    }
    let Ok(manifest) = jefe::jsp::v1::compliance::parse_compliance_manifest(&bytes) else {
        return fail_manifest("manifest shape rejected");
    };
    let errors = jefe::jsp::v1::compliance::validate_manifest_contents(&manifest);
    if errors.is_empty() {
        StabilityReport::pass("manifest", COMPLIANCE_ARTIFACT_VERSION, 1)
    } else {
        StabilityReport::fail(
            "manifest",
            COMPLIANCE_ARTIFACT_VERSION,
            1,
            errors
                .into_iter()
                .map(|detail| failure("manifest_inventory", &detail))
                .collect(),
        )
    }
}

fn fail_manifest(detail: &str) -> StabilityReport {
    StabilityReport::fail(
        "manifest",
        COMPLIANCE_ARTIFACT_VERSION,
        1,
        vec![failure("manifest_read", detail)],
    )
}

/// Convert a fatal profile error into a single-check failed per-profile
/// report so `all` can keep aggregating instead of short-circuiting.
fn profile_fatal_report(profile: &str, error: &str) -> StabilityReport {
    StabilityReport::fail(
        profile,
        COMPLIANCE_ARTIFACT_VERSION,
        1,
        vec![failure(
            "profile_fatal",
            &format!("{profile} profile failed: {error}"),
        )],
    )
}

fn profile_report(profile: &str, checks: usize, findings: Vec<ProfileError>) -> StabilityReport {
    report_from_failures(
        profile,
        checks,
        findings
            .into_iter()
            .map(|finding| failure(&finding.invariant, &finding.detail))
            .collect(),
    )
}

fn report_from_failures(
    profile: &str,
    checks: usize,
    failures: Vec<StabilityFailure>,
) -> StabilityReport {
    if failures.is_empty() {
        StabilityReport::pass(profile, COMPLIANCE_ARTIFACT_VERSION, checks)
    } else {
        StabilityReport::fail(profile, COMPLIANCE_ARTIFACT_VERSION, checks, failures)
    }
}

fn failure(invariant: &str, detail: &str) -> StabilityFailure {
    StabilityFailure {
        invariant: invariant.to_string(),
        scenario: None,
        step: None,
        expected_sequence: None,
        actual_sequence: None,
        detail: detail.to_string(),
    }
}

fn error_report(profile: &str, detail: &str) -> StabilityReport {
    StabilityReport::fail(
        profile,
        COMPLIANCE_ARTIFACT_VERSION,
        1,
        vec![failure("cli_input", detail)],
    )
}

fn emit_report(report: &StabilityReport) -> Result<(), String> {
    let json = report
        .to_json_string()
        .map_err(|error| format!("serialize report: {error}"))?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(json.as_bytes())
        .and_then(|()| stdout.write_all(b"\n"))
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("write report: {error}"))
}
