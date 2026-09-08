# Issue #719 Plan: macOS scenario suite red after #715 screen-runtime cutover

Branch: `issue719` from main @ 45bda3ff. Diagnosis comment:
https://github.com/vybestack/llxprt-jefe/issues/719#issuecomment-5584618042

## Root cause

PR #715 (f5826508) deleted the migrated Dashboard/search/help/confirm/provider
modals and replaced their presentation through the shared declared screen
runtime. Unit tests were updated; the macOS-declared tmux scenarios pinning the
old presentation were not. #715 validated five scenarios only. Result: a
deterministic failing set on every macOS CI run since ~2026-08-30. Linux is
green because the failing scenarios are macOS-declared.

Post-merge evidence at 45bda3ff (run 34221297907): same deterministic set minus
`workbench-runtime-macos` (intermittent CI timing; passes locally and on CI at
this head), plus intermittent `semantic-continuation-macos`.

## Failure classification

| Class | Scenarios | Disposition |
|---|---|---|
| A. Text drift (behavior works, literal stale) | settings-keys-normal, agent-shell-overlay, dashboard-list-windowing, issue704/atomic-success, issue704/restart-publication, package-panel-lifecycle, issue265-linux-keys | Update scenario literals to current presentation; full pass-through each |
| B1. Confirm dialog redesign, buttons deleted by #715/f5826508 (gates on product decision) | confirm-dialog-focus, issue-dirty-copy-confirm, provider-action-confirmation | AWAITING USER: restore #228/#233 focusable buttons (`ConfirmFocus`, Left/Right/Tab cycle, Enter activates focused, default Cancel) vs bless #715 `Decision:` layout |
| B2. Terminal Manager empty state, `No shells` row deleted by #720/394f59aa (#706 cutover 3, not #715) (gates on product decision) | kennel-terminal-select, terminal-manager, paste-enter-escape, code-puppy-chord-passthrough | AWAITING USER: restore `No shells` empty-state row vs update scenarios |
| C. Functional: small-viewport overlay truncation | issue382/agent-unsupported-ui | Fix renderer: action affordance rows must stay visible at 54x16 |
| D. Intermittent CI timing | issue705/workbench-runtime-macos, issue705/semantic-continuation-macos | Observe in PR CI; no code change this issue |

## Acceptance matrix (slice A)

| # | Actor/path | Input | Success behavior | Failure behavior | Test |
|---|---|---|---|---|---|
| A1 | Generated-agent overlay render | 54x16 viewport, Claude Code installed | `[Create enabled]` and `[Back]` rows visible alongside footer | Rows clipped below `Fields` with no affordance visible (current bug) | Local run of `issue382/agent-unsupported-ui.json` passes all 30 steps; focused unit test on overlay projection at small height |
| A2 | Scenario literals ×7 | Current main presentation | Each scenario passes end-to-end locally (all steps, exit 0) | First-failure literal updated only = insufficient; later steps must pass too | Local runner per scenario |

## Slice A scope (unblocked)

- Fix: overlay projection keeps action affordance rows (`[Create enabled]`,
  `[Back]`) visible at small viewports. Restore the #382-era accepted behavior;
  no modal redesign.
- Scenarios updated (literals only, step structure unchanged so the manifest
  step counts stay valid):
  - `dev-docs/tmux-scenarios/settings-keys-normal.json` (`Keys (339)` → current count)
  - `dev-docs/tmux-scenarios/agent-shell-overlay.json` (`Agent Shell` → current header)
  - `dev-docs/tmux-scenarios/dashboard-list-windowing.json` (`Repository 24` truncation)
  - `dev-docs/tmux-scenarios/issue704/atomic-success.json`, `issue704/restart-publication.json`,
    `package-panel-lifecycle.json` (`Navigate to vendor.panel.open` help line)
  - `dev-docs/tmux-scenarios/issue265-linux-keys.json` (`n/N new issue` footer)
- Allowed paths: `src/overlay_controls.rs`, `src/selection/overlay_content.rs`,
  their `*_tests.rs` companions, the scenario JSONs above,
  `dev-docs/testing/scenario-execution-manifest.json` ONLY if a step count
  changes (it must not).
- Non-goals: no dialog redesign (B1), no empty-state restoration (B2), no
  changes to the seven B scenarios, no manifest disposition changes, no CI
  workflow changes, no `.llxprt/`/`.github/`/dependency changes.

## Slice B scope (blocked on user decision)

Per B1/B2 decisions: either restore focusable buttons / empty-state row
(production change in the overlay runtime + no scenario change) or bless the
new presentation (scenario rewrites through the dialog interactions). Will be
appended here after the decision.

### Slice B evidence (from CI artifacts; exact first-failing step per scenario)

