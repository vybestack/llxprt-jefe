//! Real-PTY session ownership for the schema-1 runner (issue #380).
//!
//! Owns the PTY launch (`portable-pty` starts the child in a new session and
//! process group via its own `setsid` path), input delivery, the merged
//! output byte stream, an `alacritty_terminal` grid for exact-size frames,
//! resize, and escalating process-group teardown. Group signaling uses the
//! fixed-path `/bin/kill` (or `/usr/bin/kill`) executable — not a shell, no
//! PATH lookup — because safe-std Rust cannot send signals and `unsafe` is
//! forbidden.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use portable_pty::{Child as PtyChild, CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::contract::Size;
use super::error::HarnessError;
use super::limits::MAX_BYTES;

/// Poll interval for bounded waits.
pub const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Each teardown escalation phase is bounded at two seconds.
const ESCALATION_PHASE: Duration = Duration::from_secs(2);

struct HarnessDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for HarnessDimensions {
    fn columns(&self) -> usize {
        self.cols
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn total_lines(&self) -> usize {
        self.rows
    }
}

/// Event sink that deliberately ignores terminal events: the harness must
/// not forward clipboard or other side effects to the host.
#[derive(Clone, Copy, Debug)]
struct HarnessListener;

impl EventListener for HarnessListener {
    fn send_event(&self, _event: TermEvent) {}
}

/// How the app-under-test exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessExit {
    pub exit_code: Option<u32>,
}

/// A live PTY session holding the app-under-test.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn PtyChild + Send + Sync>,
    term: Arc<Mutex<Term<HarnessListener>>>,
    stream: Arc<Mutex<Vec<u8>>>,
    generation: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    size: Size,
    stopped: bool,
    exit: Option<ProcessExit>,
    /// Group id captured at launch; still valid for signaling after reaping.
    group: Option<i32>,
}

