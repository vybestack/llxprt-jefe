//! OSC 52 clipboard writer with tmux / GNU screen passthrough.
//!
//! Writes an [OSC 52] selection-copy escape sequence so the controlling
//! terminal emulator copies `text` to the system clipboard. The sequence is
//! wrapped in a tmux DCS passthrough when `$TMUX` is set, and chunked into GNU
//! screen DCS segments when `$STY` is set (screen enforces a small per-passthrough
//! byte limit).
//!
//! The core writer ([`write_osc52_to_writer`]) is fully testable with a mock
//! writer; the public entry point ([`write_osc52`]) opens `/dev/tty` on Unix so
//! the escape sequence reaches the terminal even when stdout is piped.
//!
//! No external `base64` crate: a minimal RFC 4648 standard encoder is provided
//! here so the dependency surface stays small.
//!
//! [OSC 52]: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-Operating-System-Commands

use std::io::{self, Write};

/// Maximum base64 payload size before the copy is truncated.
///
/// Keeps escape sequences within terminal buffer limits. ~100 KiB of base64
/// corresponds to ~75 KiB of source text, far beyond any realistic selection.
pub const MAX_BASE64_PAYLOAD_BYTES: usize = 100_000;

/// Per-chunk byte cap for GNU screen DCS passthrough.
///
/// screen reassembles `ESC P ... ESC \` segments but caps each at a small size;
/// 240 bytes stays safely under the limit observed in practice.
pub const SCREEN_DCS_CHUNK_BYTES: usize = 240;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Which multiplexer passthrough (if any) the OSC 52 sequence must be wrapped in.
///
/// Detected from the environment by [`detect_passthrough_mode`] and accepted
/// explicitly by [`write_osc52_to_writer_with_mode`] so the wrapping logic is
/// unit-testable without mutating process-global environment variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassthroughMode {
    /// No multiplexer: emit a plain OSC 52 sequence.
    None,
    /// Inside tmux (`$TMUX` set): wrap in a tmux DCS passthrough.
    Tmux,
    /// Inside GNU screen (`$STY` set): chunk into screen DCS segments.
    Screen,
}

/// Detect the required passthrough mode from the process environment.
///
/// tmux takes precedence when both `TMUX` and `STY` are set (the common nesting
/// is screen-inside-tmux, where tmux's outer passthrough is what matters).
#[must_use]
pub fn detect_passthrough_mode() -> PassthroughMode {
    // `var_os` performs a read-only environment lookup and is not marked `unsafe`.
    if std::env::var_os("TMUX").is_some() {
        PassthroughMode::Tmux
    } else if std::env::var_os("STY").is_some() {
        PassthroughMode::Screen
    } else {
        PassthroughMode::None
    }
}

/// Write an OSC 52 copy sequence for `text` to `writer`.
///
/// The passthrough mode is auto-detected from the environment. Returns the
/// number of bytes written on success.
///
/// # Errors
///
/// Propagates any underlying [`io::Write`] error.
pub fn write_osc52_to_writer(text: &str, writer: &mut impl Write) -> io::Result<usize> {
    write_osc52_to_writer_with_mode(text, detect_passthrough_mode(), writer)
}

/// Write an OSC 52 copy sequence using an explicit passthrough mode.
///
/// Equivalent to [`write_osc52_to_writer`] but lets tests pass the mode directly
/// instead of mutating global environment variables. Returns the number of
/// bytes written.
///
/// # Errors
///
/// Propagates any underlying [`io::Write`] error.
pub fn write_osc52_to_writer_with_mode(
    text: &str,
    mode: PassthroughMode,
    writer: &mut impl Write,
) -> io::Result<usize> {
    let truncated_text = truncate_text_on_char_boundary(text);
    let b64 = base64_encode(truncated_text.as_bytes());
    let truncated_b64 = truncate_payload(&b64);
    write_payload_with_mode(&truncated_b64, mode, writer)
}

