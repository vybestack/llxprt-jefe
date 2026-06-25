# Phase 09 — Message Bus & Key Routing Stub

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P09
- **Prerequisites:** `.completed/P08A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Add the key-routing and dispatch surface for PR Mode as compiling stubs: the `p`/`P` entry, the
PR-mode key handler skeleton (8-level precedence), the dispatch-layer routing arms, and the
async-loader dispatch fns (signatures only). Conversions were stubbed in P03 and implemented in P05;
this phase wires routing/dispatch.

**TOTAL-STUB rule (NO `todo!()`/`unimplemented!()` ANYWHERE — findings #1 & #4):** `Cargo.toml`
`[lints.clippy]` DENIES both macros (`todo = "deny"` L63, `unimplemented = "deny"` L64), and clippy
fires on their mere PRESENCE regardless of reachability. Since this stub phase requires
`cargo clippy --workspace --all-targets --all-features -- -D warnings` to PASS, `todo!()`/
`unimplemented!()` are FORBIDDEN in EVERY `src/app_input/prs*.rs` body. Concretely:
- Key resolvers (`handle_prs_mode_key` and its sub-handlers) return `Option<AppEvent>`
  (`Some`/`None`) safe values — NEVER `todo!()`. The outer wrapper `handle_dashboard_prs_key`
  returns `KeyHandling` (`Handled(...)`). They are reachable the moment a key is pressed in PR
  mode, so they must be panic-free anyway.
- The `dispatch_app_message` `PullRequests(...)` arms route to dispatch fns whose stub bodies are
  TOTAL NO-OPS (e.g. set a loading flag and return, or simply return) — NOT `todo!()`. Loader fns
  with no safe partial behavior yet return without spawning (no I/O) rather than `todo!()`.
- The hard gate (below) scans ALL `src/app_input/prs*.rs` files; no helper — reachable or not — may
  contain `todo!()`/`unimplemented!()`. RED behavior is proven later by the P10 RED → P11 GREEN
  cycle via behavioral assertions, never a panic.

## Requirements Implemented (Expanded)

### REQ-PR-001 entry, REQ-PR-002 routing/suppression, REQ-PR-003 focus, REQ-PR-004 Esc, REQ-PR-010 comments, REQ-PR-011 send-to-agent, REQ-PR-012 `o` open-in-browser
- **Behavior contract:** GIVEN the reducer (P05) + client (P08), WHEN P09 lands, THEN routing/
  dispatch signatures exist and compile, `p` from Dashboard is wired to `EnterPrsMode`, and
  `DashboardPullRequests` delegates via `handle_dashboard_prs_key(snapshot, key) -> KeyHandling`
  (the `KeyHandling` wrapper) to `handle_prs_mode_key(state, key) -> Option<AppEvent>` (the resolver;
  stub returns `None`).
- **Why it matters:** Establishes the key→event→dispatch path for TDD to target.

## P09 Stub Scope — Normative (authoritative ruling)

This subsection is the BINDING contract for exactly how thick the P09 key-routing stub must be. It
resolves prior ambiguity. Mirror the already-SHIPPED Issues precedent (`resolve_issues_key_event` in
`src/app_input/issues.rs`, the Issues `09-key-routing-stub.md`) and obey strict stub→RED→GREEN TDD:
the P10 RED tests must be able to FAIL, so any real key→event mapping P10 asserts MUST NOT be
pre-implemented here. The ONE exception is the REQ-PR-012 `o` thread, which is fully wired this phase.

### S1. Precedence STRUCTURE (REQUIRED — must exist and compile)
`resolve_prs_key_event` (the resolver named `handle_prs_mode_key` in this plan) MUST exist with the
full 8-tier precedence structure, mirroring `resolve_issues_key_event` (c003 L10-48):
- **P1 inline** — `if inline_state != InlineState::None { return handle_pr_inline_key(...) }`
- **P2 chooser** — `if agent_chooser.is_some() { return handle_pr_agent_chooser_key(...) }`
- **P3 search** — `if search_input_focused { return handle_pr_search_input_key(...) }`
- **P4 filter** — `if filter_ui.controls_open { return handle_pr_filter_controls_key(...) }`
  (P1–P4 are EARLY RETURNS, before the global/focus/pane chain)
- **P5 global → P6 focus → P7 pane-cycle**, expressed as the same `.or_else` chain as Issues:
  `resolve_pr_global_key(...).or_else(|| <P6 focus-domain dispatch>).or_else(|| resolve_pr_pane_cycle_key(...))`
- **P8 suppression** — applied AFTER P7 (see S2).
The sub-handler signatures (`handle_pr_list_key`, `handle_pr_detail_key`, `handle_pr_repo_key`,
`handle_pr_inline_key`, `handle_pr_agent_chooser_key`, `handle_pr_search_input_key`,
`handle_pr_filter_controls_key`, `handle_esc_in_prs_mode`, each `-> Option<AppEvent>`) MUST exist so
the structure compiles.

### S2. P8 suppression tier (REQUIRED structure — NOT deferred)
An explicit suppression resolver MUST be PRESENT at stub (mirroring the Issues precedent's reserved
keys `s`/`Ctrl-d`/`Ctrl-k`/`l`). Provide a named resolver (e.g. `resolve_pr_suppressed_key`) that
matches the reserved keys `s`, `Ctrl-d`, `Ctrl-k`, `l` and returns `None` (consumed-no-op: consumed,
silently ignored, never leaks to the dashboard), wired AFTER P7 in `handle_prs_mode_key`. This is
REQUIRED STRUCTURE this phase, NOT deferred. (Note: in the shipped Issues code the same observable
no-op also arises from the chain's terminal `None`; for PR mode the plan REQUIRES the suppression
step be an explicit, named, citable resolver so the P8 tier is unambiguously present — c003 L43-48.)

### S3. REQ-PR-012 `o` open-in-browser (FULLY IMPLEMENTED this phase — the one wired thread)
The `o` path MUST be IMPLEMENTED at stub and MUST live in the P6 focus-domain handlers
`handle_pr_list_key` AND `handle_pr_detail_key` — NOT in the P5 global resolver:
- `handle_pr_list_key`: `'o' IF selected_pr present -> Some(PrOpenInBrowser)` else
  `Some(PrShowNotice{ kind: NoSelectionToOpen })` (c003 L68-69).
- `handle_pr_detail_key`: `'o' IF pr_detail present -> Some(PrOpenInBrowser)` else
  `Some(PrShowNotice{ kind: NoSelectionToOpen })` (c003 L88-89).
- `o` MUST NOT appear in `resolve_pr_global_key` (it is focus-domain, not global).
- The dispatch side is also wired: the `AppMessage::PullRequests(OpenInBrowser)` arm routes to
  `prs_dispatch::dispatch_pr_open_in_browser`, and `dispatch_pr_open_in_browser` /
  `pr_open_in_browser_info` exist as BENIGN NO-OP / safe-default bodies (never `todo!()`).

### S4. Esc delegation decision (RULING: PRESENT/structural, inert)
The `Esc` arm in P5 `resolve_pr_global_key` `delegates` to `handle_esc_in_prs_mode(state)` and this
delegation wiring IS PRESENT at stub (structural, inert), consistent with the precedence diagram
(c003 L27). It is inert because `handle_esc_in_prs_mode` is itself a `None`-returning stub this phase
(its real precedence-unwind, c003 L92-98, is filled in at P10 RED → P11 GREEN). This is the single,
explicit choice: Esc delegation present (not deferred). Because the delegate returns `None`, no Esc
behavior is asserted yet, so TDD is not violated. Therefore `resolve_pr_global_key` at stub matches
ONLY `Esc` (delegating to `handle_esc_in_prs_mode`) and returns `None` for every other key.

### S5. DEFERRED to P10 RED → P11 GREEN (ENUMERATED — a stub-phase verifier MUST NOT flag these)
The following real mappings MUST NOT be implemented in the P09 stub; the relevant `AppEvent`
variants already exist (from P05) so the code compiles, but the stub handlers return `None` (or, for
focus-domain handlers, the bare `_ => None` fall-through) for ALL of them. A verifier MUST treat
their ABSENCE at stub as EXPECTED, not a blocker:
- **P5 global (DEFERRED, return `None` at stub):** `p`|`P` → `RefocusPrList`; `a` → `ExitPrsMode`;
  help `?`|`h`|`F1` → open help; `/` → `PrFocusSearchInput`; `f` (when `pr_focus == PrList`) →
  `PrOpenFilterControls`. (Only `Esc` delegation is present per S4.)
- **P6 focus-domain (DEFERRED, return `None`/fall-through at stub):** `Up`/`Down`/`PageUp`/
  `PageDown`/`Home`/`End`/`Enter` navigation; `Left`/`Right` arrow pane-cycle; `j`/`k` detail
  subfocus; `r`/`c`/`e` read-only `PrShowNotice` branches; `S` open-agent-chooser. (The `o` path is
  the SOLE exception — implemented per S3.)
- **P7 pane-cycle (DEFERRED, return `None` at stub):** `Tab` → `PrCycleFocus`; `Shift+Tab` →
  `PrCycleFocusReverse`.
NOTE the asymmetry: S2 P8 suppression and S3 `o` ARE implemented this phase; everything in S5 is NOT.

## Implementation Tasks

### Files to modify
- `src/app_input/normal.rs`:
  - `resolve_mode_key` — add `Char('p'|'P') if screen == Dashboard => Handled(Some(EnterPrsMode))`
    (do NOT alter `i`/`s` arms). Markers + c003 L01-09.
  - add `handle_dashboard_prs_key(snapshot, key) -> KeyHandling` (mirror
    `handle_dashboard_issues_key`: if `screen == DashboardPullRequests`, quit-shortcut when
    `input_mode == PrsNormal` and key is q/Q else delegate `prs::handle_prs_mode_key`) + add
    `prs_quit_shortcut_active`.
  - wire `handle_dashboard_prs_key` into the `handle_normal_key_event` resolver chain so it runs
    BEFORE `resolve_mode_key` (place it immediately after the `handle_dashboard_issues_key` call at
    `normal.rs` L94-98, before the `resolve_mode_key` call at L111). This ordering is REQUIRED so
    that when `screen_mode == DashboardPullRequests`, `p`/`P` is intercepted by
    `handle_dashboard_prs_key` → `handle_prs_mode_key` → `Some(RefocusPrList)` and NEVER reaches
    `resolve_mode_key` (whose `p`/`P` arm only fires when `screen == Dashboard`, so it would
    otherwise re-enter PR mode). Mirrors exactly how the issues branch keeps `i` from re-firing
    while already in `DashboardIssues`. Markers + c003 L10-14.
- `src/app_input/mod.rs`:
  - register `mod prs; mod prs_dispatch; mod prs_list_dispatch; mod prs_filter; mod prs_mutation;`
  - `dispatch_app_message` — add `AppMessage::PullRequests(...)` arms (stub: route to the new
    dispatch fns or `apply_and_persist`) — c004 L97-118.
  - add stub fns: `dispatch_prs_navigation`, `refresh_prs_navigation`,
    `refresh_repo_scope_if_changed_prs`, `update_pr_detail_viewport_rows`,
    `dispatch_pr_agent_chooser_confirm`, `pr_send_info`, `write_pr_prompt`, `launch_pr_agent`.
    **CANONICAL SYMBOL-NAME RULING:** the navigation-refresh dispatch helper is named
    `refresh_prs_navigation` (the `_prs` suffix, matching the sibling helpers
    `dispatch_prs_navigation` and `refresh_repo_scope_if_changed_prs` and P11 L103). This is
    authoritative: do NOT use `refresh_pr_navigation` (singular) even though the pseudocode source
    `component-004.md` L154/157 reads `refresh_pr_navigation` — that pseudocode citation has drifted
    (finding #6) and the canonical plan name `refresh_prs_navigation` wins.
    All of these `src/app_input/mod.rs` dispatch/loader stub bodies are TOTAL NO-OPS / safe defaults
    (return without I/O), NEVER `todo!()`/`unimplemented!()` (findings #1 & #4 — clippy denies both),
    because every one is reachable from a dispatched PR message or from startup wiring.
  - add the `AppMessage::PullRequests(OpenInBrowser)` dispatch arm routing to
    `prs_dispatch::dispatch_pr_open_in_browser` (stub) — c004 L113-115.

### Files to create (stubs)
- `src/app_input/prs.rs` — `handle_prs_mode_key(state, key) -> Option<AppEvent>` skeleton returning
  `None` (mirrors `resolve_issues_key_event`, `src/app_input/issues.rs` L29 → `Option<AppEvent>`;
  the `KeyHandling` wrapping is done ONLY by `handle_dashboard_prs_key` in `normal.rs`, mirroring
  `handle_dashboard_issues_key` at `src/app_input/normal.rs` L156-180 which wraps
  `handle_issues_mode_key` — itself `Option<AppEvent>`, `issues.rs` L197 — as
  `KeyHandling::Handled(...)` at L176-178); sub-handlers `handle_pr_list_key`, `handle_pr_detail_key`,
  `handle_pr_repo_key`, `handle_pr_inline_key`, `handle_pr_agent_chooser_key`,
  `handle_pr_search_input_key`, `handle_pr_filter_controls_key`, `handle_esc_in_prs_mode` (each
  `-> Option<AppEvent>`) — c003 L10-128. `handle_pr_detail_key` and `handle_pr_list_key` must include
  the `o` open-in-browser signature path (`Some(PrOpenInBrowser)` when a PR is present, else
  `Some(PrShowNotice{ kind: NoSelectionToOpen })`) — c003 L68-69,88-89, REQ-PR-012.
  `handle_pr_detail_key` must also include the read-only no-op signature paths for `r`/`c`/`e`:
  instead of returning bare `None`, the implemented form (P11) returns `Some(PrShowNotice{ kind })`.
  Stub leaves bodies returning `None`; the variants must exist so it compiles. **Build this file to
  the binding "P09 Stub Scope — Normative" subsection above:** S1 precedence structure + S2 explicit
  P8 suppression resolver + S3 `o` implemented in list/detail (NOT global) + S4 Esc delegation
  present/inert; everything in S5 returns `None` at stub.
- `src/app_input/prs_dispatch.rs` — `load_pr_detail_for_selection`, `load_more_pr_comments`,
  `preview_pr_from_list`, `format_pr_prompt`, `dispatch_pr_open_in_browser`, `pr_open_in_browser_info`
  — c004 L138-175; c003 L176-187,190-228.
- `src/app_input/prs_list_dispatch.rs` — `dispatch_pr_list_reload`, `dispatch_pr_list_fetch`,
  `request_pr_list_reload` — c004 L127-137.
- `src/app_input/prs_filter.rs` — filter-controls key handling — c003 L134-146.
- `src/app_input/prs_mutation.rs` — `handle_pr_inline_submit` — c003 L109-119.

Markers on every item.

## Pseudocode Traceability
- component-003 lines 01-232; component-004 lines 97-175.

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a stub/GREEN phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
rg -n "EnterPrsMode|handle_prs_mode_key|AppMessage::PullRequests" src/app_input/
```
All gates above MUST pass. Stub bodies compile; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] Build green; routing/dispatch signatures present.
- [ ] `i`/`s` arms in `resolve_mode_key` unchanged.
- [ ] `dispatch_app_message` PR arms compile (exhaustive).
- [ ] 8-tier precedence STRUCTURE present in `handle_prs_mode_key` (S1): P1–P4 early returns, then
  P5 global `.or_else` P6 focus `.or_else` P7 pane-cycle, then P8 suppression (cite).
