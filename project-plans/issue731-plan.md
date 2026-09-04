# Issue #731 — Dashboard focused-pane chrome must follow pane focus

Branch: `issue731`, cut from merged `origin/main` `ab179729`
("Restore the zero-agent Agent Types pane through the shared runtime (#736)").

Workflow: `dev-docs/workflow/ISSUE-DELIVERY.md`.
Prior investigation reused verbatim: `tmp/issue730-panefocus/FINDINGS.md`.

---

## 1. Problem, restated from evidence

Two focus notions exist and were never joined:

| Notion | Type | Written by | Read by |
|---|---|---|---|
| `AppState.pane_focus` | `PaneFocus::{Repositories,Agents,Terminal}` (`src/state/types.rs:213`) | the dashboard/split keyboard (`r`/`a`/`t`/Tab/Left/Right) | arrow routing, paging, delete/activate targets, persistence |
| `ScreenInstance.panel_focus` | `PanelId` (`src/state/navigation.rs:128`) | instance creation, the dirty-guard restore, the package-gated Ctrl+Tab, provider mouse routing | `project_current_screen` (`src/provider_panel_view.rs:193`) → `PanelProjection.focused` → border colour, border style, `▶` marker |

Nothing writes `panel_focus` on `core.dashboard` or `core.repositories`, so it
stays at `initial_focus: repositories` for the life of the process and the
focused chrome is nailed to the Repositories pane. Confirmed empirically in
`tmp/issue730-panefocus/`: 12 frames across `Tab`, `a`, `t`, `Right`, `Down`
have byte-identical border rows, and the raw-ANSI probe shows **0 bytes** of
repaint on `a` and on `r`.

Second-order defect in the same mechanism: the PTY panel forwards
`focused: panel.focused` into `TerminalView` (`src/ui/components/provider_screen.rs:395`),
so a focused terminal still advertises `F12/t to focus`
(`src/ui/components/terminal_view.rs:94-98`). Pre-cutover that prop was
`terminal_focused` (`dashboard.rs:244` at `65231932`).

---

## 2. Focus-authority decision (settled before implementation)

**Decision: one focus authority per screen, resolved at the read boundary. No
value is ever copied between the two fields.**

On the two host-driven shipped screens — `core.dashboard` and
`core.repositories` — `AppState.pane_focus` is the sole authority, and the
focused `PanelId` is *derived* from it every time it is asked for. On every
other screen `ScreenInstance.panel_focus` remains the authority, because those
screens' own runtime (`cycle_panel_focus`, provider mouse routing) is what
writes it. Nothing synchronises the two, so nothing can drift.

The derivation rule is one rule for both screens:

> `PaneFocus` is the ordinal position within the descriptor's declared
> `focus_order`, filtered to the panels this frame's `ResolvedLayout` resolved
> visible. The index clamps to the last visible entry.

