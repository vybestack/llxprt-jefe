use crate::canvas::Canvas;
use crossterm::{
    cursor,
    event::{self, Event, EventStream},
    execute, queue, terminal,
};
use futures::{
    channel::mpsc,
    future::pending,
    stream::{self, BoxStream, Stream, StreamExt},
};
use std::{
    collections::VecDeque,
    io::{self, stdout, Write},
    mem,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll, Waker},
};

// Re-exports for basic types.
pub use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers, MouseEventKind};

/// An event fired when a key is pressed.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct KeyEvent {
    /// A code indicating the key that was pressed.
    pub code: KeyCode,

    /// The modifiers that were active when the key was pressed.
    pub modifiers: KeyModifiers,

    /// Whether the key was pressed or released.
    pub kind: KeyEventKind,
}

impl KeyEvent {
    /// Creates a new `KeyEvent`.
    pub fn new(kind: KeyEventKind, code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::empty(),
            kind,
        }
    }
}

/// An event fired when the mouse is moved, clicked, scrolled, etc. in fullscreen mode.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct FullscreenMouseEvent {
    /// The modifiers that were active when the event occurred.
    pub modifiers: KeyModifiers,

    /// The column that the event occurred on.
    pub column: u16,

    /// The row that the event occurred on.
    pub row: u16,

    /// The kind of mouse event.
    pub kind: MouseEventKind,
}

impl FullscreenMouseEvent {
    /// Creates a new `FullscreenMouseEvent`.
    pub fn new(kind: MouseEventKind, column: u16, row: u16) -> Self {
        Self {
            modifiers: KeyModifiers::empty(),
            column,
            row,
            kind,
        }
    }
}

/// An event fired by the terminal.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum TerminalEvent {
    /// A key event, fired when a key is pressed.
    Key(KeyEvent),
    /// A mouse event, fired when the mouse is moved, clicked, scrolled, etc. in fullscreen mode.
    FullscreenMouse(FullscreenMouseEvent),
    /// A string pasted into the terminal (bracketed paste mode).
    Paste(String),
    /// A resize event, fired when the terminal is resized.
    Resize(u16, u16),
}

struct TerminalEventsInner {
    pending: VecDeque<TerminalEvent>,
    waker: Option<Waker>,
}

/// A stream of terminal events.
pub struct TerminalEvents {
    inner: Arc<Mutex<TerminalEventsInner>>,
}

impl Stream for TerminalEvents {
    type Item = TerminalEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(event) = inner.pending.pop_front() {
            Poll::Ready(Some(event))
        } else {
            inner.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn serialize_frame(
    canvas: &Canvas,
    previous: Option<&Canvas>,
    fullscreen: bool,
) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    queue!(
        frame,
        terminal::BeginSynchronizedUpdate,
        terminal::DisableLineWrap
    )?;
    let previous = previous.filter(|previous| {
        previous.width() == canvas.width() && previous.height() == canvas.height()
    });
    if let Some(previous) = previous {
        canvas.write_ansi_changed_rows(previous, &mut frame)?;
    } else if fullscreen {
        queue!(frame, cursor::MoveTo(0, 0), terminal::Clear(terminal::ClearType::All))?;
        canvas.write_ansi_without_final_newline(&mut frame)?;
    } else {
        canvas.write_ansi(&mut frame)?;
    }
    queue!(
        frame,
        cursor::MoveTo(0, 0),
        terminal::EnableLineWrap,
        terminal::EndSynchronizedUpdate
    )?;
    Ok(frame)
}

fn write_frame(destination: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    destination.write_all(frame)?;
    destination.flush()
}

trait TerminalImpl: Write + Send {
    fn width(&self) -> Option<u16>;
    fn size(&self) -> Option<(u16, u16)>;
    fn is_fullscreen(&self) -> bool;
    fn is_raw_mode_enabled(&self) -> bool;
    fn clear_canvas(&mut self) -> io::Result<()>;
    fn write_canvas(
        &mut self,
        canvas: &Canvas,
        previous: Option<&Canvas>,
    ) -> io::Result<()>;
    fn event_stream(&mut self) -> io::Result<BoxStream<'static, TerminalEvent>>;
}

struct StdTerminal {
    dest: io::Stdout,
    fullscreen: bool,
    raw_mode_enabled: bool,
    enabled_keyboard_enhancement: bool,
    prev_canvas_height: u16,
}

impl Write for StdTerminal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.dest.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.dest.flush()
    }
}

