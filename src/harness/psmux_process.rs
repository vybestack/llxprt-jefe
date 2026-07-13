//! Bounded native process collection for the Windows psmux boundary.

use std::io::Read;
use std::process::{Child, ExitStatus, Output};
use std::time::{Duration, Instant};

use super::tmux_driver::TmuxDriverError;

type PipeReader = std::thread::JoinHandle<std::io::Result<Vec<u8>>>;

pub(super) fn wait_for_command(
    mut child: Child,
    command_name: &str,
    timeout: Duration,
) -> Result<Output, TmuxDriverError> {
    let stdout = child.stdout.take().map(spawn_pipe_reader);
    let stderr = child.stderr.take().map(spawn_pipe_reader);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                drain_pipe(stdout);
                drain_pipe(stderr);
                return Err(TmuxDriverError::Timeout {
                    command: command_name.to_string(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                drain_pipe(stdout);
                drain_pipe(stderr);
                return Err(spawn_error(command_name, error.to_string()));
            }
        }
    };
    collect_output(status, stdout, stderr, command_name)
}

fn spawn_pipe_reader<R: Read + Send + 'static>(mut pipe: R) -> PipeReader {
    std::thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn drain_pipe(reader: Option<PipeReader>) {
    if let Some(reader) = reader {
        let _ = reader.join();
    }
}

fn collect_output(
    status: ExitStatus,
    stdout: Option<PipeReader>,
    stderr: Option<PipeReader>,
    command_name: &str,
) -> Result<Output, TmuxDriverError> {
    let output = Output {
        status,
        stdout: join_pipe(stdout, command_name)?,
        stderr: join_pipe(stderr, command_name)?,
    };
    if output.status.success() {
        Ok(output)
    } else {
        Err(TmuxDriverError::Failed {
            command: command_name.to_string(),
            stderr: format_output(&output),
        })
    }
}

fn join_pipe(reader: Option<PipeReader>, command_name: &str) -> Result<Vec<u8>, TmuxDriverError> {
    let Some(reader) = reader else {
        return Ok(Vec::new());
    };
    reader
        .join()
        .map_err(|_| spawn_error(command_name, "output reader panicked".to_string()))?
        .map_err(|error| spawn_error(command_name, error.to_string()))
}

fn spawn_error(command_name: &str, reason: String) -> TmuxDriverError {
    TmuxDriverError::Spawn {
        command: command_name.to_string(),
        reason,
    }
}

pub(super) fn format_output(output: &Output) -> String {
    format!(
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}
