# Issue #386 — CW-06: Typed routes, local unwind, navigation, and dirty lifecycle

Branch: `issue386` (from `origin/main` @ `52f8b6ed`)

## 1. Ground truth established before shaping

Facts verified in the tree (not assumed):

- `ScreenMode` is **already deleted**. `crate::workbench::ids::ScreenId` (enum, stable
  namespaced strings) is the current screen vocabulary and `AppState.screen: ScreenId`
  is the current runtime authority. There is **no** navigation stack today.
- `RouteId`, `ScreenInstanceId`, `ID_BYTE_LIMIT = 128`, `MAX_ACTIVATION_FIELDS = 32`
  already exist in `src/workbench/ids.rs`.
- `ScreenDescriptor` already carries `route: RouteId` and
  `activation: Vec<ActivationField>`; `ActivationKind` is already the closed,
  secret-free kind set (boolean / optional-boolean / string / integer / enum /
  path / string-list) in `src/workbench/activation.rs`.
- The closed post-commit effect/correlation contract already exists
  (`src/domain/effects.rs`, `src/state/transition.rs`): `Effect`, `IssuedEffect`,
  `Correlation` (owner, screen_generation, activation_generation, semantic_key,
  correlation_id), `EffectLedger::{register, complete}`, `MAX_TRANSITION_EFFECTS = 64`
  (= the issue's "follow-up limit 64").
- Screen switching today is scattered: `self.screen = …` in `issues_ops`, `prs_ops`,
  `actions_ops`, `errors_ops`, `terminal_manager_ops`, `shell_overlay_ops`,
  `state/mod.rs` (split mode). ~38 write sites, ~187 read sites, ~150 struct-literal
  sites across ~73 files.
- Esc/Back today is resolved per-mode through the action registry handler keys
  `IssuesBack`, `PullRequestsBack`, `ActionsBack`, `ErrorsBack`,
  `TerminalManagerBack`, plus `InlineCancelOrEsc` / `PrInlineCancelOrEsc` and the
  raw-key pre-empt chains in `app_input/issues.rs` / `app_input/prs.rs`.
- There is **no savable draft anywhere in the product today**. The only in-progress
  edit state is the Issues/PR inline composer/editor, which is currently
  *silently auto-discarded* on mode exit with an "Unsent draft discarded" notice.
- Layering: `workbench` depends on `domain`; `domain` must not depend on `workbench`.
  Therefore route/activation value types belong in `src/workbench/`, and the reducer
  in `src/state/navigation.rs` (as the issue specifies).

## 2. Acceptance matrix

| # | Ledger | Behavior | Inputs / boundary cases | Failure behavior | Evidence |
|---|---|---|---|---|---|
| A1 | — | A `RouteDeclaration` is resolved from the published screen registry by `RouteId` | known route; unknown route | unknown route ⇒ `NAV-E001`, `NavState` byte-identical | `tests/core/navigation_contracts.rs` |
| A2 | — | An `Activation` validates against the target screen's declared `activation` schema **before** any mutation | unknown field; missing required field; wrong kind; enum value not in `permitted`; 33 fields; serialized > 262,144 bytes; route/field id > 128 bytes | categorized `ActivationError` ⇒ `NAV-E001`, no mutation | reducer contract tests |
| A3 | — | Activation values are the closed non-secret kinds mirroring `ActivationKind`; no generic payload, no secret kind | each kind round-trips; no `Secret` variant exists | compile-time closed enum | reducer contract tests + exhaustive match |
| B1 | CW06-01 | Push validates, suspends the current instance, appends it exactly, and creates a fresh monotonically-identified instance | depth 0→1, 1→2 | validation failure ⇒ unchanged | `typed-navigation-push-back.json` + reducer golden |
| B2 | CW06-02 | Replace constructs the validated target first, then disposes the current instance without stacking | valid activation; invalid activation | invalid ⇒ current instance and stack byte-identical | replace success/failure transaction tests |
| B3 | CW06-03 | Back with a non-empty stack restores the **exact** prior instance (id, screen, activation, generation, panel focus, subscriptions) | two-instance stack | — | two-instance byte-equivalence test + harness |
| B4 | CW06-07 | Stack max 32; the 33rd Push leaves state unchanged and surfaces `NAV-E001` | depth 32 ok, 33 refused | — | depth 32/33 property test + `navigation-depth-limit.json` |
| B5 | — | Back with an empty stack and no local layer changes nothing and emits no effect | root screen | — | reducer contract test |
| B6 | CW06-08 | A completion naming a suspended / disposed / stale instance or activation generation changes nothing | stale screen_generation; stale activation_generation; disposed instance | — | generation property test |
| C1 | CW06-04 | One Back resolves to exactly one `LocalIntent`, in the exact precedence: host confirmation → dirty guard → chooser → editor → search → filter → non-dirty overlay → focused panel transient → navigation stack | all layers stacked; each layer alone | — | all-layers-stacked table test + `navigation-local-unwind.json` |
| C2 | — | Ctrl-Q remains the protected exit and is never aliased to Back | Ctrl-Q in every UI state incl. dirty modal | — | existing quit tests + dirty-modal scenario |
| C3 | — | Every existing Esc/`q` outcome is preserved behaviorally (routing change, not behavior change) | existing Esc-precedence suites for issues/PRs/actions/errors/terminals | — | existing test suites stay green unchanged |
| D1 | CW06-05 | While a dirty Save is pending, navigation is deferred until a **matching successful** completion | matching completion; stale completion; failure completion | stale ⇒ nothing changes | `navigation-dirty-save.json` + reducer tests |
| D2 | CW06-05 | Save failure retains draft and current instance and exposes Retry / Discard / Cancel | retryable and non-retryable `EffectError` | — | dirty RECOVERY test + scenario |
| D3 | CW06-06 | Discard restores the draft's base authority then performs the pending navigation; Cancel clears the pending navigation and restores the exact modal predecessor focus | discard-then-navigate; cancel-then-stay | — | dirty-choice matrix test |
| D4 | CW06-10 | The dirty modal traps focus: Tab cycles Save/Discard/Cancel, Esc = Cancel, disabled Save shows its reason, and the SMALL layout stacks the choices with Ctrl-Q visible | wide + small terminal | — | harness scenarios (DIRTY, RECOVERY, SMALL) |
| E1 | CW06-09 | The persisted selected-screen value migrates to exactly one current instance: empty stack, generation 1, `DirtyState::Clean`, compiled activation defaults | every legacy value; unknown value | unknown ⇒ default screen, no stack | migration matrix test |
| E2 | — | Stack, drafts, subscriptions, and modal state never persist | durable projection round-trip | — | `durable_projection_tests` extension |
| F1 | — | `AppState.screen` is deleted; `NavState` is the sole runtime screen authority and every previous `self.screen = …` site routes through `reduce_navigation` | all screens | — | full suite + architecture guard token scan |
| G1 | CW06-10 | NORMAL / FOCUSED / UNAVAILABLE / ERROR / DIRTY / RECOVERY / SMALL each render with protected Back/exit and keyboard-reachable modal focus | six distinct scenarios | — | six tmux scenarios |

## 3. Explicit non-goals

- Instance **reuse**, a **persisted** navigation stack, provider navigation, hot
  reload, geometry changes, and the Settings screen (CW-07).
- Moving the `issues_state` / `prs_state` / `actions_state` / `errors_state` /
  `terminal_manager` aggregates into `ScreenInstance.panel_state`. The instance owns
  the descriptor-declared panel focus and its declared per-panel transient; the mode
  aggregates keep their current owners (`*_ops.rs`) and simply stop performing
  cross-screen mutation.
- New dependencies, `unsafe`, `unwrap`/`expect` in production paths, suppression
  directives, or any weakened lint/complexity/size/coverage/CI gate.
- Any change to `.llxprt/`, `.code_puppy/`, `.github/`, dependency manifests, or
  quality-gate scripts.

## 4. Planned vertical slices

1. **S1 — Route/activation contract** (`src/workbench/route.rs`, `src/workbench/mod.rs`):
   `RouteDeclaration`, `ActivationValue`, `ActivationValues`, `Activation`,
   `DraftToken`, `NavError`/`NAV-E001`, limits, resolution from the published
   registry. RED: A1–A3.
2. **S2 — Navigation reducer core** (`src/state/navigation.rs`): `NavState`,
   `ScreenInstance`, `SuspendedInstance`, `NavIntent`, `reduce_navigation` for
   Push/Replace/Back plus the depth bound. RED: B1–B5.
3. **S3 — Generations and stale completions**: instance/activation generation
   allocation wired to `EffectLedger.screen_generation` /
   `.activation_generation`; suspended/disposed rejection. RED: B6.
4. **S4 — Message bus + `AppState` cutover**: `NavigationMessage`,
   `MessageDomain::Navigation`, `AppEvent` conversions, `AppState.nav`, deletion of
   `AppState.screen`, every `self.screen = …` site rerouted. RED: F1 + C3 regression
   suites.
5. **S5 — Local unwind**: `LocalIntent`, the single precedence resolution, and the
   per-mode Esc/Back arms rerouted to emit exactly one intent. RED: C1–C3.
6. **S6 — Dirty lifecycle**: `DirtyState`, `DirtyChoice`, `SaveIntent`,
   `DiscardIntent`, pending-navigation record, Save/Discard/Cancel/Retry
   transitions, host dirty modal UI. RED: D1–D4.
7. **S7 — Migration**: persisted screen value → one clean instance. RED: E1–E2.
8. **S8 — Evidence and docs**: `tests/core/navigation_contracts.rs`, six tmux
   scenarios, and the three standards documents.

## 4a. Slice progress

| Slice | State | Evidence |
|---|---|---|
| S1 route/activation contract | **done** — `d3f7eec3` | `src/workbench/route_tests.rs`, 15 tests green (A1–A3) |
| S2 navigation reducer core | **done** — `44c4d62b` | `src/state/navigation_tests.rs`, 20 tests green (B1–B5) |
| S3 generations / stale completions | **done** — `fa9fa698` | same file, 7 further tests green (B6) |
| S6 dirty guard reducer | **done** — `93b2f2e9` | `src/state/navigation_dirty_tests.rs`, 21 tests green (D1–D3) |
| S5a Back-precedence resolver | **done** — `17cd2c87` | `src/state/navigation_unwind_tests.rs`, 11 tests green (C1) |
| S4 `AppState` cutover | **done** — `5c379c41` | `AppState.screen` deleted; 101 files; full suite green (F1, C3) |
| S5b layer projection | **done** — `617f09fe` | `src/state/navigation_layers_tests.rs`, 13 tests green (C1) |
| S7 migration | **done** — `617f09fe` | migration matrix in `navigation_tests.rs` (E1, E2) |
| S8 docs + scenarios | **done** — `6f48b7e9` + follow-up | three standards docs; two tmux scenarios |

### Dirty-Save semantics: resolved from the adjacent capability, not guessed

`08-settings-shell.md` settles it. Its own source inventory assigns
"own Back dirty confirmation and focus restoration" to the **navigation
reducer**, while `src/state/settings.rs::reduce_settings` owns
`DraftStatus{Clean,Dirty,Saving}`, the writer, the base hash, and the save
completion. CW-06 therefore owns the *guard*; the screen holding the draft
declares what its Save is (`SaveIntent`). CW-07 supplies the first savable
owner. No new `Effect` family was needed and no product behaviour was invented.

Each committed slice passed `cargo fmt --all` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.

### Design decisions taken while implementing S1–S3

- Route/activation value types live in `src/workbench/route.rs`, not `src/domain/`,
  because they reference `RouteId` / `ScreenIdentity` / `ActivationKind` and
  `domain` must not depend on `workbench`.
- `reduce_navigation` is pure and stages **no** effect of its own. Suspension and
  disposal are state facts, not effects, so CW-01's closed `Effect` family
  inventory does not have to grow for Push/Replace/Back. The only effect CW-06
  emits is the dirty save (S6).
- `SuspendedInstance` is a newtype rather than a flag, so a stacked instance
  cannot be read as the current one by accident.
- Staleness is one comparison — `NavState::answers_live_work` — against the live
  instance's `(screen_generation, activation_generation)`. Suspended instances
  are not live, restoring one makes its work live again (this *is* "Back restores
  subscriptions"), and a disposed instance's generations never return because
  generations only move forward.
- `Activation.source_instance` / `activation_generation` identify the snapshot a
  request was computed from, so a request produced against a screen that has
  since been replaced is refused (`NavRefusal::StaleSource`) rather than acted on.

## 4b. Deferred with reason

- **DIRTY / RECOVERY / SMALL harness scenarios (part of G1).** No shipped screen
  declares a savable draft yet, so the guard has no reachable UI to drive from
  the harness. The lifecycle is proven at the reducer (21 behavioural tests
  covering Save-pending, matching/stale/failed completion, Retry, Discard,
  Cancel, and focus restoration) and specified in `display-and-ui.md`. The
  harness scenarios land with CW-07, which supplies the first owner.
- **`navigation-depth-limit` harness scenario (part of B4).** Depth 33 is not
  reachable through the shipped key map; the bound is proven by the depth 32/33
  reducer property test.
- **Routing the per-mode Esc arms through `back_resolution`.** The precedence is
  now stated once and projected from real state, and the resolver is production
  code with behavioural coverage. Rewriting each mode's existing raw-key
  pre-empt chain to consume it is a behaviour-preserving refactor that would
  touch the whole `app_input` surface; it is recorded here rather than bundled
  in, since the existing chains are already proven to agree with the stated
  order.

## 5. Scope ledger

| Entry | Status |
|---|---|
| `src/workbench/screens.rs` compiled `route_of` / `initial_focus` tables | In scope: rooting a session must be total, which F1 requires. Held to the descriptors by two tests. |
| `src/state/navigation_layers.rs` projection | In scope: C1 needs a real answer to "what is open", not a resolver nothing calls. |
| Everything else | Maps to an acceptance row. |

## 6. Review counters

| Review | Used | Cap |
|---|---|---|
| OCR local | 0 | 2 |
| OCR post-PR | 0 | 2 |
| Subagent design/code review cycles | 0 | 2 |

## 7. Open decisions requiring user confirmation

Recorded in the issue thread / chat before implementation starts. See section 8.

## 8. Verification commands

- Iteration: `cargo xtask quick`, focused `cargo test`.
- Green checkpoint / PR head: `make ci-check`.