impl PtySession {
    /// Launch `argv` in a fresh PTY with exactly `env`, working directory
    /// `cwd`, and the given terminal size. The child starts in a new session
    /// and process group.
    ///
    /// # Errors
    ///
    /// `HAR-E005` for PTY or spawn failures.
    pub fn launch(
        argv: &[String],
        env: &BTreeMap<String, String>,
        cwd: &Path,
        size: Size,
    ) -> Result<Self, HarnessError> {
        let (program, arguments) = argv
            .split_first()
            .ok_or_else(|| HarnessError::process("launch argv is empty".to_string()))?;
        let pair = native_pty_system()
            .openpty(pty_size(size))
            .map_err(|err| HarnessError::process(format!("openpty: {err}")))?;
        let mut command = CommandBuilder::new(program);
        command.args(arguments);
        command.env_clear();
        for (name, value) in env {
            command.env(name, value);
        }
        command.cwd(cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| HarnessError::process(format!("spawn '{program}': {err}")))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| HarnessError::process(format!("clone reader: {err}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| HarnessError::process(format!("take writer: {err}")))?;
        Ok(Self::assemble(pair.master, writer, child, reader, size))
    }

    fn assemble(
        master: Box<dyn MasterPty + Send>,
        writer: Box<dyn Write + Send>,
        child: Box<dyn PtyChild + Send + Sync>,
        reader: Box<dyn Read + Send>,
        size: Size,
    ) -> Self {
        let term = Arc::new(Mutex::new(Term::new(
            TermConfig::default(),
            &HarnessDimensions {
                cols: size.cols as usize,
                rows: size.rows as usize,
            },
            HarnessListener,
        )));
        let stream = Arc::new(Mutex::new(Vec::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(true));
        spawn_reader(
            reader,
            Arc::clone(&term),
            Arc::clone(&stream),
            Arc::clone(&generation),
            Arc::clone(&alive),
        );
        // portable-pty's Unix spawn calls setsid(), so the child is its own
        // session and process-group leader: its PID is the PGID.
        let group = child.process_id().and_then(|pid| i32::try_from(pid).ok());
        Self {
            master,
            writer,
            child,
            term,
            stream,
            generation,
            alive,
            size,
            stopped: false,
            exit: None,
            group,
        }
    }

    /// Write raw bytes to the application's input.
    ///
    /// # Errors
    ///
    /// `HAR-E005` on write failure.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), HarnessError> {
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|err| HarnessError::process(format!("pty write: {err}")))
    }

    /// Current rendered frame as trimmed text rows.
    ///
    /// # Errors
    ///
    /// `HAR-E005` when the terminal model lock is poisoned.
    pub fn frame_lines(&self) -> Result<Vec<String>, HarnessError> {
        let term = self
            .term
            .lock()
            .map_err(|_| HarnessError::process("terminal model lock poisoned".to_string()))?;
        Ok(render_lines(&term, self.size))
    }

    /// Current terminal size.
    #[must_use]
    pub const fn size(&self) -> Size {
        self.size
    }

    /// Lossy text view of the merged output stream (bounded at 1 MiB).
    ///
    /// # Errors
    ///
    /// `HAR-E005` when the stream lock is poisoned.
    pub fn stream_text(&self) -> Result<String, HarnessError> {
        let bytes = self
            .stream
            .lock()
            .map_err(|_| HarnessError::process("PTY stream lock poisoned".to_string()))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Output generation counter; advances on every PTY read.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Whether the PTY stream is still open.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Resize the PTY and terminal model to `size`.
    ///
    /// # Errors
    ///
    /// `HAR-E005` on PTY resize failure.
    pub fn resize(&mut self, size: Size) -> Result<(), HarnessError> {
        self.master
            .resize(pty_size(size))
            .map_err(|err| HarnessError::process(format!("pty resize: {err}")))?;
        let mut term = self
            .term
            .lock()
            .map_err(|_| HarnessError::process("terminal model lock poisoned".to_string()))?;
        term.resize(HarnessDimensions {
            cols: size.cols as usize,
            rows: size.rows as usize,
        });
        drop(term);
        self.size = size;
        Ok(())
    }

    /// The child's process group id captured at launch.
    #[must_use]
    pub const fn process_group(&self) -> Option<i32> {
        self.group
    }

    /// Reap the direct child if it has exited; the first observed status is
    /// cached because a child can only be waited once.
    pub fn try_exit(&mut self) -> Option<ProcessExit> {
        if self.exit.is_some() {
            return self.exit;
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.exit = Some(ProcessExit {
                exit_code: Some(status.exit_code()),
            });
        }
        self.exit
    }

    /// Terminate the whole process group with bounded escalation: graceful
    /// TERM, then KILL, then a final verification window, each bounded at
    /// two seconds. Reaps the direct child and verifies every group member
    /// is gone.
    ///
    /// # Errors
    ///
    /// `HAR-E007` when descendants survive the final window.
    pub fn stop(&mut self) -> Result<ProcessExit, HarnessError> {
        let result = self.stop_inner();
        if result.is_ok() {
            self.stopped = true;
        }
        result
    }

    fn stop_inner(&mut self) -> Result<ProcessExit, HarnessError> {
        let group = self.process_group();
        if let Some(pgid) = group {
            let _ = signal_group(pgid, "-TERM");
        }
        if let Some(exit) = self.await_group_exit(group)? {
            return Ok(exit);
        }
        if let Some(pgid) = group {
            let _ = signal_group(pgid, "-KILL");
        }
        if let Some(exit) = self.await_group_exit(group)? {
            return Ok(exit);
        }
        if let Some(exit) = self.await_group_exit(group)? {
            return Ok(exit);
        }
        Err(HarnessError::cleanup(
            "process group survived TERM/KILL escalation".to_string(),
        ))
    }

    /// One bounded escalation phase: reap the direct child and poll for the
    /// process group to be empty. Returns the exit when the group is gone.
    fn await_group_exit(
        &mut self,
        group: Option<i32>,
    ) -> Result<Option<ProcessExit>, HarnessError> {
        let deadline = Instant::now() + ESCALATION_PHASE;
        let mut exit = None;
        loop {
            if exit.is_none() {
                exit = self.try_exit();
            }
            let group_dead = match group {
                Some(pgid) => !group_alive(pgid)?,
                None => exit.is_some() || !self.is_alive(),
            };
            if exit.is_some() && group_dead {
                return Ok(exit);
            }
            if group_dead && !self.is_alive() {
                return Ok(Some(exit.unwrap_or(ProcessExit { exit_code: None })));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        // Last-resort orphan prevention on failure paths.
        if let Some(pgid) = self.process_group() {
            let _ = signal_group(pgid, "-KILL");
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    term: Arc<Mutex<Term<HarnessListener>>>,
    stream: Arc<Mutex<Vec<u8>>>,
    generation: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    alive.store(false, Ordering::Relaxed);
                    generation.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                Ok(count) => {
                    if let Ok(mut term) = term.lock() {
                        for byte in &buffer[..count] {
                            parser.advance(&mut *term, *byte);
                        }
                    }
                    append_bounded(&stream, &buffer[..count]);
                    generation.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    });
}

/// Append to the merged stream, retaining the first `MAX_BYTES` bytes.
fn append_bounded(stream: &Mutex<Vec<u8>>, bytes: &[u8]) {
    if let Ok(mut buffer) = stream.lock() {
        let room = MAX_BYTES.saturating_sub(buffer.len());
        buffer.extend_from_slice(&bytes[..bytes.len().min(room)]);
    }
}

fn render_lines(term: &Term<HarnessListener>, size: Size) -> Vec<String> {
    let rows = size.rows as usize;
    let cols = size.cols as usize;
    let mut grid = vec![vec![' '; cols]; rows];
    for indexed in term.renderable_content().display_iter {
        let row = usize::try_from(indexed.point.line.0).ok();
        let col = indexed.point.column.0;
        if let Some(row) = row
            && row < rows
            && col < cols
        {
            grid[row][col] = indexed.cell.c;
        }
    }
    grid.into_iter()
        .map(|row| row.into_iter().collect::<String>().trim_end().to_string())
        .collect()
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn kill_binary() -> Result<&'static str, HarnessError> {
    for candidate in ["/bin/kill", "/usr/bin/kill"] {
        if Path::new(candidate).exists() {
            return Ok(candidate);
        }
    }
    Err(HarnessError::process(
        "no fixed-path kill executable found".to_string(),
    ))
}

/// Send `signal` (e.g. `-TERM`) to every member of process group `pgid`.
fn signal_group(pgid: i32, signal: &str) -> Result<bool, HarnessError> {
    let status = std::process::Command::new(kill_binary()?)
        .args([signal, "--", &format!("-{pgid}")])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|err| HarnessError::process(format!("signal group: {err}")))?;
    Ok(status.success())
}

/// Whether any member of process group `pgid` still exists (signal 0 probe).
fn group_alive(pgid: i32) -> Result<bool, HarnessError> {
    signal_group(pgid, "-0")
}
