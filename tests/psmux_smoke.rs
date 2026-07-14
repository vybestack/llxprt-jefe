#![cfg(all(windows, feature = "psmux-smoke"))]

use std::ffi::OsString;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jefe::domain::AgentKind;
use jefe::runtime::{
    AgentExecutablePlatform, AgentExecutableResolver, LocalPlatform, MultiplexerIsolation,
    MultiplexerPlan,
};
use serde::Deserialize;

const MINIMUM_PSMUX_VERSION: PsmuxVersion = PsmuxVersion::new(3, 3, 6);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_TIMEOUT: Duration = Duration::from_secs(5);
const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-psmux-smoke-fixture");
const JEFE: &str = env!("CARGO_BIN_EXE_jefe");

/// Windows `STATUS_DLL_INIT_FAILED` as returned by `ExitStatus::code()`.
///
/// On Windows, `ExitStatus::from_raw(0xc000_0142)` causes `code()` to return
/// the raw NTSTATUS reinterpreted as a signed `i32` (two's complement of the
/// bit pattern `0xc000_0142`). We store that exact signed value here instead
/// of casting so Clippy's `as_conversions` lint stays clean.
const STATUS_DLL_INIT_FAILED: i32 = -1_073_741_502;
const MAX_VERSION_PROBE_ATTEMPTS: u32 = 4;
const VERSION_PROBE_BACKOFF: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PsmuxVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl PsmuxVersion {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        let token = value
            .split_whitespace()
            .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .ok_or_else(|| format!("version output contains no numeric token: {value:?}"))?;
        let mut parts = token.split('.');
        let major = parse_version_part(parts.next(), "major", value)?;
        let minor = parse_version_part(parts.next(), "minor", value)?;
        let patch = parse_version_part(parts.next(), "patch", value)?;
        Ok(Self::new(major, minor, patch))
    }
}

impl std::fmt::Display for PsmuxVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn parse_version_part(part: Option<&str>, name: &str, source: &str) -> Result<u32, String> {
    let value =
        part.ok_or_else(|| format!("version output has no {name} component: {source:?}"))?;
    value
        .trim_matches(|ch: char| !ch.is_ascii_digit())
        .parse::<u32>()
        .map_err(|error| format!("invalid {name} version component in {source:?}: {error}"))
}

fn is_retryable_version_probe_status(status: std::process::ExitStatus) -> bool {
    status.code() == Some(STATUS_DLL_INIT_FAILED)
}

#[derive(Debug)]
struct SmokeFailure {
    message: String,
    diagnostics: String,
}

impl std::fmt::Display for SmokeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}\n\n{}", self.message, self.diagnostics)
    }
}

impl std::error::Error for SmokeFailure {}

struct PsmuxNamespace {
    executable: PathBuf,
    name: String,
    version: String,
    transcript: String,
    artifact_dir: PathBuf,
}

impl PsmuxNamespace {
    fn new(executable: PathBuf, label: &str, version: &str) -> Result<Self, SmokeFailure> {
        let name = unique_name(label);
        let artifact_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("psmux-smoke")
            .join(&name);
        fs::create_dir_all(&artifact_dir).map_err(|error| SmokeFailure {
            message: format!("failed to create artifact directory: {error}"),
            diagnostics: format!("namespace: {name}\npath: {}", artifact_dir.display()),
        })?;
        Ok(Self {
            executable,
            name,
            version: version.to_owned(),
            transcript: String::new(),
            artifact_dir,
        })
    }

    fn run(&mut self, args: &[&str]) -> Result<Output, SmokeFailure> {
        let owned = args
            .iter()
            .map(|value| OsString::from(*value))
            .collect::<Vec<_>>();
        self.run_os(&owned)
    }

