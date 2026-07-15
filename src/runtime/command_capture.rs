//! Bounded subprocess capture with concurrent pipe draining and tree cleanup.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::errors::RuntimeError;

const POLL_INTERVAL: Duration = Duration::from_millis(10);
type PipeResult = io::Result<Vec<u8>>;

struct PipeDrain {
    receiver: Receiver<PipeResult>,
    handle: JoinHandle<()>,
    result: Option<PipeResult>,
}

impl PipeDrain {
    fn start<R>(mut pipe: R) -> Self
    where
        R: Read + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        });
        Self {
            receiver,
            handle,
            result: None,
        }
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.result.is_some() {
            return Ok(());
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.result = Some(result);
                Ok(())
            }
            Err(TryRecvError::Empty) => Ok(()),
            Err(TryRecvError::Disconnected) => Err("pipe reader stopped unexpectedly".to_owned()),
        }
    }

    fn is_complete(&self) -> bool {
        self.result.is_some()
    }

    fn finish(mut self, stream: &str) -> Result<Vec<u8>, String> {
        if self.result.is_none() {
            self.result = Some(
                self.receiver
                    .recv()
                    .map_err(|_| format!("{stream} pipe reader stopped unexpectedly"))?,
            );
        }
        self.handle
            .join()
            .map_err(|_| format!("{stream} pipe reader panicked"))?;
        match self.result {
            Some(Ok(bytes)) => Ok(bytes),
            Some(Err(error)) => Err(format!("could not read {stream}: {error}")),
            None => Err(format!("{stream} pipe reader returned no result")),
        }
    }
}

enum WaitFailure {
    Timeout,
    Process(String),
}

fn wait_for_process_and_pipes(
    child: &mut Child,
    stdout: &mut PipeDrain,
    stderr: &mut PipeDrain,
    timeout: Duration,
) -> Result<ExitStatus, WaitFailure> {
    let started = Instant::now();
    let mut status = None;
    loop {
        stdout.poll().map_err(WaitFailure::Process)?;
        stderr.poll().map_err(WaitFailure::Process)?;
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|error| WaitFailure::Process(error.to_string()))?;
        }
        if let Some(exit_status) = status
            && stdout.is_complete()
            && stderr.is_complete()
        {
            return Ok(exit_status);
        }
        if started.elapsed() >= timeout {
            return Err(WaitFailure::Timeout);
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
    let _ = Command::new("kill")
        .args(["-TERM", process_group.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("kill")
        .args(["-KILL", process_group.as_str()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn capture_error(error_context: &str, detail: &str) -> RuntimeError {
    RuntimeError::RemoteExecutionFailed(format!("{error_context}: {detail}"))
}

/// Capture both output streams while enforcing a deadline for the whole process tree.
pub(super) fn run_command_capture_with_timeout(
    mut command: Command,
    timeout: Duration,
    error_context: &str,
) -> Result<Output, RuntimeError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_tree(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| capture_error(error_context, &error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| capture_error(error_context, "spawned command did not expose stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| capture_error(error_context, "spawned command did not expose stderr"))?;
    let mut stdout = PipeDrain::start(stdout);
    let mut stderr = PipeDrain::start(stderr);

    let status = match wait_for_process_and_pipes(&mut child, &mut stdout, &mut stderr, timeout) {
        Ok(status) => status,
        Err(failure) => {
            terminate_process_tree(&mut child);
            let _ = stdout.finish("stdout");
            let _ = stderr.finish("stderr");
            return match failure {
                WaitFailure::Timeout => Err(capture_error(
                    error_context,
                    &format!("timed out after {}s", timeout.as_secs_f64()),
                )),
                WaitFailure::Process(detail) => Err(capture_error(error_context, &detail)),
            };
        }
    };
    let stdout = stdout
        .finish("stdout")
        .map_err(|detail| capture_error(error_context, &detail))?;
    let stderr = stderr
        .finish("stderr")
        .map_err(|detail| capture_error(error_context, &detail))?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