- [ ] P8 suppression tier PRESENT as an explicit named resolver matching `s`/`Ctrl-d`/`Ctrl-k`/`l`
  → `None`, wired after P7 (S2 — required structure, NOT deferred) (cite).
- [ ] `o` path implemented in `handle_pr_list_key` AND `handle_pr_detail_key` with both the
  `Some(PrOpenInBrowser)` (present) and `Some(PrShowNotice{ kind: NoSelectionToOpen })` (absent)
  branches (S3) (cite).
- [ ] `o` ABSENT from `resolve_pr_global_key` (S3) (cite).
- [ ] Canonical helper name `refresh_prs_navigation` used (NOT `refresh_pr_navigation`) (cite).
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] `p`/`P` entry only when `screen == Dashboard` (cite).
- [ ] `DashboardPullRequests` delegates to `handle_prs_mode_key` (cite).
- [ ] Esc delegation to `handle_esc_in_prs_mode` PRESENT (structural/inert) in `resolve_pr_global_key`
  (S4); `resolve_pr_global_key` matches ONLY `Esc` at stub (cite).
- [ ] **DEFERRED-OK (S5) — a verifier MUST NOT flag these as missing at stub:** P5 `p`/`P`/`a`/help
  `?`|`h`|`F1`/`/`/`f` mappings; P6 nav (`Up`/`Down`/`PageUp`/`PageDown`/`Home`/`End`/`Enter`),
  `Left`/`Right`, `j`/`k` subfocus, `r`/`c`/`e` read-only notices, `S` chooser; P7 `Tab`/`Shift+Tab`.
  All return `None` at stub (the `o` path is the SOLE implemented focus-domain mapping).