    fn run_os(&mut self, args: &[OsString]) -> Result<Output, SmokeFailure> {
        let mut command = Command::new(&self.executable);
        command.arg("-L").arg(&self.name).args(args);
        for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
            command.env_remove(variable);
        }
        let display = format_command(&self.executable, &self.name, args);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            self.failure(
                format!("failed to spawn {display}: {error}"),
                "status: not started\nstdout: \nstderr: ",
            )
        })?;
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let output = loop {
            match child.try_wait() {
                Ok(Some(_)) => break child.wait_with_output(),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let details = output
                        .as_ref()
                        .map_or_else(std::string::ToString::to_string, format_output);
                    return Err(self.failure(
                        format!("command timed out after {COMMAND_TIMEOUT:?}: {display}"),
                        &details,
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    return Err(self.failure(
                        format!("failed waiting for {display}: {error}"),
                        "status: wait failed\nstdout: \nstderr: ",
                    ));
                }
            }
        }
        .map_err(|error| self.failure(format!("failed collecting {display}: {error}"), ""))?;
        let _ = writeln!(self.transcript, "$ {display}\n{}", format_output(&output));
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.failure(
                format!("command failed: {display}"),
                &format_output(&output),
            ))
        }
    }

    fn capture(&mut self, session: &str) -> Result<String, SmokeFailure> {
        let output = self.run(&["capture-pane", "-p", "-S", "-100", "-t", session])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn wait_for_capture(&mut self, session: &str, needle: &str) -> Result<String, SmokeFailure> {
        let deadline = Instant::now() + POLL_TIMEOUT;
        let mut last = String::new();
        while Instant::now() < deadline {
            last = self.capture(session)?;
            if last.contains(needle) {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(self.failure(
            format!("pane did not contain {needle:?} within {POLL_TIMEOUT:?}"),
            &format!("last capture:\n{last}"),
        ))
    }

    fn wait_for_format(
        &mut self,
        session: &str,
        format: &str,
        expected: &str,
    ) -> Result<String, SmokeFailure> {
        let deadline = Instant::now() + POLL_TIMEOUT;
        let mut last = String::new();
        while Instant::now() < deadline {
            let output = self.run(&["display-message", "-p", "-t", session, format])?;
            last.clear();
            last.push_str(String::from_utf8_lossy(&output.stdout).trim());
            if last == expected {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(self.failure(
            format!("format {format:?} did not become {expected:?}"),
            &format!("last value: {last:?}"),
        ))
    }

    fn failure(&self, message: String, details: &str) -> SmokeFailure {
        let sessions = self.available_sessions();
        let diagnostics = format!(
            "psmux version: {}\nnamespace: {}\nartifact directory: {}\n{details}\n\navailable sessions:\n{sessions}\n\ntranscript:\n{}",
            self.version,
            self.name,
            self.artifact_dir.display(),
            self.transcript
        );
        let _ = fs::write(self.artifact_dir.join("failure.txt"), &diagnostics);
        SmokeFailure {
            message,
            diagnostics,
        }
    }

    fn available_sessions(&self) -> String {
        let output = Command::new(&self.executable)
            .arg("-L")
            .arg(&self.name)
            .args(["list-sessions", "-F", "#{session_name}"])
            .output();
        match output {
            Ok(output) => format_output(&output),
            Err(error) => format!("unable to list sessions: {error}"),
        }
    }

    fn cleanup(&mut self) {
        let output = Command::new(&self.executable)
            .arg("-L")
            .arg(&self.name)
            .arg("kill-server")
            .output();
        if let Ok(output) = output {
            let _ = writeln!(
                self.transcript,
                "$ {} -L {} kill-server\n{}",
                self.executable.display(),
                self.name,
                format_output(&output)
            );
        }
        let _ = fs::write(self.artifact_dir.join("transcript.txt"), &self.transcript);
    }
}

impl Drop for PsmuxNamespace {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[test]
fn psmux_minimum_version_parser_accepts_qualified_release() {
    let parsed = PsmuxVersion::parse("tmux 3.3.6\n");
    assert_eq!(parsed, Ok(MINIMUM_PSMUX_VERSION));
    assert!(PsmuxVersion::parse("tmux 3.3.5").is_ok_and(|version| version < MINIMUM_PSMUX_VERSION));
    assert!(PsmuxVersion::parse("psmux unknown").is_err());
}

#[derive(Debug, Deserialize)]
struct LaunchObservation {
    args: Vec<String>,
    cwd: String,
    selected_environment: Option<String>,
    tmux: Option<String>,
    tmux_pane: Option<String>,
    tmux_tmpdir: Option<String>,
}

struct AgentLaunchFixture {
    work_dir: tempfile::TempDir,
    agent_executable: jefe::runtime::ResolvedAgentExecutable,
    record: PathBuf,
    expected: Vec<&'static str>,
    launch_args: Vec<OsString>,
}

fn prepare_agent_launch_fixture() -> AgentLaunchFixture {
    let work_dir = tempfile::Builder::new()
        .prefix("jefe launch Ω ")
        .tempdir()
        .unwrap_or_else(|error| panic!("create launch directory: {error}"));
    let fixture_dir = work_dir.path().join("runtime space 犬");
    fs::create_dir_all(&fixture_dir)
        .unwrap_or_else(|error| panic!("create fixture directory: {error}"));
    fs::copy(FIXTURE, fixture_dir.join("code-puppy.exe"))
        .unwrap_or_else(|error| panic!("copy fixture executable: {error}"));
    let agent_executable = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::Windows,
        vec![fixture_dir],
        Some(OsString::from(".EXE;.CMD;.BAT")),
    )
    .resolve(AgentKind::CodePuppy)
    .unwrap_or_else(|error| panic!("resolve fixture executable: {error}"));
    let record = work_dir.path().join("launch observation.json");
    let expected = vec![
        "space value",
        "quote\"value",
        "amp&value",
        "paren(value)",
        "percent%value",
        "",
        "trailing\\",
        "Ω犬",
    ];
    let mut launch_args = vec![OsString::from("--record"), record.as_os_str().to_owned()];
    launch_args.extend(expected.iter().map(OsString::from));
    AgentLaunchFixture {
        work_dir,
        agent_executable,
        record,
        expected,
        launch_args,
    }
}

#[test]
fn psmux_agent_launch_preserves_arguments_working_directory_and_environment_policy() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable.clone(), "agent-launch", &version_text);
    let fixture = prepare_agent_launch_fixture();
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable,
        MultiplexerIsolation::Namespace(namespace.name.clone()),
    )
    .unwrap_or_else(|error| panic!("construct psmux plan: {error}"));
    let pane = plan
        .agent_pane_command_args_with_launcher(
            &fixture.agent_executable,
            &fixture.launch_args,
            &[(
                OsString::from("JEFE_FIXTURE_VALUE"),
                OsString::from("environment & (Ω) %value%"),
            )],
            Path::new(JEFE),
        )
        .unwrap_or_else(|error| panic!("build production pane command: {error}"));
    let mut command = vec![
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from("agent-launch"),
        OsString::from("-c"),
        fixture.work_dir.path().as_os_str().to_owned(),
    ];
    command.extend(pane);
    namespace
        .run_os(&command)
        .unwrap_or_else(|error| panic!("launch recording fixture through psmux: {error}"));
    let deadline = Instant::now() + POLL_TIMEOUT;
    while !fixture.record.is_file() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert_agent_launch_observation(&fixture);
    let session_status = namespace.run(&["has-session", "-t", "agent-launch"]);
    if session_status.is_ok() {
        namespace
            .run(&["kill-session", "-t", "agent-launch"])
            .unwrap_or_else(|error| panic!("clean up recording session: {error}"));
    }
}

fn assert_agent_launch_observation(fixture: &AgentLaunchFixture) {
    let bytes = fs::read(&fixture.record)
        .unwrap_or_else(|error| panic!("read fixture observation: {error}"));
    let observation: LaunchObservation = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode fixture observation: {error}"));
    assert_eq!(observation.args, fixture.expected);
    assert!(
        std::fs::canonicalize(&observation.cwd).is_ok_and(|observed| {
            std::fs::canonicalize(fixture.work_dir.path())
                .is_ok_and(|expected| observed == expected)
        }),
        "observed cwd: {}",
        observation.cwd
    );
    assert_eq!(
        observation.selected_environment.as_deref(),
        Some("environment & (Ω) %value%")
    );
    assert_eq!(observation.tmux, None);
    assert_eq!(observation.tmux_pane, None);
    assert_eq!(observation.tmux_tmpdir, None);
}

#[test]
fn psmux_command_contract_rejects_invalid_command_with_diagnostics() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable, "invalid", &version_text);
    let error = namespace.run(&["not-a-jefe-command"]);
    let Err(error) = error else {
        panic!("an invalid psmux command unexpectedly succeeded");
    };
    let report = error.to_string();
    assert!(report.contains("not-a-jefe-command"), "report: {report}");
    assert!(report.contains("status:"), "report: {report}");
    assert!(report.contains("stdout:"), "report: {report}");
    assert!(report.contains("stderr:"), "report: {report}");
    assert!(report.contains("namespace:"), "report: {report}");
    assert!(report.contains("psmux version:"), "report: {report}");
}