impl TerminalImpl for StdTerminal {
    fn width(&self) -> Option<u16> {
        self.size().map(|(width, _)| width)
    }

    fn size(&self) -> Option<(u16, u16)> {
        terminal::size().ok()
    }

    fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    fn is_raw_mode_enabled(&self) -> bool {
        self.raw_mode_enabled
    }

    fn clear_canvas(&mut self) -> io::Result<()> {
        if self.prev_canvas_height == 0 {
            return Ok(());
        }
        let lines_to_rewind = self.prev_canvas_height - if self.fullscreen { 1 } else { 0 };
        queue!(
            self.dest,
            cursor::MoveToPreviousLine(lines_to_rewind as _),
            terminal::Clear(terminal::ClearType::FromCursorDown)
        )
    }

    fn write_canvas(
        &mut self,
        canvas: &Canvas,
        previous: Option<&Canvas>,
    ) -> io::Result<()> {
        self.prev_canvas_height = canvas.height() as _;
        if self.fullscreen {
            let frame = serialize_frame(canvas, previous, true)?;
            write_frame(&mut self.dest, &frame)
        } else {
            canvas.write_ansi(&mut self.dest)?;
            self.dest.flush()
        }
    }

    fn event_stream(&mut self) -> io::Result<BoxStream<'static, TerminalEvent>> {
        self.set_raw_mode_enabled(true)?;

        Ok(EventStream::new()
            .filter_map(|event| async move {
                match event {
                    Ok(Event::Key(event)) => Some(TerminalEvent::Key(KeyEvent {
                        code: event.code,
                        modifiers: event.modifiers,
                        kind: event.kind,
                    })),
                    Ok(Event::Mouse(event)) => {
                        Some(TerminalEvent::FullscreenMouse(FullscreenMouseEvent {
                            modifiers: event.modifiers,
                            column: event.column,
                            row: event.row,
                            kind: event.kind,
                        }))
                    }
                    Ok(Event::Paste(data)) => Some(TerminalEvent::Paste(data)),
                    Ok(Event::Resize(width, height)) => Some(TerminalEvent::Resize(width, height)),
                    _ => None,
                }
            })
            .boxed())
    }
}

impl StdTerminal {
    fn new(fullscreen: bool) -> io::Result<Self>
    where
        Self: Sized,
    {
        let mut dest = stdout();
        queue!(dest, cursor::Hide)?;
        if fullscreen {
            queue!(dest, terminal::EnterAlternateScreen)?;
        }
        Ok(Self {
            dest,
            fullscreen,
            raw_mode_enabled: false,
            enabled_keyboard_enhancement: false,
            prev_canvas_height: 0,
        })
    }

    fn set_raw_mode_enabled(&mut self, raw_mode_enabled: bool) -> io::Result<()> {
        if raw_mode_enabled != self.raw_mode_enabled {
            if raw_mode_enabled {
                if terminal::supports_keyboard_enhancement()? {
                    execute!(
                        self.dest,
                        event::PushKeyboardEnhancementFlags(
                            event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        )
                    )?;
                    self.enabled_keyboard_enhancement = true;
                }
                if self.fullscreen {
                    execute!(self.dest, event::EnableMouseCapture)?;
                }
                execute!(self.dest, event::EnableBracketedPaste)?;
                terminal::enable_raw_mode()?;
            } else {
                terminal::disable_raw_mode()?;
                execute!(self.dest, event::DisableBracketedPaste)?;
                if self.fullscreen {
                    execute!(self.dest, event::DisableMouseCapture)?;
                }
                if self.enabled_keyboard_enhancement {
                    execute!(self.dest, event::PopKeyboardEnhancementFlags)?;
                }
            }
            self.raw_mode_enabled = raw_mode_enabled;
        }
        Ok(())
    }
}

impl Drop for StdTerminal {
    fn drop(&mut self) {
        let _ = self.set_raw_mode_enabled(false);
        if self.fullscreen {
            let _ = queue!(self.dest, terminal::LeaveAlternateScreen);
        }
        let _ = execute!(self.dest, cursor::Show);
    }
}

