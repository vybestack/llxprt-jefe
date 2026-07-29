//! Bounded process capture used only by the definition-driven agent probe.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::domain::agent_definition::limits::PROBE_STREAM_LIMIT;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
type PipeResult = io::Result<StreamCapture>;

/// One independently bounded process stream.
pub(super) struct StreamCapture {
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
}

/// One completed probe process.
pub(super) struct ProbeProcessOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: StreamCapture,
    pub(super) stderr: StreamCapture,
}

/// Failures at the process/capture boundary.
pub(super) enum ProbeProcessError {
    Timeout,
    Failed(String),
}

struct PipeDrain {
    receiver: Receiver<PipeResult>,
    handle: JoinHandle<()>,
    result: Option<PipeResult>,
}

impl PipeDrain {
    fn start<R>(mut pipe: R, stream: &'static str) -> Result<Self, ProbeProcessError>
    where
        R: Read + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name(format!("agent-probe-{stream}"))
            .spawn(move || {
                let result = read_bounded(&mut pipe);
                let _ = sender.send(result);
            })
            .map_err(|error| ProbeProcessError::Failed(error.to_string()))?;
        Ok(Self {
            receiver,
            handle,
            result: None,
        })
    }

    fn poll(&mut self) -> Result<(), ProbeProcessError> {
        if self.result.is_some() {
            return Ok(());
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.result = Some(result);
                Ok(())
            }
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => Err(ProbeProcessError::Failed(
                "probe pipe reader stopped unexpectedly".to_string(),
            )),
        }
    }

    fn is_complete(&self) -> bool {
        self.result.is_some()
    }

    fn finish(mut self, stream: &str) -> Result<StreamCapture, ProbeProcessError> {
        if self.result.is_none() {
            let result = self.receiver.recv().map_err(|_| {
                ProbeProcessError::Failed(format!("{stream} pipe reader stopped unexpectedly"))
            })?;
            self.result = Some(result);
        }
        self.handle.join().map_err(|_| {
            ProbeProcessError::Failed(format!("{stream} pipe reader stopped unexpectedly"))
        })?;
        match self.result {
            Some(Ok(capture)) => Ok(capture),
            Some(Err(error)) => Err(ProbeProcessError::Failed(format!(
                "could not read {stream}: {error}"
            ))),
            None => Err(ProbeProcessError::Failed(format!(
                "{stream} pipe reader returned no result"
            ))),
        }
    }
}

fn read_bounded(pipe: &mut impl Read) -> io::Result<StreamCapture> {
    let mut bytes = Vec::with_capacity(PROBE_STREAM_LIMIT);
    let mut buffer = [0_u8; 8_192];
    let mut truncated = false;
    loop {
        let count = pipe.read(&mut buffer)?;
        if count == 0 {
            return Ok(StreamCapture { bytes, truncated });
        }
        let retained = PROBE_STREAM_LIMIT.saturating_sub(bytes.len()).min(count);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < count;
    }
}

/// Run one fixed-argv probe before the shared total deadline.
pub(super) fn run_probe_process(
    mut command: Command,
    deadline: Instant,
) -> Result<ProbeProcessOutput, ProbeProcessError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_tree(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| ProbeProcessError::Failed(error.to_string()))?;
    let result = capture_child(&mut child, deadline);
    if result.is_err() {
        terminate_process_tree(&mut child);
    }
    result
}

fn capture_child(
    child: &mut Child,
    deadline: Instant,
) -> Result<ProbeProcessOutput, ProbeProcessError> {
    let stdout = child.stdout.take().ok_or_else(|| {
        ProbeProcessError::Failed("spawned probe did not expose stdout".to_string())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        ProbeProcessError::Failed("spawned probe did not expose stderr".to_string())
    })?;
    let mut stdout = PipeDrain::start(stdout, "stdout")?;
    let mut stderr = PipeDrain::start(stderr, "stderr")?;
    let status = wait_for_process(child, &mut stdout, &mut stderr, deadline)?;
    let stdout = stdout.finish("stdout")?;
    let stderr = stderr.finish("stderr")?;
    Ok(ProbeProcessOutput {
        status,
        stdout,
        stderr,
    })
}

fn wait_for_process(
    child: &mut Child,
    stdout: &mut PipeDrain,
    stderr: &mut PipeDrain,
    deadline: Instant,
) -> Result<ExitStatus, ProbeProcessError> {
    let mut status = None;
    loop {
        stdout.poll()?;
        stderr.poll()?;
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|error| ProbeProcessError::Failed(error.to_string()))?;
        }
        if let Some(exit_status) = status
            && stdout.is_complete()
            && stderr.is_complete()
        {
            return Ok(exit_status);
        }
        if Instant::now() >= deadline {
            return Err(ProbeProcessError::Timeout);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn configure_process_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_tree(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut Child) {
    let process_group = format!("-{}", child.id());
    for signal in ["-TERM", "-KILL"] {
        let _ = Command::new("kill")
            .args([signal, "--", process_group.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