#[test]
fn psmux_supports_jefe_runtime_and_harness_command_surface() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable, "surface", &version_text);
    if let Err(error) = exercise_command_surface(&mut namespace) {
        panic!("{error}");
    }
}

#[test]
fn psmux_named_namespaces_are_isolated_and_cleanup_is_scoped() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut left = namespace_or_panic(executable.clone(), "left", &version_text);
    let mut right = namespace_or_panic(executable, "right", &version_text);
    if let Err(error) = exercise_namespace_isolation(&mut left, &mut right) {
        panic!("{error}");
    }
}

#[test]
fn psmux_four_recording_agents_remain_independent_and_scoped() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable, "four-agents", &version_text);
    let repo_one = tempfile::Builder::new()
        .prefix("jefe repo Ω one ")
        .tempdir()
        .unwrap_or_else(|error| panic!("create first repository fixture: {error}"));
    let repo_two = tempfile::Builder::new()
        .prefix("jefe repo two ")
        .tempdir()
        .unwrap_or_else(|error| panic!("create second repository fixture: {error}"));
    let agents = [
        ("llxprt-one", repo_one.path(), "A", "PSMUX_BYTE_41"),
        ("puppy-two", repo_one.path(), "B", "PSMUX_BYTE_42"),
        ("llxprt-three", repo_two.path(), "C", "PSMUX_BYTE_43"),
        ("puppy-four", repo_two.path(), "D", "PSMUX_BYTE_44"),
    ];
    for (session, work_dir, input, expected) in agents {
        namespace
            .run_os(&[
                OsString::from("new-session"),
                OsString::from("-d"),
                OsString::from("-s"),
                OsString::from(session),
                OsString::from("-c"),
                work_dir.as_os_str().to_owned(),
                OsString::from(FIXTURE),
            ])
            .unwrap_or_else(|error| panic!("create {session}: {error}"));
        namespace
            .wait_for_capture(session, "PSMUX_SMOKE_READY")
            .unwrap_or_else(|error| panic!("wait for {session}: {error}"));
        namespace
            .run(&["send-keys", "-l", "-t", session, "--", input])
            .unwrap_or_else(|error| panic!("interact with {session}: {error}"));
        namespace
            .wait_for_capture(session, expected)
            .unwrap_or_else(|error| panic!("verify {session}: {error}"));
    }
    namespace
        .run(&["kill-session", "-t", "puppy-two"])
        .unwrap_or_else(|error| panic!("kill selected agent: {error}"));
    assert!(namespace.run(&["has-session", "-t", "puppy-two"]).is_err());
    for survivor in ["llxprt-one", "llxprt-three", "puppy-four"] {
        namespace
            .run(&["has-session", "-t", survivor])
            .unwrap_or_else(|error| panic!("selected kill affected {survivor}: {error}"));
    }
}