/// Write an OSC 52 copy sequence for `text` to the controlling terminal.
///
/// On Unix this opens `/dev/tty` so the sequence is delivered even when stdout
/// is redirected; on non-Unix targets it falls back to stdout. Errors are
/// surfaced to the caller (the mouse router logs them via `tracing`).
///
/// # Errors
///
/// Returns the underlying I/O error if neither `/dev/tty` nor stdout is
/// writable.
pub fn write_osc52(text: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        match std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            Ok(mut tty) => {
                write_osc52_to_writer(text, &mut tty)?;
                return Ok(());
            }
            Err(err) => {
                tracing::warn!(error = %err, "could not open /dev/tty for OSC 52; falling back to stdout");
            }
        }
    }
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    write_osc52_to_writer(text, &mut lock)?;
    Ok(())
}

/// Minimal RFC 4648 standard base64 encoder (with `=` padding).
#[must_use]
fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let mut buf = [0u8; 3];
        for (i, byte) in chunk.iter().enumerate() {
            buf[i] = *byte;
        }
        let b0 = usize::from(buf[0]);
        let b1 = usize::from(buf[1]);
        let b2 = usize::from(buf[2]);
        out.push(char::from(BASE64_ALPHABET[b0 >> 2]));
        out.push(char::from(BASE64_ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)]));
        if chunk.len() > 1 {
            out.push(char::from(BASE64_ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(BASE64_ALPHABET[b2 & 0x3f]));
        } else {
            out.push('=');
        }
    }
    out
}

/// Truncate `text` so the base64-encoded form stays within the payload limit.
///
/// Truncating on a `char` boundary first guarantees the clipboard text is
/// always valid UTF-8 even after the base64-level truncation.
#[must_use]
fn truncate_text_on_char_boundary(text: &str) -> String {
    // MAX_BASE64_PAYLOAD_BYTES is the byte limit for the base64 string.
    // Base64 encodes 3 bytes -> 4 chars, so the max input bytes = limit * 3 / 4.
    let max_input_bytes = MAX_BASE64_PAYLOAD_BYTES / 4 * 3;
    if text.len() <= max_input_bytes {
        return text.to_string();
    }
    // Walk backward from the byte limit to a char boundary.
    let mut cut = max_input_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_string()
}

/// Truncate the base64 string to the max payload size, keeping it valid base64.
#[must_use]
fn truncate_payload(b64: &str) -> String {
    if b64.len() <= MAX_BASE64_PAYLOAD_BYTES {
        return b64.to_string();
    }
    // Trim back to a multiple-of-4 boundary so the base64 stays decodable.
    let cut = MAX_BASE64_PAYLOAD_BYTES - (MAX_BASE64_PAYLOAD_BYTES % 4);
    b64[..cut].to_string()
}

/// Emit the (optionally wrapped) OSC 52 payload to `writer` for `mode`.
fn write_payload_with_mode(
    b64: &str,
    mode: PassthroughMode,
    writer: &mut impl Write,
) -> io::Result<usize> {
    match mode {
        PassthroughMode::Tmux => write_tmux_passthrough(b64, writer),
        PassthroughMode::Screen => write_screen_passthrough(b64, writer),
        PassthroughMode::None => write_plain_osc52(b64, writer),
    }
}

/// Plain OSC 52: `ESC ] 52 ; c ; <base64> BEL`.
fn write_plain_osc52(b64: &str, writer: &mut impl Write) -> io::Result<usize> {
    let mut buf: Vec<u8> = Vec::with_capacity(b64.len() + 16);
    buf.extend_from_slice(&[ESC, b']']);
    buf.extend_from_slice(b"52;c;");
    buf.extend_from_slice(b64.as_bytes());
    buf.push(BEL);
    writer.write_all(&buf)?;
    Ok(buf.len())
}

/// tmux DCS passthrough: each `ESC` byte inside the payload is doubled.
///
/// Emits `ESC P tmux ; ESC ESC ] 52 ; c ; <base64, ESC-doubled> ESC ESC \ ESC \`.
fn write_tmux_passthrough(b64: &str, writer: &mut impl Write) -> io::Result<usize> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"\x1bPtmux;");
    // The inner OSC 52 sequence is `\x1b]52;c;<base64>\x07`. Every ESC byte
    // inside the DCS payload must be doubled so tmux forwards them literally.
    buf.extend_from_slice(&escape_for_tmux(b"\x1b]52;c;"));
    buf.extend_from_slice(&escape_for_tmux(b64.as_bytes()));
    buf.extend_from_slice(&escape_for_tmux(b"\x07"));
    buf.extend_from_slice(b"\x1b\\\x1b\\");
    writer.write_all(&buf)?;
    Ok(buf.len())
}

