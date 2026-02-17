//! Viewer attachment and PTY management.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @pseudocode component-002 lines 07-14

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::errors::RuntimeError;
use super::session::TerminalSnapshot;

/// Simple dimensions struct for terminal sizing.
struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
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

/// Null event listener for alacritty_terminal.
#[derive(Clone, Copy, Debug)]
pub struct NullListener;

impl EventListener for NullListener {
    fn send_event(&self, _event: alacritty_terminal::event::Event) {}
}

/// An attached viewer representing a PTY connected to a tmux session.
pub struct AttachedViewer {
    /// PTY master handle for resize.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Write end for sending input.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Terminal state model.
    term: Arc<Mutex<Term<NullListener>>>,
    /// Liveness flag.
    alive: Arc<AtomicBool>,
    /// Reader thread handle.
    _reader_thread: JoinHandle<()>,
    /// Current terminal dimensions.
    rows: u16,
    cols: u16,
}

impl AttachedViewer {
    /// Spawn a new attached viewer for a tmux session.
    ///
    /// @pseudocode component-002 lines 10-13
    pub fn spawn(session_name: &str, rows: u16, cols: u16) -> Result<Self, RuntimeError> {
        let pty_system = native_pty_system();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::SpawnFailed(format!("openpty: {e}")))?;

        let mut cmd = CommandBuilder::new("tmux");
        cmd.arg("attach-session");
        cmd.arg("-t");
        cmd.arg(session_name);
        cmd.env("TERM", "xterm-256color");

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| RuntimeError::SpawnFailed(format!("spawn tmux attach: {e}")))?;

        // We don't need to keep the child handle for kill - tmux session manages that
        drop(child);

        let reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| RuntimeError::SpawnFailed(format!("clone reader: {e}")))?;

        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| RuntimeError::SpawnFailed(format!("take writer: {e}")))?;

        let master = Arc::new(Mutex::new(pty_pair.master));
        let writer = Arc::new(Mutex::new(writer));

        // Create terminal model
        let config = TermConfig::default();
        let term_size = TermDimensions {
            cols: cols as usize,
            rows: rows as usize,
        };
        let term = Term::new(config, &term_size, NullListener);
        let term = Arc::new(Mutex::new(term));

        let alive = Arc::new(AtomicBool::new(true));

        // Spawn reader thread
        let term_clone = Arc::clone(&term);
        let alive_clone = Arc::clone(&alive);
        let reader_thread = thread::spawn(move || {
            reader_loop(reader, term_clone, alive_clone);
        });

        Ok(Self {
            master,
            writer,
            term,
            alive,
            _reader_thread: reader_thread,
            rows,
            cols,
        })
    }

    /// Check if the viewer is still alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Write input bytes to the PTY.
    ///
    /// @pseudocode component-002 lines 18-20
    #[allow(clippy::significant_drop_tightening)]
    pub fn write_input(&self, bytes: &[u8]) -> Result<(), RuntimeError> {
        if !self.is_alive() {
            return Err(RuntimeError::WriteFailed("viewer not alive".into()));
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| RuntimeError::WriteFailed("lock poisoned".into()))?;

        writer
            .write_all(bytes)
            .map_err(|e| RuntimeError::WriteFailed(e.to_string()))?;

        writer
            .flush()
            .map_err(|e| RuntimeError::WriteFailed(format!("flush: {e}")))?;

        Ok(())
    }

    /// Resize the terminal.
    #[allow(clippy::significant_drop_tightening)]
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), RuntimeError> {
        let master = self
            .master
            .lock()
            .map_err(|_| RuntimeError::ResizeFailed("lock poisoned".into()))?;

        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| RuntimeError::ResizeFailed(e.to_string()))?;

        drop(master);

        // Also update the terminal model
        if let Ok(mut term) = self.term.lock() {
            let new_size = TermDimensions {
                cols: cols as usize,
                rows: rows as usize,
            };
            term.resize(new_size);
        }

        Ok(())
    }

    /// Get a snapshot of the terminal state.
    #[allow(
        clippy::significant_drop_tightening,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub fn snapshot(&self) -> Option<TerminalSnapshot> {
        let term = self.term.lock().ok()?;
        let grid = term.grid();

        let mut lines = Vec::with_capacity(self.rows as usize);
        let screen_lines = grid.screen_lines();
        for row_idx in 0..screen_lines {
            let row = &grid[alacritty_terminal::index::Line(row_idx as i32)];
            let mut line = Vec::with_capacity(self.cols as usize);
            for col_idx in 0..grid.columns() {
                let cell = &row[alacritty_terminal::index::Column(col_idx)];
                line.push(cell.c);
            }
            lines.push(line);
        }

        let cursor = term.grid().cursor.point;

        Some(TerminalSnapshot {
            lines,
            cursor_row: cursor.line.0 as usize,
            cursor_col: cursor.column.0,
            rows: self.rows,
            cols: self.cols,
        })
    }

    /// Mark the viewer as dead.
    pub fn mark_dead(&self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

/// Reader loop that feeds PTY output into the terminal model.
fn reader_loop(
    mut reader: Box<dyn Read + Send>,
    term: Arc<Mutex<Term<NullListener>>>,
    alive: Arc<AtomicBool>,
) {
    let mut buf = [0u8; 4096];
    let mut parser: Processor<StdSyncHandler> = Processor::new();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - viewer died
                alive.store(false, Ordering::Relaxed);
                break;
            }
            Ok(n) => {
                if let Ok(mut term) = term.lock() {
                    for byte in &buf[..n] {
                        parser.advance(&mut *term, *byte);
                    }
                }
            }
            Err(_) => {
                // Reader error - mark viewer as dead
                alive.store(false, Ordering::Relaxed);
                break;
            }
        }
    }
}