fn exercise_command_surface(namespace: &mut PsmuxNamespace) -> Result<(), SmokeFailure> {
    let session = "jefe-smoke";
    let work_dir = tempfile::Builder::new()
        .prefix("jefe psmux Ω ")
        .tempdir()
        .map_err(|error| namespace.failure(format!("temp directory failed: {error}"), ""))?;
    let args = vec![
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from(session),
        OsString::from("-x"),
        OsString::from("100"),
        OsString::from("-y"),
        OsString::from("32"),
        OsString::from("-c"),
        work_dir.path().as_os_str().to_owned(),
        OsString::from(FIXTURE),
    ];
    namespace.run_os(&args)?;
    namespace.wait_for_capture(session, "PSMUX_SMOKE_READY")?;
    namespace.run(&["has-session", "-t", session])?;
    assert_session_listed(namespace, session)?;
    configure_options(namespace, session)?;
    assert_pane_formats(namespace, session)?;
    exercise_resize(namespace, session)?;
    exercise_input_and_capture(namespace, session)?;
    namespace.run(&["copy-mode", "-t", session])?;
    namespace.run(&["send-keys", "-t", session, "q"])?;
    namespace.run(&["send-keys", "-t", session, "C-d"])?;
    namespace.wait_for_format(session, "#{pane_dead}", "1")?;
    namespace.run(&["kill-session", "-t", session])?;
    Ok(())
}