All seven fail on their first literal touching the deleted presentation; no
deeper break is proven. Restore decisions make the two `( Cancel )` scenarios
and the four `No shells` scenarios pass with zero JSON edits (the pinned
rows/buttons reappear); provider-action-confirmation needs exactly one edit
under any decision (drop the generic `Provider Action` title literal — see the
B1 addendum below). Bless decisions require rewriting steps in all 7 (plus
windows-local-repository-prep on the Windows gate — see the blast-radius
inventory below).

| Scenario | First failure | Old literal pinned |
|---|---|---|
| confirm-dialog-focus | step 5/6 assert-frame | `( Cancel )` |
| issue-dirty-copy-confirm | step 12/13 assert-frame | `( Cancel )` |
| provider-action-confirmation | step 4/5 assert-frame | `Provider Action` (old generic confirm title; new title is the declared `Confirm deployment?`) |
| kennel-terminal-select | step 5/6 wait | `No shells` (12s timeout) |
| terminal-manager | step 4/5 wait | `No shells` (12s timeout; app_exit 1) |
| paste-enter-escape | step 5/6 wait | `No shells` (12s timeout) |
| code-puppy-chord-passthrough | step 5/6 wait | `No shells` (12s timeout) |

### B1 blast-radius inventory (corpus pins BOTH layouts)

Currently-passing scenarios that pin the NEW layout (would need edits under a
restore decision):
- `issue705/semantic-continuation-linux.json` + `-macos.json`: assert
  `Decision: Cancel`, wait `Decision: Deploy alpha` — and prove the
  continuation path DOES render declared labels in the `Decision:` row.
- `issue727/edit-agent-presentation.json` + `edit-repository-presentation.json`:
  assert `submit: host.overlay-submit`.

Old-layout pins: confirm-dialog-focus (`( Cancel )`/`( Confirm )` ×4),
issue-dirty-copy-confirm (×3), and `windows-local-repository-prep.json`
(`( Cancel )`; linux-unsupported, native Windows gate only — invisible in the
macOS failure set, red-or-untested there today).

Decision cost matrix (scenario JSON files edited):
- BLESS: the 7 failing macOS scenarios + windows-local-repository-prep (8).
- RESTORE #233 buttons with declared labels: semantic-continuation ×2 +
  issue727 ×2 (currently passing, 4) + provider-action-confirmation (1
  literal). The three old-pinning confirm scenarios pass unedited.

### B1 decomposition

- B1a (decision-independent): the provider one-shot confirm path must surface
  the declared `confirm_label`/`destructive` instead of leaking
  `submit: host.overlay-submit` (the continuation path already does this —
  `Decision: Deploy alpha`). Required under either decision; without it,
  "bless" would enshrine the internal-id leak into the scenario corpus.
  Fix site: `src/selection/overlay_content.rs` ~L355-365 hardcodes the
  `Decision: Cancel` + `submit: host.overlay-submit` rows for confirm content
  (the id comes from `domain/action_registry.rs` L37 `OverlaySubmit`); the
  provider confirm projection must consume the payload's `confirm_label`/
  `destructive`. Companion pins: `overlay_controls.rs` L699 keys focus logic
  on the exact `Decision: Cancel` row text; `overlay_content.rs` tests
  L395/L413/L434 and `overlay_controls_tests.rs` L212 assert the current
  rows; form overlays already forbid submit rows
  (`overlay_controls_{agent,repository}_form_tests.rs`).
- B1b (the actual decision): #233 focusable buttons vs the `Decision:` row for
  core confirm dialogs. Gates confirm-dialog-focus, issue-dirty-copy-confirm,
  windows-local-repository-prep; blast radius per the cost matrix above.

### B1 evidence addendum: provider confirm modal drops declared contract data

The `provider-action-confirmation` fixture declares `confirm_label: "Deploy now"`
and `destructive: true` in its host-confirmation payload. The current #715
confirm modal renders NEITHER: the frame shows only `Confirm deployment?` /
`This action changes production.` / `Decision: Cancel` /
`submit: host.overlay-submit` / `Tab Select Enter Activate Esc Cancel`.

Consequences:
- The provider-declared confirm label is discarded (UI shows a generic submit).
- The destructive flag has no visible distinction.
- The internal action id (`host.overlay-submit`) leaks into user-facing text.
- Scenario literal correction needed REGARDLESS of the B1 decision: the
  generic `Provider Action` title assertion must be dropped (the declared
  title `Confirm deployment?` already asserts the same intent), so
  provider-action-confirmation is NOT zero-edit under restore — it is
  one-literal-edit under restore.
- Sibling `provider-action-recovery` passes with the current presentation
  (8/8), confirming only the confirmation surface regressed.

