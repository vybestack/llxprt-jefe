//! Tests for the viewer attachment layer (extracted to keep `attach.rs`
//! under the source-file-size limit).
//!
//! Issue #179 coverage: default-color transparency in `snapshot_cell_style`.

use super::*;

/// Build a minimal terminal model for testing `process_pty_read`.
fn test_term() -> Arc<Mutex<Term<RuntimeListener>>> {
    let size = TermDimensions { cols: 80, rows: 24 };
    Arc::new(Mutex::new(Term::new(
        TermConfig::default(),
        &size,
        RuntimeListener,
    )))
}

/// Embedded OSC 52 events must use the project clipboard boundary.
#[test]
fn clipboard_store_uses_injected_project_boundary() {
    let mut observed = String::new();
    let result = forward_clipboard_store("Ω clipboard", |text| {
        observed.push_str(text);
        Ok(())
    });

    assert!(
        result.is_ok(),
        "clipboard boundary should succeed: {result:?}"
    );
    assert_eq!(observed, "Ω clipboard");
}

#[test]
fn clipboard_store_provider_failure_is_recoverable() {
    let result = forward_clipboard_store("copy", |_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "provider unavailable",
        ))
    });

    assert!(
        result.is_err(),
        "provider failure must be surfaced without panicking"
    );
}

#[test]
fn clipboard_store_ignores_empty_payload() {
    let mut called = false;
    let result = forward_clipboard_store("", |_| {
        called = true;
        Ok(())
    });

    assert!(result.is_ok());
    assert!(!called, "empty OSC 52 stores must not invoke the provider");
}

/// Processing a batch of PTY bytes must set the dirty flag — this is the
/// core wiring between the reader thread and the event-driven render loop.
#[test]
fn process_pty_read_marks_viewer_dirty() {
    let term = test_term();
    let dirty = Arc::new(AtomicBool::new(false));
    let mut parser: Processor<StdSyncHandler> = Processor::new();

    assert!(
        !dirty.load(Ordering::Relaxed),
        "dirty should be false before any data arrives"
    );

    process_pty_read(b"hello world", &mut parser, &term, &dirty);

    assert!(
        dirty.load(Ordering::Relaxed),
        "dirty must be set after PTY data arrives"
    );

    // take_dirty() pattern: swap clears and returns the previous value.
    assert!(
        dirty.swap(false, Ordering::Relaxed),
        "take_dirty must return true after data arrived"
    );
    assert!(
        !dirty.load(Ordering::Relaxed),
        "take_dirty must clear the flag"
    );

    // A second take_dirty() returns false (no new data since last clear).
    assert!(
        !dirty.swap(false, Ordering::Relaxed),
        "take_dirty must return false when no new data"
    );
}

/// Processing a PTY batch advances the terminal parser model (not just
/// the dirty flag), proving the wiring feeds real bytes into the `Term`.
#[test]
fn process_pty_read_advances_terminal_model() {
    let term = test_term();
    let dirty = Arc::new(AtomicBool::new(false));
    let mut parser: Processor<StdSyncHandler> = Processor::new();

    // A blank terminal has no content in the first cell.
    {
        let Ok(guard) = term.lock() else {
            panic!("term lock should succeed");
        };
        let snapshot = snapshot_from_term(&guard);
        assert_eq!(
            snapshot.cells[0][0].ch, ' ',
            "terminal should be blank before processing"
        );
    }

    process_pty_read(b"X", &mut parser, &term, &dirty);

    let Ok(guard) = term.lock() else {
        panic!("term lock should succeed");
    };
    let snapshot = snapshot_from_term(&guard);
    assert!(
        snapshot
            .cells
            .iter()
            .any(|row| row.iter().any(|c| c.ch == 'X')),
        "terminal model should contain processed data after read"
    );
}

// ── Issue #179: default-color transparency ────────────────────────────

use alacritty_terminal::index::{Column, Line, Point};

/// Build an `Indexed<&Cell>` at row 0, col 0 referencing the given cell.
fn indexed_cell(cell: &Cell) -> Indexed<&Cell> {
    Indexed {
        point: Point {
            line: Line(0),
            column: Column(0),
        },
        cell,
    }
}

/// Build a cell with explicit fg/bg (no field reassign, so clippy's
/// `field_reassign_with_default` stays happy). Flags default to empty.
fn styled_cell(fg: ansi::Color, bg: ansi::Color) -> Cell {
    Cell {
        c: ' ',
        fg,
        bg,
        flags: Flags::empty(),
        extra: None,
    }
}