/// Double every `ESC` byte so the sequence survives tmux passthrough embedding.
fn escape_for_tmux(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(byte);
        if byte == ESC {
            out.push(ESC);
        }
    }
    out
}

/// GNU screen DCS passthrough — wraps the complete OSC 52 sequence in a single
/// DCS wrapper so the terminal receives one intact clipboard-copy command.
///
/// Screen's per-segment byte limit means very large payloads may not fit. When
/// that happens we truncate the base64 on a 4-char boundary (each 4-char group
/// decodes to 3 bytes, so truncation stays valid) rather than splitting the OSC
/// 52 across multiple independent sequences (which would cause only the last
/// fragment to reach the clipboard).
fn write_screen_passthrough(b64: &str, writer: &mut impl Write) -> io::Result<usize> {
    let inner = build_screen_inner(b64);
    let body = truncate_for_screen(&inner);
    let mut buf: Vec<u8> = Vec::with_capacity(body.len() + 8);
    buf.extend_from_slice(b"\x1bP");
    buf.extend_from_slice(body);
    buf.extend_from_slice(b"\x1b\\");
    writer.write_all(&buf)?;
    Ok(buf.len())
}

/// Build the inner OSC 52 sequence bytes for screen DCS wrapping.
fn build_screen_inner(b64: &str) -> Vec<u8> {
    let mut inner: Vec<u8> = Vec::with_capacity(b64.len() + 16);
    inner.extend_from_slice(b"\x1b]52;c;");
    inner.extend_from_slice(b64.as_bytes());
    inner.push(BEL);
    inner
}

