# CW-00: Deterministic real-process TUI harness

## Outcome and boundary

Deliver a deterministic, synchronous, real-PTY scenario runner. It may create contained files and fixture executables, launch Jefe, drive terminal input/resize/restart, capture child invocations, and assert frames/files/process results. It must not modify production startup, execute a shell, inherit ambient environment, or add a test-only production hook. This issue has no prerequisite.

Schema 1 is the only scenario format when this issue closes. Every existing `dev-docs/tmux-scenarios/*.json` file is converted to schema 1 in-repo within this issue, with per-scenario equivalence proven before the old format and its parsing code are deleted. No legacy input mode, dual parser, or format-detection branch ships.

## Source and symbol inventory

| Source | Existing symbol/responsibility | Required responsibility and parity |
|---|---|---|
| `src/harness/mod.rs` | harness public surface | export the closed schema-1 parser, validator, runner, and report; nothing else |
| `src/harness/scenario.rs` | scenario DTO/parser | parse schema 1 with duplicate-key and unknown-field rejection; missing `schema` is `HAR-E001` |
| `src/harness/runner.rs` | scenario execution | synchronously execute the operation state machine and always reap process groups |
| `src/harness/tmux.rs` | real terminal driving | own PTY/tmux launch, literal waits, key delivery, frame capture, and resize acknowledgement |
| `src/bin/tmux_scenario.rs::main` | harness entry point | accept schema-1 input only, print one deterministic report, return 0/2/4/124 |
| `dev-docs/tmux-scenarios/*.json` | old-format scenarios | every file rewritten as an equivalent schema-1 scenario in this issue; superseded parser paths deleted at feature-complete |
| `dev-docs/testing/tmux-harness.md` | operator contract | document this grammar, containment, exits, limits, capture assertions, and the completed one-time conversion |

If a listed path has moved, update the table and implementation in the same change; do not create a second owner.

## Closed contract

All JSON objects reject duplicate and unknown keys. Integers are decimal JSON integers.

```text
Scenario={schema:1,name:NonEmpty,platform:"macos"|"linux",terminal:Size,
 workspace:Workspace,steps:[Step;1..1024],secrets:[NonEmpty;0..64]}
Size={cols:1..500,rows:1..200}
Workspace={mode:448,dirs:[Dir;0..256],files:[File;0..256],env:[Env;0..256]}
Dir={path:RelativePath,mode:448|493}
File={path:RelativePath,content:{utf8:string}|{base64:string},mode:384|420|448|493}
Env={name:EnvName,value:string}
Step={op:"write",file:File}|{op:"mkdir",dir:Dir}|{op:"remove",path:RelativePath}|
 {op:"capture",name:Id,path:RelativePath,behavior:CaptureBehavior}|
 {op:"launch",argv:[string;0..64],env:[Env;0..256],cwd:RelativePath}|
 {op:"key",key:string,modifiers:["alt"|"control"|"shift";0..3]}|
 {op:"text",text:string}|{op:"resize",size:Size}|
 {op:"wait",source:"frame"|"stdout"|"stderr",literal:NonEmpty,timeout_ms:1..30000}|
 {op:"assert-frame",contains:[string],absent:[string]}|
 {op:"assert-capture",capture:CaptureExpectation}|{op:"assert-file",file:FileExpectation}|
 {op:"restart"}|{op:"finish"}
CaptureBehavior={stdout:string,stderr:string,exit_code:0..255,stdin_limit:0..1048576,
 hang:bool,spawn_child_hang:bool}
CaptureExpectation={name:Id,invocation:1..MAX,argv:[ByteString],env:[BytePair],cwd:ByteString,
 stdin?:ByteString,stdout?:ByteString,stderr?:ByteString,exit_code?:0..255,signal?:i32}
Report={schema:1,scenario,status:"passed"|"failed",workspace,steps:[StepResult],
 captures:[Capture],frames:[Frame],app_exit?:ProcessExit,redaction_count:u64}
```

`RelativePath` is UTF-8, 1–4096 bytes, `/` separated, and has no root/prefix, empty, `.`, `..`, NUL, or backslash component. Names are unique after exact normalization; env names match `[A-Z_][A-Z0-9_]{0,127}`. Before every open, mutation, capture, and launch, resolve existing ancestors using no-follow handles and verify their physical identity remains below the workspace. A changed identity is `HAR-E004`; there is no check-then-follow path.

The runner starts with an empty environment. It injects only scenario env plus deterministic `HOME`, `PATH`, `TMPDIR`, `JEFE_CONFIG_DIR`, `JEFE_STATE_DIR`, `JEFE_PLUGIN_DIR`, `LANG=C.UTF-8`, and `TERM=xterm-256color` rooted in the workspace. Capture records raw argv elements, sorted env byte pairs, cwd, stdin, separate stdout/stderr, exit/signal, start ordinal, and process-tree cleanup. `${workspace}` interpolation is allowed only as the complete prefix of env values and launch argv values; `$$` is a literal `$`; every other `${name}` is `HAR-E003`. Paths never interpolate.