/// Build a `RenderableCursor` that is hidden and far away (never matches).
fn hidden_cursor() -> RenderableCursor {
    RenderableCursor {
        shape: ansi::CursorShape::Hidden,
        point: Point {
            line: Line(99),
            column: Column(99),
        },
    }
}

/// A default cell (terminal-default fg+bg) must produce `Color::Reset`
/// for both channels so the host terminal's colors show through.
#[test]
fn default_cell_produces_reset_colors() {
    let cell = Cell::default();
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.fg,
        iocraft::Color::Reset,
        "default fg must be Reset (transparent)"
    );
    assert_eq!(
        style.bg,
        iocraft::Color::Reset,
        "default bg must be Reset (transparent)"
    );
}

/// A DIM default-colored cell keeps a transparent foreground (Reset) but must
/// carry `dim: true` so the renderer applies `Weight::Light` (ANSI SGR 2).
/// Without the separate flag, `to_terminal` would discard the dimmed RGB as
/// Reset and the dimming would be silently lost (issue #179 regression).
#[test]
fn dim_default_cell_keeps_transparent_fg_but_sets_dim_flag() {
    let cell = Cell {
        c: 'x',
        fg: ansi::Color::Named(ansi::NamedColor::Foreground),
        bg: ansi::Color::Named(ansi::NamedColor::Background),
        flags: Flags::DIM,
        extra: None,
    };
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.fg,
        iocraft::Color::Reset,
        "DIM default fg must stay Reset (transparent)"
    );
    assert!(
        style.dim,
        "DIM default cell must set dim=true so the renderer applies Weight::Light"
    );
    assert!(!style.bold, "DIM (not DIM_BOLD) must not set bold");
}

/// An explicitly-colored DIM cell bakes the dimming into the concrete RGB via
/// `dim_rgb`, so the `dim` flag stays false to avoid double-dimming in the
/// renderer (Weight::Light on top of an already-darkened color).
#[test]
fn dim_explicit_cell_bakes_dimming_into_color_not_flag() {
    let cell = Cell {
        c: 'x',
        fg: ansi::Color::Spec(ansi::Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        }),
        bg: ansi::Color::Named(ansi::NamedColor::Background),
        flags: Flags::DIM,
        extra: None,
    };
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.fg,
        iocraft::Color::Rgb {
            r: 0x7f,
            g: 0x7f,
            b: 0x7f
        },
        "explicit DIM fg must be dimmed RGB (halved)"
    );
    assert!(!style.dim, "explicit-colored DIM must not set the dim flag");
}

/// A DIM_BOLD plain default cell carries both flags: bold=true (BOLD bit is set
/// in DIM_BOLD) and dim=true (default-colored, so the dim must survive as a
/// renderer hint). The renderer resolves bold-over-dim precedence.
#[test]
fn dim_bold_default_cell_sets_both_bold_and_dim_flags() {
    let cell = Cell {
        c: 'x',
        fg: ansi::Color::Named(ansi::NamedColor::Foreground),
        bg: ansi::Color::Named(ansi::NamedColor::Background),
        flags: Flags::DIM_BOLD,
        extra: None,
    };
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.fg,
        iocraft::Color::Reset,
        "DIM_BOLD default fg must stay Reset (transparent)"
    );
    assert!(style.bold, "DIM_BOLD includes the BOLD bit -> bold=true");
    assert!(
        style.dim,
        "DIM_BOLD default-colored cell must set dim=true for the renderer"
    );
}

/// Non-regression matrix for the `bold` flag across the Alacritty bit layout:
/// only cells whose flags contain the BOLD bit render bold. DIM-only must NOT
/// be bold (the old `intersects(BOLD | DIM_BOLD)` wrongly matched it because
/// DIM and DIM_BOLD share the DIM bit).
#[test]
fn bold_flag_matrix_matches_only_bold_bit() {
    fn bold_for(flags: Flags) -> bool {
        let cell = Cell {
            c: 'x',
            fg: ansi::Color::Spec(ansi::Rgb { r: 1, g: 1, b: 1 }),
            bg: ansi::Color::Named(ansi::NamedColor::Background),
            flags,
            extra: None,
        };
        let indexed = indexed_cell(&cell);
        let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());
        style.bold
    }

    assert!(bold_for(Flags::BOLD), "BOLD -> bold");
    assert!(bold_for(Flags::DIM_BOLD), "DIM_BOLD -> bold");
    assert!(bold_for(Flags::BOLD_ITALIC), "BOLD_ITALIC -> bold");
    assert!(!bold_for(Flags::DIM), "DIM-only must NOT be bold");
    assert!(!bold_for(Flags::empty()), "no flags -> not bold");
}