fn assert_session_listed(
    namespace: &mut PsmuxNamespace,
    session: &str,
) -> Result<(), SmokeFailure> {
    let output = namespace.run(&["list-sessions", "-F", "#{session_name}"])?;
    let sessions = String::from_utf8_lossy(&output.stdout);
    if sessions.lines().any(|line| line.trim() == session) {
        Ok(())
    } else {
        Err(namespace.failure(
            format!("session {session:?} missing from list-sessions"),
            &format!("sessions:\n{sessions}"),
        ))
    }
}

fn configure_options(namespace: &mut PsmuxNamespace, session: &str) -> Result<(), SmokeFailure> {
    for option in ["prefix", "prefix2"] {
        namespace.run(&["set-option", "-t", session, option, "None"])?;
    }
    namespace.run(&["set-option", "-t", session, "remain-on-exit", "on"])?;
    namespace.run(&["set-option", "-g", "set-clipboard", "on"])?;
    namespace.run(&["set-option", "-gp", "allow-passthrough", "on"])?;
    namespace.run(&["set-option", "-p", "-t", session, "allow-passthrough", "on"])?;
    namespace.run(&["set-option", "-wt", session, "history-limit", "2000"])?;
    Ok(())
}

fn assert_pane_formats(namespace: &mut PsmuxNamespace, session: &str) -> Result<(), SmokeFailure> {
    let format = "#{session_name}|#{window_index}|#{pane_index}|#{pane_pid}|#{pane_dead}|#{pane_width}|#{pane_height}|#{history_size}";
    let output = namespace.run(&["list-panes", "-t", session, "-F", format])?;
    let value = String::from_utf8_lossy(&output.stdout);
    let parts = value.trim().split('|').collect::<Vec<_>>();
    if parts.len() == 8 && parts[0] == session && parts[3].parse::<u32>().is_ok() {
        Ok(())
    } else {
        Err(namespace.failure("unexpected list-panes format output".to_owned(), &value))
    }
}

fn exercise_resize(namespace: &mut PsmuxNamespace, session: &str) -> Result<(), SmokeFailure> {
    namespace.run(&["resize-window", "-t", session, "-x", "90", "-y", "28"])?;
    let output = namespace.run(&[
        "display-message",
        "-p",
        "-t",
        session,
        "#{pane_width}x#{pane_height}",
    ])?;
    let dimensions = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if dimensions == "90x28" || dimensions == "100x32" {
        Ok(())
    } else {
        Err(namespace.failure(
            "resize produced an unexpected pane size".to_owned(),
            &format!("observed dimensions: {dimensions}"),
        ))
    }
}

fn exercise_input_and_capture(
    namespace: &mut PsmuxNamespace,
    session: &str,
) -> Result<(), SmokeFailure> {
    namespace.run(&["send-keys", "-l", "-t", session, "--", "AΩ"])?;
    for key in ["Enter", "Escape", "Tab", "Up", "Down", "C-c"] {
        namespace.run(&["send-keys", "-t", session, key])?;
    }
    let expected = [
        "PSMUX_BYTE_41",
        "PSMUX_BYTE_CE",
        "PSMUX_BYTE_A9",
        "PSMUX_BYTE_0D",
        "PSMUX_BYTE_09",
        "PSMUX_BYTE_03",
    ];
    let capture = namespace.wait_for_capture(session, "PSMUX_BYTE_03")?;
    for needle in expected {
        if !capture.contains(needle) {
            return Err(namespace.failure(
                format!("captured pane is missing {needle}"),
                &format!("capture:\n{capture}"),
            ));
        }
    }
    let history_capture =
        namespace.run(&["capture-pane", "-p", "-S", "-20", "-E", "-", "-t", session])?;
    let history = namespace.run(&["display-message", "-p", "-t", session, "#{history_size}"])?;
    let history_value = String::from_utf8_lossy(&history.stdout)
        .trim()
        .parse::<u64>();
    if history_value.is_ok()
        && String::from_utf8_lossy(&history_capture.stdout).contains("PSMUX_SMOKE_READY")
    {
        Ok(())
    } else {
        Err(namespace.failure(
            "history capture or history_size format was unavailable".to_owned(),
            &format_output(&history),
        ))
    }
}

fn exercise_namespace_isolation(
    left: &mut PsmuxNamespace,
    right: &mut PsmuxNamespace,
) -> Result<(), SmokeFailure> {
    let fixture = OsString::from(FIXTURE);
    let left_args = [
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from("shared-name"),
        fixture.clone(),
    ];
    let right_args = [
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from("shared-name"),
        fixture,
    ];
    left.run_os(&left_args)?;
    right.run_os(&right_args)?;
    left.run(&["has-session", "-t", "shared-name"])?;
    right.run(&["has-session", "-t", "shared-name"])?;
    left.cleanup();
    right.run(&["has-session", "-t", "shared-name"])?;
    right.run(&["kill-session", "-t", "shared-name"])?;
    Ok(())
}