pub(crate) struct MockTerminalOutputStream {
    inner: mpsc::UnboundedReceiver<Canvas>,
}

impl Stream for MockTerminalOutputStream {
    type Item = Canvas;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

/// Used to provide the configuration for a mock terminal which can be used for testing.
///
/// This can be passed to [`ElementExt::mock_terminal_render_loop`](crate::ElementExt::mock_terminal_render_loop) for testing your dynamic components.
#[non_exhaustive]
pub struct MockTerminalConfig {
    /// The events to be emitted by the mock terminal.
    pub events: BoxStream<'static, TerminalEvent>,
}

impl MockTerminalConfig {
    /// Creates a new `MockTerminalConfig` with the given event stream.
    pub fn with_events<T: Stream<Item = TerminalEvent> + Send + 'static>(events: T) -> Self {
        Self {
            events: events.boxed(),
        }
    }
}

impl Default for MockTerminalConfig {
    fn default() -> Self {
        Self {
            events: stream::pending().boxed(),
        }
    }
}

struct MockTerminal {
    config: MockTerminalConfig,
    output: mpsc::UnboundedSender<Canvas>,
}

impl MockTerminal {
    fn new(config: MockTerminalConfig) -> (Self, MockTerminalOutputStream) {
        let (output_tx, output_rx) = mpsc::unbounded();
        let output = MockTerminalOutputStream { inner: output_rx };
        (
            Self {
                config,
                output: output_tx,
            },
            output,
        )
    }
}

impl Write for MockTerminal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl TerminalImpl for MockTerminal {
    fn width(&self) -> Option<u16> {
        None
    }

    fn size(&self) -> Option<(u16, u16)> {
        None
    }

    fn is_fullscreen(&self) -> bool {
        false
    }

    fn is_raw_mode_enabled(&self) -> bool {
        false
    }

    fn clear_canvas(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_canvas(
        &mut self,
        canvas: &Canvas,
        _previous: Option<&Canvas>,
    ) -> io::Result<()> {
        let _ = self.output.unbounded_send(canvas.clone());
        Ok(())
    }

    fn event_stream(&mut self) -> io::Result<BoxStream<'static, TerminalEvent>> {
        let mut events = stream::pending().boxed();
        mem::swap(&mut events, &mut self.config.events);
        Ok(events.chain(stream::pending()).boxed())
    }
}

pub(crate) struct Terminal {
    inner: Box<dyn TerminalImpl>,
    event_stream: Option<BoxStream<'static, TerminalEvent>>,
    subscribers: Vec<Weak<Mutex<TerminalEventsInner>>>,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        Ok(Self::new_with_impl(StdTerminal::new(false)?))
    }

    pub fn fullscreen() -> io::Result<Self> {
        Ok(Self::new_with_impl(StdTerminal::new(true)?))
    }

    pub fn mock(config: MockTerminalConfig) -> (Self, MockTerminalOutputStream) {
        let (term, output) = MockTerminal::new(config);
        (Self::new_with_impl(term), output)
    }

    fn new_with_impl<T: TerminalImpl + 'static>(inner: T) -> Self {
        Self {
            inner: Box::new(inner),
            event_stream: None,
            subscribers: Vec::new(),
        }
    }

    pub fn is_raw_mode_enabled(&self) -> bool {
        self.inner.is_raw_mode_enabled()
    }

    pub fn width(&self) -> Option<u16> {
        self.inner.width()
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        self.inner.size()
    }

    pub fn is_fullscreen(&self) -> bool {
        self.inner.is_fullscreen()
    }

    pub fn clear_canvas(&mut self) -> io::Result<()> {
        self.inner.clear_canvas()
    }

    pub fn write_canvas(
        &mut self,
        canvas: &Canvas,
        previous: Option<&Canvas>,
    ) -> io::Result<()> {
        self.inner.write_canvas(canvas, previous)
    }

    pub async fn wait(&mut self) {
        match &mut self.event_stream {
            Some(event_stream) => {
                while let Some(event) = event_stream.next().await {
                    // NOTE: jefe deliberately does NOT special-case Ctrl-C here.
                    // iocraft upstream hardcodes Ctrl-C as an exit signal, but jefe
                    // owns its own quit policy (Ctrl-Q / rapid qqq) and must forward
                    // Ctrl-C to the embedded agent terminal so runtimes like Code
                    // Puppy can use it to kill running shells / cancel an agent run
                    // (issue #200). Intercepting Ctrl-C here would (a) kill jefe
                    // instead of the agent's command and (b) prevent the event from
                    // ever reaching subscribers, so the app could never choose to
                    // forward it. Every event is delivered unconditionally below.
                    self.subscribers.retain(|subscriber| {
                        if let Some(subscriber) = subscriber.upgrade() {
                            let mut subscriber = subscriber.lock().unwrap();
                            subscriber.pending.push_back(event.clone());
                            if let Some(waker) = subscriber.waker.take() {
                                waker.wake();
                            }
                            true
                        } else {
                            false
                        }
                    });
                }
            }
            None => pending().await,
        }
    }

    pub fn events(&mut self) -> io::Result<TerminalEvents> {
        if self.event_stream.is_none() {
            self.event_stream = Some(self.inner.event_stream()?);
        }
        let inner = Arc::new(Mutex::new(TerminalEventsInner {
            pending: VecDeque::new(),
            waker: None,
        }));
        self.subscribers.push(Arc::downgrade(&inner));
        Ok(TerminalEvents { inner })
    }
}