- [ ] Handlers return `Option<AppEvent>`; no direct `app_state.write()` in handlers.
- [ ] No `todo!()`/`unimplemented!()` in ANY `src/app_input/prs*.rs` file (findings #1 & #4 — clippy
  denies both macros): key resolvers return `Option<AppEvent>` (`Some`/`None`) safe values and the
  dispatched `PullRequests(...)` arms route to TOTAL NO-OP stub bodies, never `todo!()`. HARD gate
  (scans ALL `prs*.rs` files):
  ```bash
  if rg -n "todo!\(\)|unimplemented!\(\)" src/app_input/prs*.rs ; then
    echo "FAIL: todo!()/unimplemented!() present in src/app_input/prs*.rs"; exit 1
  fi
  ```

## Deferred Implementation Detection
```bash
# Stub phase: todo!()/unimplemented!() are FORBIDDEN in ALL src/app_input/prs*.rs (findings #1 & #4
# — clippy denies both macros) — HARD inverted gate (absence passes, presence fails):
if rg -n "todo!\(\)|unimplemented!\(\)" src/app_input/prs*.rs ; then
  echo "FAIL: todo!()/unimplemented!() present in src/app_input/prs*.rs (must be benign no-ops)"; exit 1
fi
# Record (do NOT fail on) other deferred markers in app_input PR files; these become hard gates in P11.
# Record-only: append `|| true` so a no-match (rg exit 1) cannot abort the phase under `set -e`.
rg -n "TODO|FIXME|HACK|placeholder|for now|will be implemented" src/app_input/prs*.rs || true
```

## Success Criteria
- Compiles green; entry + delegation wired; dispatch arms present.

## Failure Recovery
Restore the two modified tracked files and delete ONLY the five new files this phase created. Do NOT
use `git clean`.
```bash
git restore --staged --worktree -- src/app_input/normal.rs src/app_input/mod.rs
rm -f src/app_input/prs.rs src/app_input/prs_dispatch.rs src/app_input/prs_list_dispatch.rs \
      src/app_input/prs_filter.rs src/app_input/prs_mutation.rs
```

## Phase Completion Marker (`.completed/P09.md`)
Phase ID, timestamp, files changed, build result, confirmation that NO `todo!()`/`unimplemented!()`
appear in any `src/app_input/prs*.rs` file (findings #1 & #4 — clippy denies both), semantic summary.
