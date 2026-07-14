# Issue #260 plan: native Windows terminal attachment validation

## Goal

Validate and harden the production `portable-pty -> psmux attach-session -> ConPTY`
path while preserving the existing Unix and input-routing architecture.

## RED

1. Add pure runtime contracts for a local attach command specification and prove
   Windows uses the resolved psmux executable/namespace directly, never `sh`.
2. Add clipboard-boundary tests proving embedded OSC 52 stores are forwarded
   through Jefe's clipboard abstraction and provider failures remain recoverable.
3. Add paste-byte tests for exact multiline Unicode bracketed-paste framing.
4. Extend the Windows psmux smoke fixture and tests to attach through
   `portable-pty`, record exact input bytes, report initial/live geometry, emit
   ANSI/Unicode/cursor/alternate-screen/scrollback output, and prove closing the
   attach client leaves the persistent session alive.

## GREEN

1. Represent local attach invocation as an explicit platform-owned program/argv
   specification; convert it to `portable_pty::CommandBuilder` only at spawn.
2. Route `alacritty_terminal::ClipboardStore` events through `crate::clipboard`
   on every platform, logging provider errors without panicking.
3. Extract deterministic terminal paste framing from the app shell.
4. Add deterministic fixture protocol and diagnostic artifacts to the psmux
   smoke suite, with unique namespace teardown on every path.

## REFACTOR and review

- Keep remote SSH attachment behavior unchanged.
- Verify attach teardown kills only the client and never the persistent session.
- Check all runtime failures remain typed and reader teardown stays bounded.
- Audit terminal focus interception, key encoding, mouse/selection policy, and
  clipboard behavior against existing unit and harness scenarios.
- Run focused tests, strict Clippy, locked build/tests, then the full CI gates.