fn qualified_psmux() -> Option<(PathBuf, String)> {
    let executable =
        std::env::var_os("JEFE_PSMUX_BIN").map_or_else(|| PathBuf::from("psmux"), PathBuf::from);
    let result = probe_qualified_version(
        || Command::new(&executable).arg("-V").output(),
        thread::sleep,
    );
    match result {
        Ok((version, diagnostics)) => {
            if diagnostics.len() > 1 {
                let _ = writeln!(
                    std::io::stdout(),
                    "psmux version probe recovered:\n{}",
                    diagnostics.join("\n---\n")
                );
            }
            Some((executable, version))
        }
        Err(reason) => unavailable(&executable, &reason),
    }
}

fn probe_qualified_version<F, S>(
    mut probe: F,
    mut sleep: S,
) -> Result<(String, Vec<String>), String>
where
    F: FnMut() -> std::io::Result<Output>,
    S: FnMut(Duration),
{
    let mut diagnostics = Vec::new();
    for attempt in 1..=MAX_VERSION_PROBE_ATTEMPTS {
        let output = match probe() {
            Ok(output) => output,
            Err(error) => {
                diagnostics.push(format!(
                    "attempt {attempt}/{MAX_VERSION_PROBE_ATTEMPTS}: spawn failed: {error}"
                ));
                return Err(diagnostics.join("\n---\n"));
            }
        };
        diagnostics.push(format!(
            "attempt {attempt}/{MAX_VERSION_PROBE_ATTEMPTS}:\n{}",
            format_output(&output)
        ));
        if output.status.success() {
            let version = parse_qualified_version(&output).map_err(|error| {
                diagnostics.push(format!("qualification failed: {error}"));
                diagnostics.join("\n---\n")
            })?;
            return Ok((version, diagnostics));
        }
        if !is_retryable_version_probe_status(output.status) {
            return Err(diagnostics.join("\n---\n"));
        }
        if attempt < MAX_VERSION_PROBE_ATTEMPTS {
            sleep(VERSION_PROBE_BACKOFF);
        }
    }
    Err(format!(
        "STATUS_DLL_INIT_FAILED (0xc000_0142) persisted across {MAX_VERSION_PROBE_ATTEMPTS} attempts:\n{}",
        diagnostics.join("\n---\n")
    ))
}

fn parse_qualified_version(output: &Output) -> Result<String, String> {
    let version_text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let version = PsmuxVersion::parse(&version_text)?;
    if version < MINIMUM_PSMUX_VERSION {
        return Err(format!(
            "found {version}; minimum is {MINIMUM_PSMUX_VERSION}"
        ));
    }
    Ok(version_text)
}

fn unavailable(executable: &Path, reason: &str) -> Option<(PathBuf, String)> {
    let message = format!(
        "psmux smoke unavailable: executable={} reason={reason}. Install Winget package marlocarlo.psmux (minimum {MINIMUM_PSMUX_VERSION}).",
        executable.display()
    );
    assert!(
        !std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1"),
        "{message}"
    );
    let _ = writeln!(std::io::stdout(), "SKIP: {message}");
    None
}

fn namespace_or_panic(executable: PathBuf, label: &str, version: &str) -> PsmuxNamespace {
    match PsmuxNamespace::new(executable, label, version) {
        Ok(namespace) => namespace,
        Err(error) => panic!("{error}"),
    }
}

fn unique_name(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-psmux-{label}-{}-{nanos:x}", std::process::id())
}

fn format_command(executable: &Path, namespace: &str, args: &[OsString]) -> String {
    let arguments = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} -L {namespace} {arguments}", executable.display())
}