Refined B1 recommendation: restore #233-style focusable buttons using the
DECLARED labels — `( Cancel )` / `( Deploy now )`, focus defaults to Cancel,
Left/Right/Tab cycle, Enter activates focused, destructive styling from the
declared flag. This fixes the label/destructive regressions, removes the
internal-id leak, and matches the two `( Cancel )` scenarios unedited.

Restore points: B1 = #233 contract (`ConfirmFocus` on every confirm modal
variant, default Cancel, Left/Right/Tab/BackTab cycle, Enter activates
focused), deleted by #715/f5826508; declared labels from the host-confirmation
payload (`confirm_label`, `destructive`). B2 = `No shells.` empty-state row
(pre-#720: `src/ui/screens/terminal_manager.rs:136` returned it as the
zero-sessions placeholder; #720 deleted the screen). Current projection: the
Terminal Manager arm of `project_host_panel` (`src/host_panel_models.rs`
~L560-608) builds `ListItem`s from session rows and emits an empty `ListBody`
when there are no shells — restore = emit a `No shells.` placeholder item when
`rows.is_empty()`.

B2 cost note: no passing scenario pins the blank pane, so restore = 0 scenario
edits (all 4 pass as-is) while bless = rewriting the 4 `No shells` waits with
no stable literal to wait on (a blank pane offers no deterministic
"manager is up, zero shells" target — the empty-state row is itself the
testability affordance).

## Scope ledger

### Decisions RESOLVED (user directive 2026-09-08: "do it all together, no follow-ups, fix it the best way")

- B1b: RESTORE #233 focusable confirm buttons WITH declared labels
  (`( Cancel )` / `( Deploy now )`, ConfirmFocus default Cancel,
  Left/Right/Tab/BackTab cycle, Enter activates, destructive styling).
- B1a: folded into the restore (the provider path consumes
  `confirm_label`/`destructive`; the `submit: host.overlay-submit` leak dies
  with the Decision row).
- B2: RESTORE the `No shells.` empty-state row.
- C: FOLD the escape-semantics scenario step fixes + execution-manifest
  regeneration into this issue (user approval for the manifest edit given).
- Blast-radius consequence: update the 4 new-layout-pinning scenarios
  (semantic-continuation ×2, issue727 ×2) to the restored presentation;
  the 3 old-button scenarios must pass UNEDITED (restore-fidelity proof).

### Slice A status (partial, committed)

- `dbc87332` — small-viewport overlay fix (RED→GREEN test, 30/30 ops green on
  agent-unsupported-ui).
- `62349ea8` — literal-only updates, verified end-to-end:
  settings-keys-normal 13/13 (`Keys (339)`→`Keys (337)`),
  dashboard-list-windowing 9/9 (`Repository 24`→`Name: Agent 24`,
  `Repository 7`→`25 repos | 0/25 running`),
  agent-shell-overlay 50/50 (6× `Agent Shell`→`Terminal (F12 hide shell)`).
  Manifest untouched (verified 0-line diff).

### Slice C: escape-semantics scenarios (STOPPED, approval-gated)

Four scenarios fail on BEHAVIOR, not literals — the #715/#720 cutovers changed
what Escape does after a screen/plugin opens:

- F2 trio (issue704/atomic-success, issue704/restart-publication,
  package-panel-lifecycle): F2 → package review renders, but the following
  escape now EXITS the plugin screen to the dashboard, so the next
  `wait Package Review` polls the wrong screen. The dismissible
  `Navigate to vendor.panel.open` affordance is gone. Fix = delete the escape
  immediately after each post-F2 wait (atomic-success step 5;
  package-panel-lifecycle step 4; restart-publication steps 5 and 16); use
  literal `Package Review` for the post-F2 waits. Evidence:
  tmp/issue719/a2fix-{atomic-success,package-panel-lifecycle}/.
- issue265-linux-keys: CORRECTION — the `n/N new issue` footer still renders
  (baseline frames 4–6); the earlier text-drift diagnosis was wrong. Real
  failure: step 12's second escape exits the issues screen to the dashboard,
  so step 13 polls the wrong screen. Fix = step 12 `key escape`→`key i`
  (footer advertises `i list`), steps 13–15 unchanged.

Both fixes CHANGE STEP COUNTS → require regenerating the
scenario-execution-manifest entry, a quality-gate artifact that is off-limits
without explicit approval. Stopped per slice bounds; awaiting user decision:
fold into #719 (approval to edit scenarios + manifest here) or split to a
follow-up issue.

- (empty; record any approved additions)

## Review counters

- OCR runs before PR: 0/2 used
- OCR runs after PR: 0/2 used

## Verification evidence

- Slice A: focused cargo tests + per-scenario local runs (command in
  tmp/issue719 logs) + `cargo fmt --all --check`, clippy, locked build/test
  before PR.
- PR CI: macOS shards 0-5 + completion gate green is the success criterion.