One-time scenario conversion: within this issue, each old-format file's `initial_size`, `env`, `keys`, `wait_for`, `expect`, `resize`, and `restart` entries are rewritten in source order into schema-1 operations — old key names to canonical key ops, default 5,000 ms waits made explicit, inherited fixture PATH semantics expressed as explicit workspace PATH, and frame expectations as `assert-frame`. The conversion is a dev-time rewrite of the checked-in files (a throwaway script may assist but does not ship as a runtime input path). Every converted scenario must produce the same pass/fail result and terminal assertions as its pre-conversion run, proven by a recorded before/after result index; after that proof, the old format has zero readers. Reports always use schema 1.

Bounds are inclusive: input/report/file/captured-stream 1,048,576 bytes each; strings 262,144; depth 16; object members 256; arrays 1,024; frames 2,048; captures 256; processes per capture 32. Redaction replaces every nonempty secret byte sequence in frame cells, streams, env, paths printed in errors, reports, and stderr with `<redacted>` before persistence.

Diagnostics and exits: `HAR-E001` syntax/duplicate/unknown, `HAR-E002` limit, `HAR-E003` interpolation, `HAR-E004` containment/race, `HAR-E005` process/PTY, `HAR-E006` assertion, `HAR-E007` cleanup. Validation exits 2, I/O/process 4, timeout 124, success 0.

## End-to-end and recovery

Parse and fully validate; create a mode-0700 unique workspace; materialize fixtures; register capture shims; launch Jefe in a new process group and real PTY; execute steps sequentially; acknowledge resize only after a frame with exact dimensions; on restart terminate/reap the old group and relaunch in the same workspace; finish by graceful stop then escalation at 2 s/2 s/2 s; redact and write the report. Any failure stops later steps, performs the same cleanup, retains the workspace and bounded report, and permits a fresh run. Durable files survive restart; processes, PTY buffers, and frames do not.

Security: no `sh`, command-string split, host executable lookup outside explicit PATH, symlink following, ambient env, network helper, or production hook. Malformed base64, non-UTF-8 JSON, secret equal to empty, and limit plus one fail before launch.

## Product UI applicability

This infrastructure introduces no product screen. Normal, focused, unavailable, error, dirty, recovery, and small-terminal product mockups are individually not applicable because no Jefe UI is changed. Required harness evidence is a normal 100x30 frame and a distinct focused 70x18 frame after resize; later capabilities own their product-state mocks.

## Test-first ledger

| ID | Singular EARS requirement | First failing test/scenario | Exact evidence |
|---|---|---|---|
| CW00-01 | WHEN schema-1 input is valid, the runner shall produce one deterministic operation plan. | `harness-schema-all-ops.json` | parser golden plus duplicate/unknown table |
| CW00-02 | WHEN every converted scenario runs under schema 1, it shall reproduce its pre-conversion pass/fail result and terminal assertions, and the runner shall reject non-schema-1 input with `HAR-E001`. | every converted `dev-docs/tmux-scenarios/*.json` plus one old-format rejection fixture | before/after result index and rejection golden |
| CW00-03 | WHEN capture executes, the runner shall record separate exact process-boundary fields. | `harness-capture.json` | argv/env/cwd/stdin/stdout/stderr/exit fixture |
| CW00-04 | WHEN workspace interpolation occurs, the runner shall apply only the specified prefix and escaping rules. | `harness-interpolation.json` | valid, unknown, embedded, and `$$` cases |
| CW00-05 | WHEN the PTY resizes, the runner shall wait for an exact-size redraw. | `harness-resize-restart.json` | distinct 100x30 and 70x18 frames |
| CW00-06 | WHEN Jefe restarts, the runner shall retain files and replace/reap the process group. | `harness-resize-restart.json` | invocation ordinals and durability map |
| CW00-07 | IF physical containment changes, the runner shall reject the operation before access. | `harness-containment.json` | symlink swap and ancestor replacement fixtures |
| CW00-08 | IF a wait or process exceeds a bound, the runner shall escalate and reap every descendant. | `harness-timeout.json` | child/grandchild hang fixture |
| CW00-09 | WHEN any observation emits, the runner shall redact all declared secrets. | `harness-redaction.json` | scan frames, streams, env, error, report, stderr |
| CW00-10 | IF any resource is at limit plus one, the runner shall fail before app launch. | `harness-limits.json` | at-limit and plus-one matrix |

Implement RED in the listed scenarios/unit/property tests, GREEN with the smallest synchronous parser/runner, then REFACTOR: after the before/after result index proves equivalence, delete the old-format parsing code and any conversion helper so only the schema-1 path remains.

## Normative documentation and done

Update `dev-docs/testing/tmux-harness.md` and `dev-docs/RULES.md` with the grammar, capture/interpolation semantics, containment algorithm, exits, cleanup, redaction, the completed one-time conversion, and the rule that feature scenarios use bounded literal synchronization. Done requires every ledger row, every converted scenario green under schema 1, zero remaining old-format readers or scenario files (shim-token scan clean per the epic no-shim policy), and unchanged `make ci-check`; no unsafe, production unwrap/expect, lint allowance, threshold change, unapproved dependency, shell operation, orphan, or secret leak.