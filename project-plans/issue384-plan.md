# Issue #384 — CW-04: Sole internal screen descriptors and unified layout parity

Branch: `issue384` (from `origin/main` @ `57469961`)
Workflow: `dev-docs/workflow/ISSUE-DELIVERY.md`

---

## 1. Grounded survey of the current system

Facts established by reading the tree at `57469961` (not assumptions):

| Fact | Evidence |
|---|---|
| The UI renders through **iocraft 0.5** with **taffy 0.5** flexbox. Real on-screen geometry is computed by taffy from declarative `Box` props inside each `#[component]`. | `Cargo.toml`; `src/ui/screens/*.rs` use `element! { Box(flex_direction: …, flex_grow: …) }` |
| `src/layout.rs` (869 lines) does **not** own geometry; it holds hand-maintained *mirror* arithmetic (`compute_pty_layout`, `dashboard_middle_row_heights_inner`, `split_layout_for_render_size`, per-pane chrome constants) that re-derives what taffy already computed, for the benefit of mouse routing, selection, wrap, and PTY resize. | `src/layout.rs`; comment "mirror exactly what each `#[component]` renders" |
| Geometry is re-derived independently at **21** `crossterm::terminal::size()` call sites across screens, components, mouse routing, key routing, app-input, and `main.rs`. | `grep -rn "terminal::size()" src/` |
| `ScreenMode` has **7** variants: `Dashboard`, `Split`, `DashboardIssues`, `DashboardPullRequests`, `DashboardActions`, `DashboardErrors`, `DashboardTerminals`. | `src/state/types.rs:303-319` |
| `ScreenMode` is referenced **416** times across **93** files (39 non-test, 54 test). | `grep -rln "ScreenMode" src/ tests/` |
| A parallel geometry concept already exists in `src/selection/layout_descriptor.rs::ScreenLayout` + `src/selection/geometry.rs` (600 lines) which does its own `ScreenMode`-switched pane arithmetic. | those files |
| The action registry from CW-03 (#383) **is merged and available**: `src/domain/action_registry.rs` exposes `ActionRegistrySnapshot`, `Availability`, `Resolution`, `Chord`, `HandlerKey`. | `src/domain/action_registry.rs`, PR #548 |
| Consumers that must read resolved rectangles: `src/mouse_routing.rs` (944), `src/mouse_routing_detail.rs` (48), `src/selection/geometry.rs` (600), `src/selection/layout_descriptor.rs` (143), `src/detail_wrap_map.rs` (158), `src/ui/components/terminal_view.rs` (701). | `wc -l` |
| Renderers that must become thin: `dashboard.rs` (379), `split.rs` (146), `issues.rs` (451), `pull_requests.rs` (484), `screens/actions.rs` (251), `actions_view.rs` (170). | `wc -l` |
| Source-size gates: warn 750 lines, hard 1000 lines per file. | `xtask/src/source_size.rs:16-18` |
| Verification: `cargo xtask ci` (fmt, clippy-allow policy, source size, architecture, strict lint, complexity, coverage, locked all-feature build, full test suite). | `xtask/src/`, `.llxprt/LLXPRT.md` |

---

## 2. Decision-complete acceptance matrix

Ten EARS rows from the issue, each made decision-complete (actor/launch path, inputs and
boundaries, target, observable success, observable failure + diagnostic location, side effects
permitted before failure, persistence/compatibility, proving test).

### CW04-01 — Exactly the five parity descriptors are instantiated

- **Actor / launch path:** process start, `src/app_init.rs::init_app_state`, before first render.
- **Inputs / boundaries:** no override source exists (none is introduced); the compiled descriptor
  table is the only input.
- **Target:** local, all platforms (pure, I/O-free).
- **Success:** `builtin_screens()` yields exactly `core.dashboard`, `core.repositories`,
  `github.issues`, `github.pull-requests`, `github.actions`; all IDs unique; every descriptor
  passes validation; every panel appears exactly once in `panels` and exactly once in the layout
  tree; every focusable panel appears exactly once in `focus_order`; `initial_focus` is focusable.
- **Failure:** a malformed compiled descriptor returns a typed `DescriptorError` naming the screen,
  panel, and violated invariant; `init_app_state` hard-fails before publication (no partial screen
  registry is ever visible to a renderer). Diagnostic: returned error surfaced through the existing
  startup error path plus `tracing::error!`.
- **Side effects permitted before failure:** none. Validation is pure and runs before any terminal,
  PTY, persistence, or network activity.
- **Persistence/compatibility:** none — descriptors are compiled, not persisted.
- **Proof:** `shipped-screen-definition-parity.json` golden compared against the compiled table,
  plus per-invariant validation unit tests (one test per violation branch).

### CW04-02 — Each screen renders with legacy visual and action behavior

- **Actor / launch path:** normal render of each of the five screens.
- **Inputs / boundaries:** representative populated state, empty state, and the terminal sizes used
  by the existing render tests and tmux scenarios.
- **Target:** local; tmux harness on Unix, psmux on Windows (existing drivers).
- **Success:** rendered frames are byte-identical to the pre-change frames for the same state and
  size; action ordering/status/invocation in the Actions panel is unchanged.
- **Failure:** any divergence fails the corresponding golden/scenario with a frame diff.
- **Side effects:** none beyond existing render.
- **Persistence/compatibility:** unchanged.
- **Proof:** five normal-state goldens captured from `origin/main` before the change (RED baseline),
  re-asserted after; existing `dev-docs/tmux-scenarios/*.json` scenarios for dashboard, split,
  issues, PRs, actions must pass unmodified.

### CW04-03 — Focus follows descriptor order and repairs deterministically

- **Actor / launch path:** Tab / Shift-Tab through the resolved focus order on each screen.
- **Inputs / boundaries:** prior focus is (a) visible, (b) hidden by collapse, (c) hidden by the
  too-small fallback, (d) absent (first frame).
- **Target:** local, all platforms.
- **Success:** focus advances cyclically to the first **visible focusable** panel at or after the
  prior focus; if none exists, focus becomes `initial_focus`. Panel-local arrows/`j`/`k` are
  unchanged. `q`/`Esc` still invoke the existing Back action; `F12`/`t` terminal; `Ctrl-Q` exit —
  all resolved through the CW-03 action registry, not re-implemented here.
- **Failure:** focus landing on a hidden panel, or a non-cyclic advance, fails the focus property
  test.
- **Side effects:** none — focus repair is pure over the snapshot.
- **Persistence/compatibility:** persisted focus continues to round-trip through the existing
  `src/app_input/persist_focus.rs` bridge.
- **Proof:** focus property test across all five descriptors × all four prior-focus classes; five
  focused-state scenarios.

### CW04-04 — Every geometry consumer receives the same snapshot identity

- **Actor / launch path:** one render frame, and one `TerminalEvent::Resize`.
- **Inputs / boundaries:** a frame with mouse click, wheel, text selection, detail wrap, list
  scroll, and a live PTY all active.
- **Target:** local, all platforms.
- **Success:** renderer, mouse routing, selection projection, focus, scrolling, wrapping, and PTY
  resize all read one `ResolvedLayout` bearing the same `ScreenInstanceId`; a frame never mixes two
  identities; resize replaces the snapshot atomically.
- **Failure:** any consumer that derives geometry from `crossterm::terminal::size()` instead of the
  snapshot fails a static call-site assertion plus the integration identity test.
- **Side effects:** none.
- **Persistence/compatibility:** unchanged.
- **Proof:** integration test asserting one identity across render/mouse/wrap/selection/scroll/PTY
  in a single frame, plus a repository-scan test bounding `terminal::size()` call sites to the
  single choke point.

### CW04-05 — Fixed/weighted/min/max/remainder allocation executes the stated algorithm

- **Actor / launch path:** `resolve_layout(descriptor, outer_rect, panel_state)`.
- **Inputs / boundaries:** axis lengths 0..=200 exhaustively for the five descriptors; fixed sizes
  below `min` and above `max`; weights that do not divide evenly; children reaching `max` mid
  distribution; nesting to depth 8; 2 and 8 split children.
- **Target:** pure function, all platforms.
- **Success:** children are flattened in declaration order; one separator cell is subtracted per
  adjacent **visible** pair; fixed sizes clamp to `[min, max]`; weighted children receive minima
  first, then `floor(remaining * weight / sum_weight)`, then remainder one cell at a time in
  declaration order; a child reaching `max` is removed and distribution repeats; produced rectangles
  are contiguous and non-overlapping and exactly tile the axis; zero-width/height leaves are hidden;
  borders/titles live **inside** child rectangles; global chrome is stripped exactly once by the
  caller.
- **Failure:** arithmetic overflow returns a typed `LayoutError` — never a panic. All internal math
  is checked `u32`.
- **Side effects:** none (no I/O, no allocation of external resources).
- **Persistence/compatibility:** none.
- **Proof:** exhaustive small-axis sweep property test (tiling, non-overlap, determinism,
  order-stability) plus const golden tables for the documented worked examples.

### CW04-06 — Collapse order under insufficient optional space

- **Actor / launch path:** `resolve_layout` when visible minima exceed axis space.
- **Inputs / boundaries:** ties on `collapse_priority`; ties on depth; nested collapsibles;
  collapsible siblings at different depths.
- **Target:** pure function.
- **Success:** while visible minima exceed axis space, hide one collapsible child chosen by
  `(collapse_priority ascending, depth_first_index descending)`; repeat until it fits or no
  collapsible remains. PR screen collapses `pr-detail` then `pr-actions` per the parity table.
- **Failure:** wrong collapse choice fails the fixture.
- **Proof:** collapse-priority/depth fixture table over the five descriptors and synthetic
  depth-tie cases.

### CW04-07 — Too-small fallback preserves the first required panel and Back/exit

- **Actor / launch path:** `resolve_layout` when required minima still do not fit.
- **Inputs / boundaries:** every size from 1×1 through 80×24.
- **Target:** pure function.
- **Success:** the result contains only the **first required focusable panel in descriptor focus
  order**, occupying the entire rect, with `too_small = Some(TooSmall { needed, available })`. The
  Back/exit-critical controls remain reachable (they are global actions in the CW-03 registry and
  are unaffected by panel visibility). No PTY geometry of zero is ever emitted.
- **Failure:** an empty panel set, a non-required panel chosen, a missing `TooSmall`, or a zero PTY
  rect fails the sweep.
- **Proof:** 1×1..80×24 sweep across all five descriptors (1,920 cases per descriptor) asserting the
  above invariants.

### CW04-08 — A visible PTY panel always has a nonzero content rectangle

- **Actor / launch path:** `resolve_layout` for `core.repositories` (terminal panel) and any screen
  whose visible panel has a PTY panel type.
- **Inputs / boundaries:** the full 1×1..80×24 sweep plus degenerate 0×0.
- **Success:** every visible PTY-typed `ResolvedPanel` has `content.width >= 1 && content.height >= 1`
  (and the PTY resize path receives that rect). If a PTY panel cannot receive a nonzero rect it is
  **hidden** rather than sized to zero.
- **Failure:** any zero-area visible PTY content rect fails the terminal-leaf property test.
- **Side effects:** hidden PTY panels receive no resize call at all (no zero-resize is issued).
- **Proof:** terminal-leaf property test; the scattered `.max(1)` / `.max(2)` guards in
  `src/layout.rs` are removed in favour of this structural guarantee.

### CW04-09 — One-way persistence migration; `ScreenMode` has zero references outside it

- **Actor / launch path:** startup, reading persisted screen state.
- **Inputs / boundaries:** each legacy persisted value; an unknown/invalid value; a missing value.
- **Success:** `Dashboard -> core.dashboard`, `Split -> core.repositories`,
  `DashboardIssues -> github.issues`, `DashboardPullRequests -> github.pull-requests`,
  current Actions state `-> github.actions`. Each old variant maps exactly once. Runtime never uses
  an enum ordinal. After migration, `ScreenId` is the sole runtime authority.
- **Failure:** an invalid/unknown legacy value emits `tracing::warn!` naming the offending value and
  selects the compiled initial screen. Never panics, never loses the rest of the persisted state.
- **Side effects permitted before failure:** none beyond reading the persisted document.
- **Persistence/compatibility:** one-way. New writes carry stable IDs; old documents are readable.
- **Proof:** migration matrix test (one row per legacy value + invalid + missing) and a
  superseded-symbol absence assertion that `ScreenMode` appears in no file other than the migration
  module.
- **⚠ Ambiguity requiring a decision — see §5.1:** `ScreenMode` has **seven** variants, but the
  migration table and the parity table cover only **five** screens. `DashboardErrors` and
  `DashboardTerminals` have no specified stable ID and no parity row.

### CW04-10 — Unavailable / error / recovery / dirty states keep screen-specific parity

- **Actor / launch path:** each of the five screens in: GitHub-unavailable, load-error, layout
  recovery (invalid legacy ID warned + layout repaired), and the existing dirty overlay.
- **Success:** the unavailable and error bodies, the retry affordance, the Back hint, and the dirty
  overlay render exactly as before, now positioned from the resolved content rectangle.
  Descriptors create no draft, so `DIRTY` has no new state — the existing dirty overlay gets
  geometry-parity coverage only.
- **Failure:** a frame diff against the pre-change baseline.
- **Proof:** a five-screen state ledger (screen × state) of render goldens plus dirty-overlay
  geometry parity.

---

## 3. Explicit non-goals

1. No external screen-definition syntax, file format, or user-editable descriptor source.
2. No screen relationship graph, navigation stack, or history.
3. No screen/layout editor UI.
4. No new dependency, no `unsafe`, no production `unwrap`/`expect`, no lint/complexity/coverage
   threshold change, no suppression directive.
5. No change to the CW-03 action registry contract — it is **consumed** (action IDs, contexts,
   availability reasons, resolved chords), never re-implemented or extended here.
6. No change to keybindings, key dispatch, or PTY passthrough byte semantics.
7. No replacement of iocraft/taffy as the drawing engine — the descriptor resolver becomes the
   authoritative geometry source and the iocraft trees are driven from it; iocraft is not removed.
8. No new persistence format beyond the one-way legacy screen-value migration.
9. No overlay/modal/chooser geometry rework (`src/selection/overlay_content.rs`, `src/ui/modals/`,
   forms). Overlays keep their current positioning; only the dirty-overlay geometry parity test is
   added.
10. No performance/caching work beyond the one snapshot per size/state change required by CW04-04.

---

## 4. Bounded vertical slices

| # | Slice | Acceptance rows | Architecture owner | RED evidence | Expected paths |
|---|---|---|---|---|---|
| S1 | Descriptor vocabulary + validation + five compiled screens + migration adapter | CW04-01, CW04-09 (mapping half) | domain (I/O-free) | descriptor golden + per-invariant validation tests + migration matrix | `src/workbench/{mod,ids,descriptor,validate,screens,migration}.rs`, `src/workbench/*_tests.rs`, `dev-docs/…/shipped-screen-definition-parity.json`, `src/lib.rs` |
| S2 | Deterministic `resolve_layout` + `ResolvedLayout` snapshot + collapse + too-small + focus repair + nonzero PTY | CW04-05, CW04-06, CW04-07, CW04-08, CW04-03 (pure half) | domain (I/O-free), re-exported by `src/layout.rs` | axis sweeps, collapse fixtures, 1×1..80×24 sweep, terminal-leaf property, focus property | `src/workbench/resolve*.rs` + tests, `src/layout.rs` (re-export) |
| S3 | Single snapshot production at startup + per-frame/resize choke point, threaded into props | CW04-04 | app shell / orchestration | snapshot-identity integration test + `terminal::size()` call-site bound | `src/app_init.rs`, `src/app_shell.rs`, `src/ui/orchestration.rs`, `src/state/types.rs` |
| S4 | Migrate consumers one at a time to the snapshot: mouse routing, selection geometry, detail wrap, terminal view | CW04-04, CW04-08 | UI/consumer boundary | existing mouse/selection/wrap/PTY tests must stay green; identity assertions added | `src/mouse_routing*.rs`, `src/selection/{geometry,layout_descriptor}.rs`, `src/detail_wrap_map.rs`, `src/ui/components/terminal_view.rs` |
| S5 | Make the five renderers thin over `ResolvedLayout` | CW04-02, CW04-10 | UI screens | five normal goldens + five-screen state ledger + existing tmux scenarios | `src/ui/screens/{dashboard,split,issues,pull_requests,actions}.rs`, `src/actions_view.rs` |
| S6 | Delete duplicate geometry and `ScreenMode` outside the migration module | CW04-09 (deletion half) | cross-cutting | superseded-symbol absence assertion + shim-token scan | `src/state/types.rs` + ~92 files carrying `ScreenMode` references |
| S7 | Standards documentation | done criteria | docs | n/a | `dev-docs/standards/display-and-ui.md`, `dev-docs/standards/architecture.md` |

Ordering is strict: S1→S2 are pure and independently provable; S3 introduces the choke point;
S4/S5 migrate one consumer at a time preserving parity; S6 deletes only after parity holds.

---

## 5. Blocking questions (workflow §1 / §10 — must be resolved before implementation)

### 5.1 `ScreenMode` has seven variants; the issue specifies five screens

`DashboardErrors` (the errors panel screen, `src/ui/screens/errors.rs`) and `DashboardTerminals`
(Terminal Manager, `src/ui/screens/terminal_manager.rs`) exist as first-class screens today but
appear in neither the five-screen parity table nor the migration table.

CW04-09 requires `ScreenMode` deleted with **zero** references outside the migration module, which
is impossible while two live screens have no stable `ScreenId`. Options:

- **(a)** Add `core.errors` and `core.terminals` descriptors (7 descriptors), and treat CW04-01's
  "exactly the five parity descriptors" as covering the five *parity* screens within a 7-entry
  registry. Keeps `ScreenMode` deletable.
- **(b)** Keep exactly five descriptors and record "Errors and Terminal Manager remain on the legacy
  screen enum" as an explicit non-goal, deferring their conversion — CW04-09's deletion clause then
  cannot be met in this issue and must be re-scoped to "`ScreenMode` is reduced to the two
  unconverted screens plus the migration module".

### 5.2 Scope budget

Measured blast radius for full delivery of all ten rows:

| Component | Files | Rough net changed lines |
|---|---:|---:|
| S1 descriptor module + tests + golden | ~10 | ~1,600 |
| S2 resolver + tests | ~5 | ~1,400 |
| S3 choke point | ~5 | ~250 |
| S4 consumer migration | ~10 | ~900 |
| S5 five renderers thin | ~7 | ~700 |
| S6 `ScreenMode` deletion (416 refs / 93 files) | ~93 | ~500 |
| S7 docs | 2 | ~150 |
| **Total** | **~130** | **~5,500** |

That is **~3.3× the hard file budget (40)** and **~2.2× the hard line budget (2,500)**. Per
`dev-docs/workflow/ISSUE-DELIVERY.md` §6 and §10 this is a mandatory stop for explicit approval.

Note for context: the sibling epic issue #383 shipped as a single 100-file PR (#548), so this
epic's issues have previously been delivered over budget.

Options:

- **(A)** Approve a single over-budget PR for the whole issue (precedent: #548). Estimated ~130
  files / ~5,500 net lines.
- **(B)** Approve a stacked/sequential split into separate PRs, e.g. PR-1 = S1+S2+S7 (pure
  descriptor + resolver + docs, ~15 files / ~3,200 lines, still over the line budget), PR-2 =
  S3+S4 (choke point + consumers), PR-3 = S5+S6 (thin renderers + `ScreenMode` deletion). This
  conflicts with the standing preference for one PR per issue and no stacked PRs, so it needs an
  explicit exception.
- **(C)** Reduce accepted scope for this issue (e.g. defer S6's full `ScreenMode` deletion to a
  follow-up issue) and record the deferral, keeping the PR nearer budget.

---

## 6. Scope ledger

| Date | Change | Category | Approval |
|---|---|---|---|
| — | (no implementation started; awaiting §5 decisions) | — | — |

## 7. Review counters

| Review | Cap | Used |
|---|---:|---:|
| Local OCR (pre-PR) | 2 | 0 |
| PR OCR | 2 | 0 |
| Rust architecture review | — | 0 |

## 8. Verification evidence

| Gate | Head | Result |
|---|---|---|
| — | — | not yet run |

## 9. Deferred findings / follow-ups

_(none yet)_
