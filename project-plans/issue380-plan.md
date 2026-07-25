# Issue 380 delivery plan — CW-00: Deterministic real-process TUI harness

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/380
- Branch: `issue380`
- Base: `origin/main` at `f805532`
- Review counters: OCR pre-PR 0/2, OCR post-PR 0/2
- Delivery shape: single PR on `issue380`; user pre-approved exceeding the
  hard scope budget (foundational subsystem; stacked PRs explicitly declined).
- Immediate follow-up: #397 (CW-00b) migrates all shipped scenarios to
  schema 1 and deletes the superseded harness. No dead code remains after it.

## Summary

Deliver a deterministic, synchronous, real-PTY scenario runner with a closed
schema-1 contract, contained workspaces, capture shims, redaction, and a new
`tmux_scenario` entry point with 0/2/4/124 exit codes. No production startup
changes, no shell execution, no ambient environment, no test-only production
hooks.

**Hard prohibition (user directive + issue text):** no legacy adapter, no
lowering pass, no compatibility shim, no dual-format detection — anywhere,
including in response to review feedback. Schema 1 is the only accepted
input; missing/wrong `schema` is `HAR-E001`. Preexisting scenarios stay on
the existing harness untouched until #397 migrates them forward and deletes
that harness.

## Acceptance matrix

Derived from the issue's test-first ledger (decision-complete in the issue).

| Row | Actor / path | Input & boundary | Observable success | Observable failure | Evidence |
|---|---|---|---|---|---|
| CW00-01 | schema-1 parser | valid all-ops doc; duplicate key; unknown field; non-decimal int | one deterministic operation plan | `HAR-E001`, exit 2, before launch | `harness-schema-all-ops.json` parser golden + duplicate/unknown table |
| CW00-02 | input gate | document without `schema:1` (incl. a pre-schema scenario document) | — | `HAR-E001`, exit 2, before any workspace/launch work | missing-`schema` rejection table |
| CW00-03 | capture shim | fixture invocation with stdin/args/env | exact argv/env/cwd/stdin/stdout/stderr/exit recorded per invocation | `HAR-E006` on expectation mismatch | `harness-capture.json` |
| CW00-04 | `${workspace}` interpolation | complete-prefix env/argv, `$$`, unknown `${name}`, embedded ref, path fields | prefix-only expansion; `$$` literal; paths never interpolate | `HAR-E003` | `harness-interpolation.json` |
| CW00-05 | resize | 100x30 then 70x18 | resize acknowledged only after exact-dimension frame | wait timeout, exit 124 | distinct 100x30 and 70x18 frames in report |
| CW00-06 | restart | durable files + running process group | files survive; old group terminated/reaped; new ordinal | `HAR-E005` | `harness-resize-restart.json` ordinals + durability |
| CW00-07 | containment | symlink swap, ancestor replacement mid-run | operation rejected before access | `HAR-E004`, no check-then-follow | `harness-containment.json` |
| CW00-08 | timeout escalation | hanging child + grandchild | escalate; every descendant reaped | exit 124, `HAR-E007` if cleanup fails | `harness-timeout.json` |
| CW00-09 | redaction | secrets in frames/streams/env/error paths/report/stderr | every occurrence `<redacted>`; `redaction_count` accurate | any leak fails test | `harness-redaction.json` scan |
| CW00-10 | limits | each bound at limit and limit+1 | at-limit validates | limit+1 → `HAR-E002` exit 2 before launch | `harness-limits.json` + matrix test |

Cross-cutting: empty-base environment (only scenario env + deterministic
`HOME`/`PATH`/`TMPDIR`/`JEFE_CONFIG_DIR`/`JEFE_STATE_DIR`/`JEFE_PLUGIN_DIR`/
`LANG=C.UTF-8`/`TERM=xterm-256color` rooted in workspace); mode-0700 unique
workspace; failure stops later steps, performs cleanup, retains workspace and
bounded report; canonical reports always schema 1.

## Non-goals

- No legacy adapter, lowering pass, compatibility shim, or dual-format
  detection (prohibited — see above). Reviews may not add these.
- No change to production startup, runtime, or UI (no product screens;
  mockups N/A per issue).
- No shell (`sh`) execution, command-string splitting, or host PATH lookup.
- No network helpers, no async runtime, no new dependencies.
- No changes to `.github/workflows`, quality gates, or thresholds.
- No edits to shipped `dev-docs/tmux-scenarios/*.json` pre-schema scenarios
  or the scripts/CI invoking the existing harness; #397 migrates them
  forward and deletes that harness immediately after this lands.

## Architectural decisions