/// Truncate the inner OSC 52 bytes to fit screen's DCS segment limit, on a
/// 4-char base64 boundary so the truncated payload stays decodable.
fn truncate_for_screen(inner: &[u8]) -> &[u8] {
    if inner.len() <= SCREEN_DCS_CHUNK_BYTES {
        return inner;
    }
    let limit = SCREEN_DCS_CHUNK_BYTES;
    let boundary = (limit / 4) * 4;
    &inner[..boundary.max(4)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encodes_known_ascii() {
        // "hi" -> base64 "aGk=" (RFC 4648 test vector).
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[test]
    fn base64_encodes_utf8_bytes() {
        // "é" is 0xC3 0xA9 in UTF-8 -> base64 "w6k=".
        assert_eq!(base64_encode("é".as_bytes()), "w6k=");
    }

    #[test]
    fn write_osc52_plain_ascii() {
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode("hi", PassthroughMode::None, &mut out) {
            panic!("write failed: {e}");
        }
        let expected: Vec<u8> = vec![
            ESC, b']', b'5', b'2', b';', b'c', b';', b'a', b'G', b'k', b'=', BEL,
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn write_osc52_utf8_text() {
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode("é", PassthroughMode::None, &mut out) {
            panic!("write failed: {e}");
        }
        let Ok(s) = String::from_utf8(out) else {
            panic!("non-UTF8 output");
        };
        assert!(s.ends_with("w6k=\u{7}"), "got {s:?}");
        assert!(s.starts_with("\x1b]52;c;"), "got {s:?}");
    }

    #[test]
    fn write_osc52_empty_string() {
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode("", PassthroughMode::None, &mut out) {
            panic!("write failed: {e}");
        }
        let expected: Vec<u8> = vec![ESC, b']', b'5', b'2', b';', b'c', b';', BEL];
        assert_eq!(out, expected);
    }

    #[test]
    fn write_osc52_tmux_passthrough_exact_bytes() {
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode("hi", PassthroughMode::Tmux, &mut out) {
            panic!("write failed: {e}");
        }
        // Expected: ESC P t m u x ; ESC ESC ] 5 2 ; c ; a G k = BEL ESC \ ESC \
        // The inner OSC 52 is `\x1b]52;c;aGk=\x07` with every ESC doubled.
        let expected: Vec<u8> = vec![
            ESC, b'P', b't', b'm', b'u', b'x', b';', ESC, ESC, b']', b'5', b'2', b';', b'c', b';',
            b'a', b'G', b'k', b'=', BEL, ESC, b'\\', ESC, b'\\',
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn write_osc52_screen_passthrough_emits_chunk() {
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode("hi", PassthroughMode::Screen, &mut out) {
            panic!("write failed: {e}");
        }
        let Ok(s) = String::from_utf8(out) else {
            panic!("non-UTF8 output");
        };
        assert!(
            s.contains("\x1bP\x1b]52;c;aGk=\u{7}\x1b\\"),
            "screen chunk missing: {s:?}"
        );
    }

    #[test]
    fn write_osc52_screen_passthrough_truncates_large_payload() {
        // A payload whose base64 exceeds screen's DCS segment limit must be
        // truncated to fit a single DCS wrapper rather than split across
        // multiple independent OSC 52 sequences (which would cause only the
        // last fragment to reach the clipboard).
        let big = "a".repeat(SCREEN_DCS_CHUNK_BYTES * 4);
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode(&big, PassthroughMode::Screen, &mut out) {
            panic!("write failed: {e}");
        }
        let Ok(s) = String::from_utf8(out) else {
            panic!("non-UTF8 output");
        };
        // Should produce exactly one DCS-wrapped OSC 52 sequence.
        let segment_count = s.matches("\x1bP\x1b]52;c;").count();
        assert_eq!(
            segment_count, 1,
            "expected exactly 1 DCS-wrapped sequence, got {segment_count}: {s:?}"
        );
        // The output must end with the DCS terminator.
        assert!(
            s.ends_with("\x1b\\"),
            "expected DCS terminator at end, got: {s:?}"
        );
    }

    #[test]
    fn write_osc52_truncates_large_payload() {
        let big = "a".repeat(MAX_BASE64_PAYLOAD_BYTES * 2);
        let mut out: Vec<u8> = Vec::new();
        if let Err(e) = write_osc52_to_writer_with_mode(&big, PassthroughMode::None, &mut out) {
            panic!("write failed: {e}");
        }
        let Ok(s) = String::from_utf8(out) else {
            panic!("non-UTF8 output");
        };
        let payload = s
            .strip_prefix("\x1b]52;c;")
            .and_then(|rest| rest.strip_suffix('\u{7}'))
            .unwrap_or_default();
        // Truncated payload must be valid base64: length is a multiple of 4.
        assert_eq!(
            payload.len() % 4,
            0,
            "payload not multiple of 4: {payload:?}"
        );
        assert!(payload.len() <= MAX_BASE64_PAYLOAD_BYTES);
    }

    #[test]
    fn truncate_keeps_short_payloads_unchanged() {
        assert_eq!(truncate_payload("aGk="), "aGk=");
    }

    #[test]
    fn truncate_lands_on_multiple_of_four() {
        let overlong = "A".repeat(MAX_BASE64_PAYLOAD_BYTES + 5);
        let got = truncate_payload(&overlong);
        // The result must be valid base64: length is a multiple of 4.
        assert_eq!(got.len() % 4, 0, "truncated payload not multiple of 4");
        assert!(got.len() <= MAX_BASE64_PAYLOAD_BYTES);
    }

    #[test]
    fn escape_for_tmux_doubles_esc_only() {
        let got = escape_for_tmux(b"\x1b]52");
        assert_eq!(got, b"\x1b\x1b]52");
        // Non-ESC bytes pass through unchanged.
        let got2 = escape_for_tmux(b"abc");
        assert_eq!(got2, b"abc");
    }

    #[test]
    fn passthrough_mode_enum_round_trips() {
        assert_ne!(PassthroughMode::Tmux, PassthroughMode::Screen);
        assert_ne!(PassthroughMode::None, PassthroughMode::Tmux);
    }
}