fn format_output(output: &Output) -> String {
    format!(
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

#[test]
fn version_probe_retry_is_limited_to_dll_init_failure() {
    use std::os::windows::process::ExitStatusExt;

    assert!(is_retryable_version_probe_status(
        std::process::ExitStatus::from_raw(0xc000_0142)
    ));
    assert!(!is_retryable_version_probe_status(
        std::process::ExitStatus::from_raw(1)
    ));
    assert!(!is_retryable_version_probe_status(
        std::process::ExitStatus::from_raw(0xc000_0005)
    ));
}

type ProbeAttempt = Result<Output, &'static str>;

fn probe_output(raw_status: u32, stdout: &str, stderr: &str) -> Output {
    use std::os::windows::process::ExitStatusExt;

    Output {
        status: std::process::ExitStatus::from_raw(raw_status),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

type ProbeResult = Result<(String, Vec<String>), String>;

struct ProbeSequenceOutcome {
    result: ProbeResult,
    probes: usize,
    sleeps: usize,
}

fn run_probe_sequence(sequence: Vec<ProbeAttempt>) -> ProbeSequenceOutcome {
    let mut sequence = std::collections::VecDeque::from(sequence);
    let probes = std::cell::Cell::new(0);
    let sleeps = std::cell::Cell::new(0);
    let result = probe_qualified_version(
        || {
            probes.set(probes.get() + 1);
            match sequence.pop_front() {
                Some(Ok(output)) => Ok(output),
                Some(Err(message)) => Err(std::io::Error::other(message)),
                None => panic!("probe sequence exhausted"),
            }
        },
        |_| sleeps.set(sleeps.get() + 1),
    );
    ProbeSequenceOutcome {
        result,
        probes: probes.get(),
        sleeps: sleeps.get(),
    }
}

fn assert_probe_error(outcome: ProbeSequenceOutcome) -> (String, usize, usize) {
    match outcome.result {
        Err(reason) => (reason, outcome.probes, outcome.sleeps),
        Ok((version, _)) => panic!("probe unexpectedly qualified as {version:?}"),
    }
}

fn assert_probe_ok(outcome: ProbeSequenceOutcome) -> ((String, Vec<String>), usize, usize) {
    match outcome.result {
        Ok(value) => (value, outcome.probes, outcome.sleeps),
        Err(reason) => panic!("probe unexpectedly failed: {reason}"),
    }
}

#[test]
fn version_probe_stops_immediately_on_non_retryable_failure() {
    let outcome = run_probe_sequence(vec![Ok(probe_output(1, "", "fatal"))]);
    let (reason, probes, sleeps) = assert_probe_error(outcome);
    assert_eq!((probes, sleeps), (1, 0));
    assert!(reason.contains("attempt 1/4") && reason.contains("fatal"));
}

#[test]
fn version_probe_recovers_after_loader_transient() {
    let sequence = vec![
        Ok(probe_output(0xc000_0142, "", "first transient")),
        Ok(probe_output(0, "tmux 3.3.6\n", "")),
    ];
    let outcome = run_probe_sequence(sequence);
    let ((version, diagnostics), probes, sleeps) = assert_probe_ok(outcome);
    assert_eq!((version.as_str(), probes, sleeps), ("tmux 3.3.6", 2, 1));
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics[0].contains("first transient"));
}

#[test]
fn version_probe_bounds_loader_retries_and_reports_every_attempt() {
    let sequence = (1..=4)
        .map(|attempt| {
            Ok(probe_output(
                0xc000_0142,
                "",
                &format!("transient-{attempt}"),
            ))
        })
        .collect();
    let outcome = run_probe_sequence(sequence);
    let (reason, probes, sleeps) = assert_probe_error(outcome);
    assert_eq!((probes, sleeps), (4, 3));
    for attempt in 1..=4 {
        assert!(reason.contains(&format!("transient-{attempt}")));
    }
}

#[test]
fn version_probe_reports_transient_before_terminal_failure() {
    let sequence = vec![
        Ok(probe_output(0xc000_0142, "", "first transient")),
        Ok(probe_output(1, "", "terminal failure")),
    ];
    let outcome = run_probe_sequence(sequence);
    let (reason, probes, sleeps) = assert_probe_error(outcome);
    assert_eq!((probes, sleeps), (2, 1));
    assert!(reason.contains("first transient") && reason.contains("terminal failure"));
}

#[test]
fn version_probe_reports_spawn_failure_after_transient() {
    let sequence = vec![
        Ok(probe_output(0xc000_0142, "", "first transient")),
        Err("spawn unavailable"),
    ];
    let outcome = run_probe_sequence(sequence);
    let (reason, probes, sleeps) = assert_probe_error(outcome);
    assert_eq!((probes, sleeps), (2, 1));
    assert!(reason.contains("first transient") && reason.contains("spawn unavailable"));
}