- `core.dashboard` declares `[repositories, agents, agent-types, terminal]`.
  Exactly one of `agents`/`agent-types` is ever visible (#734/#736), so the
  visible order is three long and one-to-one with `PaneFocus`:
  `Repositories→repositories`, `Agents→agents` **or** `agent-types`,
  `Terminal→terminal`.
- `core.repositories` declares `[repositories, status, cards]`, all three
  visible: `Repositories→repositories`, `Agents→status`, `Terminal→cards`.

The visibility filter is the same predicate `cycle_panel_focus` already applies
(`src/app_input/provider_panel_input.rs:626-634`), so the two traversals agree.

### Stated consequence on the Repositories split screen

Under this rule `a` focuses `status` and `t` focuses `cards` on the split
screen, because the letter keys are direct jumps to positions 0/1/2 of the
declared traversal and the split's declared traversal is not the dashboard's.
That is the literal reading the issue asks for ("focus_order: [repositories,
status, cards]" is named as the mapping vehicle). The alternative — mapping
`Agents→cards` semantically — makes `status` unreachable and turns `Tab` into a
two-cycle that skips a declared pane, contradicting `focus_order` being the
traversal order. No pre-cutover behaviour exists to restore here: the old split
screen hard-coded `focused: true` on the sidebar and gave the card grid no
focus chrome at all (`git show 65231932:src/ui/screens/split.rs`, line 160).
Recorded as a follow-up question rather than silently re-decided.

### Rejected: collapse `PaneFocus` into `nav.panel_focus`

Rejected as a different issue, not as a worse design. `pane_focus` has 40
non-test read sites and 25 non-test write sites, and three of its
responsibilities are not "which panel is focused":

- it is persisted and restored (`preferences.pane_focus`,
  `src/state/durable_projection.rs:508`, `src/state/durable_restore.rs:309`,
  `src/persistence/mod.rs:195`);
- it is saved and restored across modals as `return_focus`
  (`src/state/modal_ops.rs:391,423`, `src/state/generated_form_submit.rs:45`)
  and across the shell overlay (`src/state/interaction_types.rs:22`);
- two compiled screens read it with a *different* panel set, where
  `PaneFocus::Repositories` means "the sidebar", not "focus_order[0]"
  (`src/ui/screens/issues.rs:235`, `src/ui/screens/pull_requests.rs:253`).

Collapsing would change Issues/PRs sidebar chrome and the persisted schema.
That is a multi-issue cutover and is out of scope here.

### Rejected: synchronise at the `set_pane_focus`/`cycle_pane_focus` boundary

Rejected as the defence-in-depth shim the project's fail-fast preference
forbids. Twenty-five non-test sites assign `pane_focus` outside those two
functions — `src/app_shell.rs:816`, `src/state/shell_overlay_ops.rs:42,81,88,110`,
`src/state/workbench_reducers.rs:92`, `src/state/prs_ops.rs:65`,
`src/state/issues_ops.rs:55`, `src/app_input/modal_handlers.rs:47,59,65,543`,
`src/app_input/relaunch.rs:421`, `src/state/mod.rs:686,844`,
`src/state/state_ops.rs:51,67,128`, `src/app_input/mod.rs:221,241,257,476`,
`src/app_input/terminal_manager.rs:148`,
`src/state/generated_form_submit.rs:45`, `src/app_init.rs:400`,
`src/app_input/normal.rs:93,102`. Every one of them would have to remember to
write a mirror field, and a missed one is a silent wrong-border bug with no
failing test. Deriving on read makes that class of bug unrepresentable.

### Terminal focus is its own authority, applied in the projection

`state.terminal_focused` is the authority for whether the embedded PTY is
focused, and it is applied where every other panel's focus is applied — in
`project_declared_content`, by setting `projection.focused` on the PTY panel.
The renderer keeps reading `panel.focused` and gains no new state read.
`normalize_terminal_focus` (`src/app_shell.rs:698-711`) already guarantees
`terminal_focused ⟹ pane_focus == Terminal`, so this is strictly narrower than
the pane authority and matches pre-cutover `dashboard.rs:244`.

### Readers deliberately left on `nav.panel_focus`

These are unreachable on `core.dashboard`/`core.repositories`, so they are the
provider screens' authority and are not touched:

| Site | Why unreachable on the two host-driven screens |
|---|---|
| `src/app_input/provider_panel_input.rs:16,21,25,31,56` (`apply`) | reached only from the `workbench` keymap context (`src/domain/default_action_inventory.rs:353-405`), disjoint from `dashboard`/`split` |
| `src/app_input/provider_panel_input.rs:566-643` (Ctrl+Tab, `cycle_panel_focus`) | gated on `panel_binding(...).is_some()`; builtin panels carry no `PackagePanelBinding` (`src/workbench/screens.rs:271`) |
| `src/app_input/provider_panel_input.rs:253,257,266` (`apply_mouse_target`) | host-owned panels divert to `apply_host_owned_click` at `:196-201`, which sets no focus |
| `src/mouse_routing.rs:186-192` | `#[cfg(test)]` |
| `src/state/navigation.rs:316,472,645,790` | writers (instance creation, dirty-guard save/restore) |

---

## 3. Acceptance matrix

| # | Actor / launch path | Input & boundary cases | Observable success | Observable failure & diagnostic | Side effects | Persistence / compatibility | Proving test |
|---|---|---|---|---|---|---|---|
| A1 | Dashboard keyboard, real PTY 120x40 | `a` with agents present | Frame contains `╔ ▶ Agents`; `╔ ▶ Repositories` absent; `╭ Repositories` present | assert-frame reports the missing/forbidden literal with the captured frame | none beyond the existing `pane_focus` write + durable save | `preferences.pane_focus` unchanged in shape | scenario `dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json` |
| A2 | Dashboard keyboard, real PTY | `Down` after `a` | repository cursor does not move (`>> Alpha Repo` still present, `>> Beta Repo` absent) — proves `pane_focus` moved, not just a glyph | as above | none | none | same scenario |
| A3 | Dashboard keyboard, real PTY | `t`, then `r` | after `t` no list pane advertises focus (`╔ ▶ Repositories` and `╔ ▶ Agents` both absent); after `r`, `╔ ▶ Repositories` present and `╔ ▶ Agents` absent | as above | none | none | same scenario |
| A4 | `project_current_screen` on `core.dashboard` | `pane_focus = PaneFocus::Agents` | `panels["agents"].focused == true` and `panels["repositories"].focused == false` | assertion names the panel id | pure | none | `dashboard_focus_follows_pane_focus_*` in `src/provider_panel_view_focus_tests.rs` |
| A5 | `project_current_screen` on `core.dashboard`, zero agents | `pane_focus = Agents` while `agent_types_pane_active()` | `panels["agent-types"].focused == true`, `panels["agents"]` not visible | assertion | pure | `focus_order` and `shipped-screen-definition-parity.json` unchanged | same file |
| A6 | `project_current_screen` on `core.dashboard` | stored `nav.panel_focus` deliberately set to `terminal` while `pane_focus = Repositories` | `panels["repositories"].focused` — the stored field is inert, proving one authority | assertion | pure | none | same file |
| A7 | Border colour resolution | focused panel, `PanelStatus::Active` | `panel_border_color(status, focused, rc) == rc.border_focused`; unfocused resolves `rc.border`; `Failed` still resolves `rc.error` | assertion | pure | none | `src/ui/components/provider_screen.rs` unit tests (colour is not observable in schema-1 frame text: `Frame.lines` is `Vec<String>`, `src/harness/v1/report.rs:14-20`) |
| A8 | Dashboard projection + colour | `pane_focus = Agents` | the *agents* projection resolves `rc.border_focused` and the repositories projection resolves `rc.border` | assertion | pure | none | `src/provider_panel_view_focus_tests.rs` |
| A9 | `project_current_screen` on `core.repositories` | `pane_focus = Agents`, then `Terminal` | `panels["status"].focused` then `panels["cards"].focused`; `repositories` unfocused in both | assertion | pure | none | same file |
| A10 | PTY panel projection | `pane_focus = Terminal`, `terminal_focused = false` → `true` | `panels["terminal"].focused` is `false` then `true` | assertion | pure | none | same file |
| A11 | Input geometry | `pane_focus = Agents` on the dashboard | `focused_host_reorder_panel()` and the page capacity read the *agents* pane, i.e. the same authority the renderer reads | assertion | pure | none | `src/app_input/list_navigation.rs` unit test |
| A12 | Whole-tree contracts | any | `tests/issue706_cutover_contracts.rs` green; no legacy screen, no parallel routing, no second geometry source | test output | none | `dev-docs/testing/issue706-owner-evidence.json` untouched | `cargo test --test issue706_cutover_contracts` |

---

## 4. Non-goals

- Collapsing `PaneFocus` into `nav.panel_focus` (section 2).
- Mouse-driven pane focus on the dashboard or split screen (absent today; the
  issue's required behaviour lists `Tab`, `a`, `r`, `t`, Left/Right only).
- Ctrl+Tab focus cycling for builtin panels (needs a package binding).
- Agent row content, preview content, footer chrome, Repositories screen
  geometry — each tracked separately.
- Scenario startup instability (#719).
- Changing `focus_order` on any screen, and therefore
  `src/workbench/shipped-screen-definition-parity.json` stays byte-identical.
- Issues / Pull Requests / Actions / Errors / Settings / Terminal Manager focus
  behaviour.

---

## 5. Slices

### Slice 0 — test seam (refactor only, no behaviour change)

Extract the border-colour choice out of `render_panel` into a pure
`panel_border_color(status, focused, rc)`. Colour cannot be asserted from
schema-1 frames, so A7/A8 need a function to call. Behaviour-preserving;
`cargo test --lib` must stay green across it.

Allowed paths: `src/ui/components/provider_screen.rs`.

### Slice 1 — RED

1. `dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json` (schema 1,
   macos, 120x40, fixture with two repositories and one agent so the `agents`
   pane is the visible workspace form), covering A1–A3.
2. `src/provider_panel_view_focus_tests.rs` covering A4–A6, A8–A10.
3. `panel_border_color` unit tests covering A7.
4. `list_navigation` unit test covering A11.

Every one must fail for the intended reason, captured under `tmp/issue731/`.

### Slice 2 — GREEN: the focus authority

- `src/state/focus_resolution.rs` (new): `resolve_focused_panel(state,
  descriptor, layout) -> PanelId` and `AppState::focused_panel()`.
- `src/state/mod.rs`: register the module.
- `src/provider_panel_view.rs`: `project_current_screen` resolves through it;
  `project_declared_content` sets the PTY panel's `focused` from
  `state.terminal_focused`.
- `src/state/host_panel_input_ops.rs`, `src/app_input/list_navigation.rs`: read
  the same authority (A11).

### Slice 3 — scenario registration

`dev-docs/testing/scenario-execution-manifest.json` and
`dev-docs/testing/scenario-owner-evidence.json` entries for the new scenario
(`tests/scenario_manifest.rs` requires exact classification of the recursive
corpus).

---

## 6. Expected paths

| Layer | Path | Change |
|---|---|---|
| State / focus authority | `src/state/focus_resolution.rs` | new: the single resolver |
| State | `src/state/mod.rs` | module registration + re-export |
| State | `src/state/host_panel_input_ops.rs` | `focused_host_reorder_panel` reads the resolver |
| Projection | `src/provider_panel_view.rs` | focus argument + PTY `focused` |
| Renderer | `src/ui/components/provider_screen.rs` | `panel_border_color` extraction + its tests |
| Input geometry | `src/app_input/list_navigation.rs` | page capacity reads the resolver |
| Tests | `src/provider_panel_view_focus_tests.rs`, `src/provider_panel_view_tests.rs` | new projection focus tests |
| Scenario | `dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json` | new |
| Scenario registry | `dev-docs/testing/scenario-execution-manifest.json`, `dev-docs/testing/scenario-owner-evidence.json` | register the new scenario |
| Plan | `project-plans/issue731-plan.md` | this file |

Explicitly **not** expected to change: `src/workbench/screens.rs`,
`src/workbench/shipped-screen-definition-parity.json`, `src/state/navigation.rs`,
`src/state/mod.rs` focus reducers, `src/app_input/normal.rs`,
`src/app_input/action_handlers.rs`. The issue listed them as candidates; the
chosen authority makes them unnecessary, which is the point of deriving rather
than synchronising.

---

## 7. Scope ledger

| # | Item | Disposition | Rationale |
|---|---|---|---|
| L1 | `panel_border_color` extraction | In scope | A7/A8 need a callable seam; colour is invisible in schema-1 frames |
| L2 | `focused_host_reorder_panel` / page capacity retargeted | In scope | leaving them on `nav.panel_focus` would keep a second live authority on the dashboard, which is the defect |
| L3 | Scenario manifest + owner evidence entries | In scope | `tests/scenario_manifest.rs` fails otherwise; required to ship the scenario |
| L4 | Split-screen `a`→`status`, `t`→`cards` | In scope, stated | direct consequence of the decided rule; recorded, not hidden |
| L5 | Mouse pane focus on dashboard/split | Deferred | absent today, not in required behaviour |
| L6 | Builtin Ctrl+Tab panel cycling | Deferred | needs a package binding; separate issue |
| L7 | Full `PaneFocus` → `PanelId` collapse | Deferred | section 2; multi-issue cutover touching persistence and two compiled screens |

Review counters: OCR pre-PR 0/2, OCR post-PR 0/2.

---

## 8. Verification commands

Strictly serial, logged under `tmp/issue731/`.

```
cargo fmt --all --check
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- \
  -A clippy::all -A clippy::pedantic -A clippy::nursery \
  -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments \
  -D clippy::type_complexity -D clippy::struct_excessive_bools
cargo test --lib
cargo test --test issue706_cutover_contracts
cargo test --test scenario_manifest
cargo run --bin tmux_scenario -- --scenario dev-docs/tmux-scenarios/issue731/dashboard-focus-chrome.json \
  --install jefe=cargo-bin:jefe --install tmux=repo:scripts/harness-tmux-shim.sh --install tmux-real=host-path:tmux
```

Out of this run's remit: `git push`, PR creation.