1. **New code lives under `src/harness/v1/`** (contract, parser, validate,
   plan, workspace, env, redact, capture, runner, report). The issue's source
   table maps to these paths; table updated in
   `dev-docs/testing/tmux-harness.md` per the issue's "if a listed path has
   moved" clause. Single owner per responsibility. #397 collapses the module
   layout once the old harness is deleted.
2. **Terminal driving is a direct real PTY** owned by `src/harness/v1/pty.rs`
   via `portable-pty` (new session ⇒ new process group) with an
   `alacritty_terminal` grid for exact-size frames — the same idiom as
   production `src/runtime/attach.rs`. No tmux dependency for the v1 runner,
   so ledger tests run hermetically on any macOS/Linux runner. The issue's
   `tmux.rs` table row maps here; docs table updated per the issue's moved-
   path clause. Group signaling/escalation uses `/bin/kill` (or
   `/usr/bin/kill`) resolved from fixed paths — not a shell, no PATH lookup —
   because safe-std Rust cannot send signals and `unsafe` is forbidden.
3. **Capture shim** is one Rust binary (`jefe-capture-shim`) materialized at
   each capture's workspace path; it locates `<exe>.capture.json` beside
   itself via `current_exe()` (no env, no PATH), claims a start ordinal with
   `create_new`, and writes one JSON record per invocation. A
   `jefe-harness-probe` binary is the deterministic app-under-test for the
   ledger fixtures (prints size on resize, echoes keys, runs captures).
4. **Strict parsing** extends the project's existing hand-rolled serde
   visitor idiom: duplicate-key and unknown-field rejection, decimal-integer
   checks, byte/depth pre-scan for input bounds, then semantic validation.
   `schema:1` is required; anything else is `HAR-E001`.
5. **Closed sub-definitions** the issue references but does not define,
   fixed in `dev-docs/testing/tmux-harness.md`: `Id` (1..64 bytes,
   `[A-Za-z0-9._-]`, not `.`/`..`), `ByteString`/`BytePair` (JSON UTF-8
   strings / `{name,value}`), `FileExpectation`
   (`{path, exists?, content?:{utf8|base64}}`); in a real PTY stdout and
   stderr are one stream, so `wait` sources `stdout`/`stderr` both scan the
   merged PTY byte stream (grammar keeps both for future capture streams).

## Vertical slices

| Slice | Rows | Content | Layer/owner |
|---|---|---|---|
| S1 | CW00-01, 02, 04 (validator), 10 | v1 contract types, strict parser, schema gate, bounds, `HAR-E001..E007` + exit mapping, interpolation validator | pure model |
| S2 | CW00-07, 09, 04 (application) | workspace create/materialize, no-follow containment, deterministic env, redaction | boundary fs |
| S3 | CW00-03 | capture shim fixture bin + record/assert | boundary process |
| S4 | CW00-05, 06, 08 | v1 runner state machine on driver seam: launch/wait/key/text/frame asserts, exact-size resize ack, restart, finish, escalation reaping | boundary pty |
| S5 | all e2e | `src/bin/tmux_scenario.rs`, report emission, 8 ledger scenario fixtures, docs (`tmux-harness.md`, `RULES.md`) | CLI + docs |

## Expected files

- `src/harness/v1/{mod,contract,parse,validate,plan,interp,error,workspace,env,redact,capture,runner,report}.rs` + colocated `*_tests.rs`
- `src/bin/tmux_scenario.rs`, `src/bin/jefe-capture-shim.rs`
- `src/harness/mod.rs` (export v1), `Cargo.toml` (bin entries only)
- `dev-docs/tmux-scenarios/harness-{schema-all-ops,capture,interpolation,resize-restart,containment,timeout,redaction,limits}.json`
- `dev-docs/testing/tmux-harness.md`, `dev-docs/RULES.md`
- `tests/` additions for ledger rows needing integration-level proof

## Scope ledger

| Date | Discovery | Disposition |
|---|---|---|
| 2026-07-22 | Faithful implementation exceeds hard PR budget | Approved by user: single PR, budget exceedance accepted |
| 2026-07-22 | Issue as filed demanded a legacy adapter | User prohibited adapters/shims; issue body amended; forward migration + old-harness deletion split to #397 |

## Verification

Per green checkpoint: `make quick-check`; before push: `make ci-check`.
Scenario fixtures executed via the new binary in tests gated the same way
existing tmux-dependent tests are gated.

## Deferred / follow-ups

- #397 (CW-00b): migrate all shipped scenarios to schema 1; update scripts,
  Makefile, CI, and docs contracts; delete `src/bin/jefe-tmux-harness.rs`
  and the pre-schema harness modules. Immediately follows this PR.
