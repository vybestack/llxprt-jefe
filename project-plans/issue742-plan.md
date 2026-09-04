# Issue #742 — name the composition-root screen by its screen name

Branch: `issue742`, based on `origin/main` `ab179729` (`Restore the zero-agent
Agent Types pane through the shared runtime (#736)`).
Issue: #742 (`Settings, Screens names the composition root after the
application`). Corpus cluster: `tmp/triage-corpus/REPORT.md` Cluster C /
"Issue C".

---

## 1. Established evidence this plan builds on

Nothing below is re-derived; each row names where it came from.

| Fact | Source |
|---|---|
| The Screens editor row label is the descriptor title verbatim | `src/state/screens_editor.rs:216`, `title: screen.title.clone()` |
| The shared top bar reads the same descriptor title | `src/provider_panel_view.rs:167` -> `ProviderScreenView.title` -> `src/ui/components/provider_screen.rs:86` -> `themed_header` (`:147,163`) |
| #715 (`f5826508`) renamed the dashboard descriptor title from `Dashboard` to `LLxprt Jefe` in one line | `src/workbench/screens.rs:573` |
| Before #715 the band came from `status_bar.rs:104`, `format!("LLxprt Jefe{title_suffix} - {}", props.version)`, and the descriptor title was read only by the Screens editor | `src/ui/components/status_bar.rs:104` |
| Screen identity never moved: `DASHBOARD_IDENTITY` is `core.dashboard`, and the doc comment above `dashboard_screen()` still calls it the Dashboard | `src/workbench/screens.rs:568-572` |
| The shared band shows the screen's own title for every other runtime screen: `Terminals - 0.0.32` appears in 8 corpus frames, `LLxprt Jefe - 0.0.32` in 71 | `tmp/triage-corpus/evidence/*.frames.txt` |
| Settings, still a legacy screen, renders `StatusBar`, so its band already reads `LLxprt Jefe - 0.0.32` independent of any descriptor | `src/ui/screens/settings.rs:65`; frame 2 of `tmp/triage-corpus/evidence/settings-screens-normal.frames.txt` |
| Exactly four Rust sites carry the literal `LLxprt Jefe` in production: `status_bar.rs:104`, `selection/content.rs:394`, `screens.rs:573`, plus header tests | `grep -rn "LLxprt Jefe" src --include=*.rs` |
| `src/workbench/shipped-screen-definition-parity.json` pins `core.dashboard`'s `title` and the parity test compares it field by field | `src/workbench/screens_tests.rs:16,360-361` |
| No owner-evidence ledger hash-pins `src/lib.rs`, `src/state/screens_editor.rs`, `src/ui/components/provider_screen.rs` or the parity golden. `issue705-owner-evidence.json` hash-pins `src/workbench/screens.rs` and `src/provider_panel_view.rs` | walk of `dev-docs/testing/*.json` for `path` keys |

---

## 2. Design choice: the declaration owns the branding, the band obeys it

The issue leaves this open: "whether that means a second descriptor field or a
branding string owned by the header is a design choice; settle it in the plan".
Three candidates, and the repository's own architecture contract eliminates one
of them outright.

**Candidate A — the header decides, by looking at the screen's identity.**
`ScreenDescriptor.title` returns to `Dashboard`, and the band asks "am I on the
composition root?" before choosing between the product name and the screen
title. **Rejected, and proved impossible.** `tests/issue705_owner_evidence.rs::generic_runtime_paths_do_not_branch_on_dashboard_identity`
lists ten generic runtime paths — including both sites the issue nominates,
`src/provider_panel_view.rs` and `src/ui/components/provider_screen.rs` — and
fails if any of them so much as mentions `DASHBOARD_IDENTITY` or
`core.dashboard` in production code. This was tried first and the gate rejected
it (`tmp/issue742/gates/test-issue705.log`, `generic_runtime_paths_do_not_branch_on_dashboard_identity … src/ui/components/provider_screen.rs`).
The #705/#706 cutover's whole point is that the shared runtime treats every
screen alike, so the runtime may not learn which screen is the home screen.

**Candidate B — a second string field on `ScreenDescriptor`.** Keep `title` as
the screen's name and add, say, `band_title: Option<String>`. Rejected on cost
and on safety. Every one of the 33 `ScreenDescriptor` literals in the tree would
have to set it, and a free-form string field is lowerable: a provider screen
authored in TOML could name itself `LLxprt Jefe`, which is the same defect one
layer down.

**Candidate C — a sealed host capability on the declaration. Chosen.**
`HostScreenCapability` already exists for exactly this shape of problem: "sealed
host authority granted only by compiled screen declarations … local and package
syntax cannot lower these capabilities" (`src/workbench/descriptor.rs:301-312`),
and it is already how the host expresses Dashboard-specific presentation
authority without any runtime branching on identity
(`src/state/types.rs:451` reads `DashboardFooter` through
`has_host_capability`). One new variant, `ProductBrandedHeader`, says the band
this screen displays under carries the product name. `ScreenDescriptor::band_title()`
resolves it, and the projection asks the declaration rather than the identity:

```rust
title: descriptor.band_title().to_owned(),   // src/provider_panel_view.rs
```

Consequences, all deliberate:

- `core.dashboard`'s `title` returns to `Dashboard`, so the Screens editor lists
  a screen name and the parity golden is updated in the same commit; the parity
  test keeps proving the two places agree.
- The product name becomes one named constant, `crate::PRODUCT_NAME`, so a test
  can assert a row label is distinct from *the* branding string rather than from
  a copy of a literal.
- Lowered screens get `host_capabilities: Vec::new()` unconditionally
  (`src/workbench/screen_lowering.rs:259`), so branding stays unavailable to
  provider-authored screens by construction, not by validation.
- The shared runtime stays the single rendering path, and it still knows nothing
  about which screen it is drawing.

---

## 3. Acceptance matrix

Every row is decision-complete: one observable success, one observable failure,
one named proof.

| # | Actor / launch path | Input and boundary cases | Success behavior (observable) | Failure behavior and diagnostic | Side effects | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | `project_screens(&builtin_screens(), &PublishedSettings::default())` | the shipped registry, `core.dashboard` row | The row for `core.dashboard` has `title == "Dashboard"` and `title != crate::PRODUCT_NAME` | assertion prints the projected label beside the branding string | none (pure projection) | row shape, order, enablement lock and provenance unchanged | `src/state/screens_editor_tests.rs::composition_root_row_is_named_for_the_screen_not_the_application` |
| A2 | same | every registered screen | Each row's label is still its descriptor title verbatim, and no row's label equals `crate::PRODUCT_NAME` | existing per-screen equality assertion fails, naming the screen | none | the editor keeps reading one field | `src/state/screens_editor_tests.rs::every_registered_screen_projects_exactly_one_row` (existing) + A1 |
| B1 | `builtin_screens()` -> `core.dashboard` | the shipped composition root | `title == "Dashboard"`, `band_title() == crate::PRODUCT_NAME`, and the declaration owns `HostScreenCapability::ProductBrandedHeader`, so the band's left segment is `LLxprt Jefe - {VERSION}` | assertion prints the title beside the resolved band title | none | reproduces the pre-#715 band text for the composition root | `src/workbench/screens_tests.rs::the_composition_root_is_named_for_the_screen_and_brands_only_the_band` |
| B2 | `builtin_screens()` -> every other screen | `core.repositories`, `github.issues`, `github.pull-requests`, `core.actions`, `core.errors`, `core.terminals`, `core.settings` | None claims `ProductBrandedHeader`; `band_title() == title`, so `Terminals - 0.0.32` still renders | assertion names the screen that claimed the product | none | the 8 corpus frames pinning `Terminals - ` stay valid | `src/workbench/screens_tests.rs::every_other_shipped_screen_bands_its_own_title` |
| B3 | `themed_header` | any title, kennel mode on/off | Unchanged: `{title}{kennel} - {VERSION}`, band colours `rc.border`/`rc.bg` | existing header assertions fail | none | #723 band styling untouched | `src/ui/components/provider_screen.rs` tests (existing) |
| C1 | `builtin_screens()` parity against the golden | `core.dashboard` | `title` is `Dashboard` in both the compiled table and `shipped-screen-definition-parity.json`; every other golden field byte-identical | parity test prints the field that diverged | none | golden updated deliberately in the same commit | `src/workbench/screens_tests.rs` (existing) |
| D1 | Real PTY, 100x32, `settings-screens-normal` | Settings -> Screens | Step 8 frame contains `Screens (8)`, `Dashboard`, `Settings`; scenario exits 0, 12 steps | `HAR-E006: frame does not contain 'Dashboard'`, exit 4, 9 steps | none | required macOS disposition unchanged | `scripts/run-scenario-manifest.py --platform macos --scenario …/settings-screens-normal.json` |
| D2 | Real PTY, `settings-screens-{dirty,error,focused,recovery,unavailable}` | Settings -> Screens, cursor on the first row | Step 9 observes `>>Dashboard` within 20000 ms | `HAR-E005: literal '>>Dashboard' not observed within 20000 ms`, exit 4 | none | required macOS disposition unchanged | same runner, one invocation per scenario |
| D3 | Real PTY, `pid-commit-corner` | dashboard at 100x32 | Still passes: `Agent Types` pane, `pid:` footer, and the band still reads `LLxprt Jefe` | any step failure with its `HAR-E00x` code | none | regression check for the band after the rename | same runner |
| D4 | Real PTY, `v1/custom-screen-enable` and the dashboard-boot corpus | scenarios that wait on `LLxprt Jefe` while on the composition root | The literal still appears, now from the header rather than the descriptor | `HAR-E005: literal 'LLxprt Jefe' not observed` | none | branding preserved where it was | same runner (survey, §7) |

---

## 4. Non-goals

Each stays exactly as it is; none is touched by this change.

1. **Screen ordering and enablement locks.** Correct today; the mandatory-row
   rule and `MANDATORY_SCREEN_REASON` are read, not edited.
2. **The `Keys` count** (`Keys (339)` vs `337`, corpus Cluster H) and every other
   Settings sidebar count.
3. **The legacy `StatusBar` band** (`src/ui/components/status_bar.rs:104`) and
   the selection-copy status projection (`src/selection/content.rs:394`). Both
   keep their own `LLxprt Jefe` literal. Folding them onto `PRODUCT_NAME` is a
   tidy-up of code this issue does not otherwise touch, and the legacy band is
   on its way out with the remaining legacy screens.
4. **Which screens run on the shared runtime.** No screen is migrated, added or
   removed; the runtime stays shared.
5. **Every other corpus cluster**: A (confirmation button row), B
   (`Navigate to vendor.panel.open`), D (overlay action rows), E, F, G, H, I, and
   everything tracked by #730, #732, #733, #737.
6. **`ScreenDescriptor`'s field set.** No field added or removed (Candidate B,
   rejected in §2); the one new declaration is a variant of the existing sealed
   `HostScreenCapability` enum.

---

## 5. Vertical slices

One behavior, one commit. The slice boundary is the RED proof.

### Slice 1 — the Screens editor names the composition root by its screen name

- Acceptance rows: A1, A2, B1, B2, B3, C1, D1, D2, D3, D4.
- Architecture owners: `src/workbench` (the screen declaration), `src/state`
  (the pure editor projection, unchanged code, new test), `src/ui/components`
  (the chrome band that now owns branding), crate root (the branding constant).
- Allowed paths:
  - `src/workbench/descriptor.rs` — the `ProductBrandedHeader` capability and
    `ScreenDescriptor::band_title()`.
  - `src/workbench/screens.rs` — the descriptor title returns to `Dashboard`;
    the composition root declares the sealed branding capability.
  - `src/workbench/shipped-screen-definition-parity.json` — golden kept in sync.
  - `src/lib.rs` — `pub const PRODUCT_NAME`.
  - `src/provider_panel_view.rs` — the projection asks the declaration for the
    band title.
  - `src/workbench/screens_tests.rs`, `src/state/screens_editor_tests.rs` — the
    behavioral tests.
  - `dev-docs/tmux-scenarios/settings-screens-unavailable.json` — see ledger S7.
  - `project-plans/issue742-plan.md` — this plan.
  - owner-evidence ledgers, only as deterministic re-pinning of drifted hashes.
- RED: `settings-screens-normal` against a binary built from the branch head,
  then the unit test in `screens_editor_tests.rs`, both failing before any
  production edit.
- GREEN: §3 rows proven; the six `settings-screens-*` scenarios, plus
  `pid-commit-corner` and `issue704/atomic-success`-style unaffected neighbours
  left as they were.
- Stop conditions: any need to add a `ScreenDescriptor` field, to migrate a
  screen off the shared runtime, to edit a gate script, `.github/`, `.llxprt/`
  or a dependency manifest, or to re-pin `issue706-owner-evidence.json`.

---

## 6. Expected paths, by architectural layer

| Layer | Path | Change |
|---|---|---|
| Screen definition | `src/workbench/descriptor.rs` | `HostScreenCapability::ProductBrandedHeader`; `ScreenDescriptor::band_title()` |
| Screen declaration | `src/workbench/screens.rs` | `title: "Dashboard"`; the composition root declares `ProductBrandedHeader` |
| Screen declaration golden | `src/workbench/shipped-screen-definition-parity.json` | `"title": "Dashboard"` for `core.dashboard` |
| Crate root | `src/lib.rs` | `pub const PRODUCT_NAME: &str = "LLxprt Jefe";` |
| Screen projection | `src/provider_panel_view.rs` | `ProviderScreenView.title` comes from `descriptor.band_title()` |
| Definition tests | `src/workbench/screens_tests.rs` | B1, B2 |
| State projection tests | `src/state/screens_editor_tests.rs` | A1, A2 (production file unchanged) |
| Required scenario | `dev-docs/tmux-scenarios/settings-screens-unavailable.json` | stale literal corrected, ledger S7 |
| Evidence ledgers | `scenario-owner-evidence.json`, `issue704-owner-evidence.json`, `issue705-owner-evidence.json` | deterministic re-pin only |
| Plan | `project-plans/issue742-plan.md` | this document |

`src/ui/components/provider_screen.rs` is listed by the issue as a candidate
site and is **not** changed: it keeps rendering `view.title` verbatim, and the
gate in §2 is the reason it must.

---

## 7. Scope ledger

| # | Discovered work | Disposition |
|---|---|---|
| S1 | `shipped-screen-definition-parity.json` pins the title | **In scope.** The golden exists to force a deliberate two-place edit; that is exactly what this is. Acceptance row C1. |
| S2 | `issue705-owner-evidence.json` hash-pins `src/workbench/screens.rs` | **In scope, mechanical.** Deterministic re-pin, old hashes proved to reproduce from HEAD first, `issue706-owner-evidence.json` left byte-identical. |
| S3 | `status_bar.rs` and `selection/content.rs` each carry their own `LLxprt Jefe` literal | **Defer.** Non-goal 3. Worth a follow-up when the legacy screens go, not now. |
| S4 | The selection-copy status projection reports `LLxprt Jefe` on screens whose band reads their own title | **Defer.** Pre-existing since #715, unobserved by any required scenario, unrelated to the Screens list. Follow-up material. |
| S5 | `dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json` was requested as a regression check but exists only on the unmerged `issue731` branch | **Reported, not run.** It is absent from `origin/main`, so it cannot run against this branch; `pid-commit-corner` covers the dashboard band regression instead (row D3). |
| S6 | `settings-screens-small.json` exists alongside the six named scenarios | **Run as a neighbour check.** Not required by the issue, but it is the seventh file in the same family and must not regress. |
| S7 | With row A1 green, `settings-screens-unavailable` advances past step 9 and then fails at step 11 waiting for `compiled screens are always composed and cannot be turned off`, while the editor emits `shipped screens …` (`src/state/screens_editor.rs:37`) | **In scope, scenario corrected.** Same commit, same collateral: #715 (`f5826508`) reworded `MANDATORY_SCREEN_REASON` and left the scenario pinning the old text. The wording change is the correct one — the lock covers the open builtin definitions *and* the residual compiled adapters, so "compiled" understates it — so the scenario literal is stale and moves to `shipped`. Evidence: `tmp/issue742/green/reports-settings-screens-unavailable` before and after. Without it the issue's stated GREEN target (all six) is unreachable. |
| S8 | The band-title rule cannot live in the runtime: `generic_runtime_paths_do_not_branch_on_dashboard_identity` forbids it | **In scope, design settled in §2.** One sealed `HostScreenCapability` variant, no `ScreenDescriptor` field, no identity branch in any generic path. |

---

## 8. Verification commands

Run serially on the exact candidate head:

```
cargo fmt --all --check
cargo test --workspace --all-features --locked
cargo xtask coverage
cargo xtask check source-size
cargo xtask check architecture
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- \
  -A clippy::all -A clippy::pedantic -A clippy::nursery \
  -D clippy::cognitive_complexity -D clippy::too_many_lines \
  -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools
cargo test --test scenario_manifest --all-features --locked
cargo test --test issue704_owner_evidence --all-features --locked
cargo test --test issue705_owner_evidence --all-features --locked
cargo test --test issue706_cutover_contracts --all-features --locked
cargo test --test harness_authority --all-features --locked
git diff --check
```

Scenarios, macOS, one invocation each through
`scripts/run-scenario-manifest.py`: `settings-screens-{normal,dirty,error,focused,recovery,unavailable,small}`
and `pid-commit-corner`.

---

## 9. Review counters

| Counter | Cap | Used |
|---|---|---|
| OCR runs before PR | 2 | 0 |
| OCR runs after PR | 2 | n/a (no PR opened for this delivery) |

---

## 10. Evidence

Logs and reports under `tmp/issue742/`:

- `red/` — the pre-change scenario run and its frames, the failing unit test.
- `green/` — the post-change scenario runs and the mandatory gate logs.
- `pins/` — the deterministic re-pin proof.
