//! Bounded subprocess adapter invocation.
//!
//! Challenge stdin and response stdout use temporary regular files so a
//! non-reading or over-producing child cannot fill a pipe and deadlock the
//! synchronous runner. Stderr is discarded and every public error is a stable,
//! payload-free code.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_ADAPTER_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ADAPTER_INPUT_BYTES: usize = 1024 * 1024;
const ADAPTER_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterInvocationError {
    InputTooLarge,
    SpawnFailed,
    TimedOut,
    OutputTooLarge,
    NonZeroExit,
    StdinWrite,
    StdoutRead,
}

impl AdapterInvocationError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InputTooLarge => "JSP-C-ADAPTER-INPUT-TOO-LARGE",
            Self::SpawnFailed => "JSP-C-ADAPTER-SPAWN-FAILED",
            Self::TimedOut => "JSP-C-ADAPTER-TIMED-OUT",
            Self::OutputTooLarge => "JSP-C-ADAPTER-OUTPUT-TOO-LARGE",
            Self::NonZeroExit => "JSP-C-ADAPTER-NONZERO-EXIT",
            Self::StdinWrite => "JSP-C-ADAPTER-STDIN-WRITE",
            Self::StdoutRead => "JSP-C-ADAPTER-STDOUT-READ",
        }
    }
}

impl std::fmt::Display for AdapterInvocationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}
impl std::error::Error for AdapterInvocationError {}

#[derive(Debug, Clone)]
pub struct AdapterOutput {
    pub stdout: Vec<u8>,
}

struct CaptureFile {
    path: PathBuf,
    file: File,
}

impl CaptureFile {
    fn create(label: &str) -> Result<Self, AdapterInvocationError> {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        for attempt in 0..16_u8 {
            let path = std::env::temp_dir().join(format!(
                "jefe-jsp-{}-{}-{}-{}",
                label,
                std::process::id(),
                nanos,
                attempt
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(AdapterInvocationError::SpawnFailed),
            }
        }
        Err(AdapterInvocationError::SpawnFailed)
    }
}

impl Drop for CaptureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Invoke an external adapter with a bounded stdin challenge, deadline, and
/// bounded stdout capture.
pub fn invoke_adapter(
    command_spec: &[String],
    challenge_json: &[u8],
) -> Result<AdapterOutput, AdapterInvocationError> {
    if challenge_json.len() > MAX_ADAPTER_INPUT_BYTES {
        return Err(AdapterInvocationError::InputTooLarge);
    }
    let Some(program) = command_spec.first() else {
        return Err(AdapterInvocationError::SpawnFailed);
    };
    if program.is_empty() {
        return Err(AdapterInvocationError::SpawnFailed);
    }

    let mut input = CaptureFile::create("input")?;
    input
        .file
        .write_all(challenge_json)
        .map_err(|_| AdapterInvocationError::StdinWrite)?;
    input
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|_| AdapterInvocationError::StdinWrite)?;
    let mut output = CaptureFile::create("output")?;
    let stdin = input
        .file
        .try_clone()
        .map_err(|_| AdapterInvocationError::SpawnFailed)?;
    let stdout = output
        .file
        .try_clone()
        .map_err(|_| AdapterInvocationError::SpawnFailed)?;

    let mut child = Command::new(program)
        .args(&command_spec[1..])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| AdapterInvocationError::SpawnFailed)?;
    wait_for_completion(&mut child)?;

    output
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|_| AdapterInvocationError::StdoutRead)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut output.file)
        .take((MAX_ADAPTER_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| AdapterInvocationError::StdoutRead)?;
    if bytes.len() > MAX_ADAPTER_OUTPUT_BYTES {
        return Err(AdapterInvocationError::OutputTooLarge);
    }
    Ok(AdapterOutput { stdout: bytes })
}

fn wait_for_completion(child: &mut std::process::Child) -> Result<(), AdapterInvocationError> {
    let deadline = Instant::now() + ADAPTER_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(AdapterInvocationError::NonZeroExit),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdapterInvocationError::TimedOut);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdapterInvocationError::StdoutRead);
            }
        }
    }
}

pub fn run_reference_adapter(
    challenge_json: &[u8],
) -> Result<AdapterOutput, AdapterInvocationError> {
    super::reference_adapter::run(challenge_json)
        .map(|stdout| AdapterOutput { stdout })
        .ok_or(AdapterInvocationError::NonZeroExit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_is_bounded_and_payload_free() {
        assert_eq!(
            invoke_adapter(&[], b"{}").err(),
            Some(AdapterInvocationError::SpawnFailed)
        );
        assert_eq!(
            invoke_adapter(&["false".to_string()], b"{}").err(),
            Some(AdapterInvocationError::NonZeroExit)
        );
        assert_eq!(
            invoke_adapter(&["true".to_string()], &vec![0; MAX_ADAPTER_INPUT_BYTES + 1]).err(),
            Some(AdapterInvocationError::InputTooLarge)
        );
    }

    #[test]
    fn regular_file_capture_avoids_pipe_deadlock() {
        let output = invoke_adapter(&["cat".to_string()], b"challenge");
        let Ok(output) = output else {
            panic!("cat adapter failed");
        };
        assert_eq!(output.stdout, b"challenge");
    }
}