impl Write for Terminal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{serialize_frame, write_frame};
    use crate::prelude::*;

    #[derive(Default)]
    struct CountingWriter {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
    }

    impl std::io::Write for CountingWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(buffer);
            self.writes += 1;
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn changed_frame_is_buffered_without_fullscreen_clear() {
        let mut previous = Canvas::new(8, 2);
        previous
            .subview_mut(0, 0, 8, 2, true)
            .set_text(0, 0, "stable", CanvasTextStyle::default());
        let mut current = previous.clone();
        current
            .subview_mut(0, 0, 8, 2, true)
            .set_text(0, 1, "changed", CanvasTextStyle::default());

        let frame = serialize_frame(&current, Some(&previous), true).unwrap();
        let output = String::from_utf8_lossy(&frame);

        assert!(output.contains("changed"));
        assert!(!output.contains("stable"));
        assert!(!output.contains("\u{1b}[2J"));
        assert!(output.contains("\u{1b}[?7l"));
        assert!(output.contains("\u{1b}[?7h"));
    }

    #[test]
    fn exact_width_initial_frame_disables_autowrap() {
        let mut canvas = Canvas::new(4, 1);
        canvas
            .subview_mut(0, 0, 4, 1, true)
            .set_text(0, 0, "|--|", CanvasTextStyle::default());

        let frame = serialize_frame(&canvas, None, true).unwrap();
        let output = String::from_utf8_lossy(&frame);

        assert!(output.contains("\u{1b}[?7l"));
        assert!(output.contains("|--|"));
        assert!(output.contains("\u{1b}[?7h"));
        assert!(!output.contains("\r\n"));
    }

    #[test]
    fn dimension_change_forces_complete_fullscreen_frame() {
        let mut previous = Canvas::new(3, 1);
        previous
            .subview_mut(0, 0, 3, 1, true)
            .set_text(0, 0, "old", CanvasTextStyle::default());
        let mut current = Canvas::new(4, 1);
        current
            .subview_mut(0, 0, 4, 1, true)
            .set_text(0, 0, "new!", CanvasTextStyle::default());

        let frame = serialize_frame(&current, Some(&previous), true).unwrap();
        let output = String::from_utf8_lossy(&frame);

        assert!(output.contains("\u{1b}[2J"));
        assert!(output.contains("new!"));
    }

    #[test]
    fn frame_is_published_with_one_write_and_one_flush() {
        let mut destination = CountingWriter::default();

        write_frame(&mut destination, b"complete frame").unwrap();

        assert_eq!(destination.bytes, b"complete frame");
        assert_eq!(destination.writes, 1);
        assert_eq!(destination.flushes, 1);
    }

    #[test]
    fn test_std_terminal() {
        // There's unfortunately not much here we can really test, but we'll do our best.
        // TODO: Is there a library we can use to emulate terminal input/output?
        let mut terminal = Terminal::new().unwrap();
        assert!(!terminal.is_raw_mode_enabled());
        assert!(!terminal.is_raw_mode_enabled());
        let canvas = Canvas::new(10, 1);
        terminal.write_canvas(&canvas, None).unwrap();
    }
}