/// A cell with an explicit `Spec(rgb)` bg must keep that concrete bg.
#[test]
fn explicit_spec_bg_is_preserved() {
    let cell = styled_cell(
        ansi::Color::Named(ansi::NamedColor::Foreground),
        ansi::Color::Spec(ansi::Rgb {
            r: 0xff,
            g: 0x00,
            b: 0x00,
        }),
    );
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.bg,
        iocraft::Color::Rgb {
            r: 0xff,
            g: 0x00,
            b: 0x00
        },
        "explicit Spec(rgb) bg must be preserved"
    );
}

/// A cell with an explicit `Spec(rgb)` fg must keep that concrete fg.
#[test]
fn explicit_spec_fg_is_preserved() {
    let cell = styled_cell(
        ansi::Color::Spec(ansi::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00,
        }),
        ansi::Color::Named(ansi::NamedColor::Background),
    );
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.fg,
        iocraft::Color::Rgb {
            r: 0x00,
            g: 0xff,
            b: 0x00
        },
        "explicit Spec(rgb) fg must be preserved"
    );
}

/// A cell with an `Indexed(u8)` bg (e.g. ANSI color 4 = blue) must keep
/// the resolved concrete bg, not collapse to Reset.
#[test]
fn explicit_indexed_bg_is_preserved() {
    let cell = styled_cell(
        ansi::Color::Named(ansi::NamedColor::Foreground),
        ansi::Color::Indexed(4),
    );
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_ne!(
        style.bg,
        iocraft::Color::Reset,
        "explicit Indexed bg must not be Reset"
    );
}

/// A cell with default-bg but an explicit non-default fg must have
/// Reset bg (not black) and the explicit fg preserved.
#[test]
fn mixed_default_bg_explicit_fg() {
    let cell = styled_cell(
        ansi::Color::Spec(ansi::Rgb {
            r: 0xaa,
            g: 0xbb,
            b: 0xcc,
        }),
        ansi::Color::Named(ansi::NamedColor::Background),
    );
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_eq!(
        style.bg,
        iocraft::Color::Reset,
        "default bg must be Reset even with explicit fg"
    );
    assert_eq!(
        style.fg,
        iocraft::Color::Rgb {
            r: 0xaa,
            g: 0xbb,
            b: 0xcc
        },
        "explicit fg must be preserved alongside default bg"
    );
}

/// `base_terminal_style()` (used for blank/unwritten cells) must use
/// `Color::Reset` for bg so blank regions are transparent.
#[test]
fn base_terminal_style_uses_reset_bg() {
    let style = base_terminal_style();
    assert_eq!(
        style.bg,
        iocraft::Color::Reset,
        "base style bg must be Reset for transparent blank cells"
    );
    assert_eq!(
        style.fg,
        iocraft::Color::Reset,
        "base style fg must be Reset for consistency"
    );
}

// ── Issue #179: transformed (inverse/cursor) cells keep concrete contrast ──

/// A default cell with the INVERSE flag must render with concrete (non-Reset)
/// fg and bg so the inversion is visible. The runtime layer applies ANSI
/// high-contrast fallbacks for transformed default cells; only plain default
/// cells resolve to `Color::Reset`.
#[test]
fn inverse_default_cell_keeps_concrete_contrast() {
    let mut cell = Cell::default();
    cell.flags.insert(Flags::INVERSE);
    let indexed = indexed_cell(&cell);
    let style = snapshot_cell_style(&indexed, None, hidden_cursor(), &Colors::default());

    assert_ne!(
        style.fg,
        iocraft::Color::Reset,
        "inverse default fg must be concrete (visible inversion)"
    );
    assert_ne!(
        style.bg,
        iocraft::Color::Reset,
        "inverse default bg must be concrete (visible inversion)"
    );
    // Inversion swaps fg/bg: concrete fg differs from concrete bg.
    assert_ne!(style.fg, style.bg, "inverse must swap fg and bg");
}

/// A default cell under the cursor must render with a concrete cursor color
/// (not transparent Reset) so the cursor block is visible.
#[test]
fn cursor_on_default_cell_keeps_concrete_colors() {
    let cell = Cell::default();
    let indexed = indexed_cell(&cell);
    let cursor = RenderableCursor {
        shape: ansi::CursorShape::Block,
        point: Point {
            line: Line(0),
            column: Column(0),
        },
    };
    let style = snapshot_cell_style(&indexed, None, cursor, &Colors::default());

    assert_ne!(
        style.fg,
        iocraft::Color::Reset,
        "cursor cell fg must be concrete (visible cursor)"
    );
    assert_ne!(
        style.bg,
        iocraft::Color::Reset,
        "cursor cell bg must be concrete (visible cursor)"
    );
}
