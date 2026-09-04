# Issue #734 part one — restore the zero-agent Agent Types availability pane

Branch: `issue734`, based on `origin/main` `3293a3d132e6ccd79353717c9898d1cd81e86f5b`.
Issue: #734 (`Required macOS scenarios were red on main through four merges, and
the zero-agent Agent Types pane is gone`). Related: #719, #731, #730.

This plan covers **part one only**: the content restoration. The gate hole
(required macOS scenario set not blocking pull requests) and the triage record
are the other half of #734 and are explicitly **not** delivered here.

---

## 1. Established evidence this plan builds on

Matched-pair captures already exist; nothing below is re-derived.

| Fact | Source |
|---|---|
| Pre-cutover `65231932` renders a full-width `Agent Types` pane when the dashboard has no agents, replacing the agent list, terminal and preview; sidebar retained | `src/ui/screens/dashboard.rs:171-188` at `65231932`; frame in `tmp/issue731-agentlist/manifest-old-pid/dev-docs__tmux-scenarios__pid-commit-corner.json` |
| The mount was dropped by #715 (`f5826508`), which deleted `src/ui/screens/dashboard.rs`; `src/ui/components/agent_types_status.rs` still compiles with no live caller | `tmp/issue731-agentlist/FINDINGS.md` §2.2, §3.6; #719 maintainer comment |
| `pid-commit-corner.json` passed 8/8 pre-cutover, fails on merged main at step 2 with `HAR-E006: frame does not contain 'Agent Types'` | `tmp/issue731-agentlist/manifest-{old,new}-pid/` |
| The same merged-main frame has no `pid:` in the footer, so step 3 of the same scenario is also unsatisfiable today | `tmp/issue731-agentlist/manifest-new-pid/…` frame 1, last line; `tmp/issue731-agentlist/FINDINGS.md` §3.2 |
| Most of the corpus boots by waiting on availability rows: `Code Puppy  Installed`, `Code Puppy  Installed, enabled`, `LLxprt  Installed` | scenario step dump, §4 below |
| The shared runtime already has the exact mechanism needed: declare every panel, and let `screen_layout::hidden_panel_ids` state which ones the application is hiding this frame (the Settings screen precedent) | `src/screen_layout.rs:214-307`, `src/workbench/screens.rs:772-779` |
| A split child is treated as hidden only when every panel beneath it is hidden, and a hidden child receives `None` cells, so its siblings absorb the space | `src/workbench/resolve.rs:419-442`, `src/workbench/allocate.rs:79-113` |

---

## 2. Acceptance matrix

Every row is decision-complete: one observable success, one observable failure,
one named proof.

| # | Actor / launch path | Input and boundary cases | Success behavior (observable) | Failure behavior and diagnostic | Side effects | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | `project_host_panel(state, AgentTypeAvailability)` | zero agents, four `AgentAvailabilityObservation`s covering NotFound / InstalledCompatible / InstalledIncompatible / ProbeError / pending | Returns a `PanelBody::List` titled `Agent Types` with one item per observation, in observation order; each item's `label` is `"{display_name}  {status_text}, {enabled|disabled}"`; `status` is `Create enabled` / `Create disabled`; `description` is the projected reason (error code prefixed when present) | Wrong arm, wrong order or a missing row fails the projection unit test with the projected rows printed | none (pure) | reuses `agent_status_view::project_agent_type_statuses`; no new status vocabulary | `src/host_panel_models_agent_types_tests.rs` |
| A2 | same | `state.selected_agent_type_index` = 0, 1, out of range | `selected_id` is the indexed id for the selected row; an out-of-range index clamps to the last row rather than selecting nothing | selection assertion fails naming the resolved id | none | mirrors the clamp `workbench_status` applies to `filter_cursor` | `src/host_panel_models_agent_types_tests.rs` |
| A3 | same, empty observation vector | no observations | Projects an empty list, still titled `Agent Types`; `selected_id` is `None` | assertion prints the non-empty body | none | — | `src/host_panel_models_agent_types_tests.rs` |
| B1 | `screen_layout::hidden_panel_ids` on the dashboard | `agents.is_empty() && !agent_type_availability.is_empty()` | Hides `agents`, `terminal` and `preview`; does **not** hide `agent-types` | assertion prints the hidden set | none | same authority the Settings sections use | `src/screen_layout_agent_types_tests.rs` |
| B2 | same | any agent present, or availability empty | Hides `agent-types`; leaves `agents`, `terminal`, `preview` visible | assertion prints the hidden set | none | pre-cutover condition reproduced exactly (`dashboard.rs:171`) | `src/screen_layout_agent_types_tests.rs` |
| B3 | same, shell overlay active | zero agents, availability present, `shell_overlay_active()` | `agent-types` is hidden and the required `terminal` panel stays visible, so the overlay is never rendered into a screen with no visible content pane | assertion prints the hidden set | none | pre-cutover ordering put the availability pane first, but the state is unreachable (an overlay needs a running agent); the safe branch is chosen deliberately | `src/screen_layout_agent_types_tests.rs` |
| B4 | `resolve_layout` on the dashboard descriptor at 100x32 | zero-agent hidden set from B1 | The `agent-types` panel is visible and occupies every column the sidebar does not, full height | assertion prints the resolved rect | none | geometry comes only from the resolver; no second source | `src/screen_layout_agent_types_tests.rs` |
| B5 | `builtin_screens()` parity | — | `core.dashboard` keeps `required_panels: [repositories, terminal]`, `focus_order: [repositories, agents, terminal]` and `collapse_order: [search, preview, agents]` | existing parity test fails against `shipped-screen-definition-parity.json` | none | the new panel is neither required nor collapsible, so the golden record is unchanged | `src/workbench/screens_tests.rs` (existing) |
| C1 | `ProviderScreen` dashboard render, real PTY, 100x32, zero repositories | `dev-docs/tmux-scenarios/pid-commit-corner.json` | Frame contains `Agent Types` (step 2) | `HAR-E006: frame does not contain 'Agent Types'`, exit 4 | none | required macOS disposition, unchanged | scenario run under `scripts/run-scenario-manifest.py` |
| C2 | same | same scenario, step 3 | Frame contains `pid:` in the footer | `HAR-E006: frame does not contain 'pid:'` | none | label text from the existing `process_identity_label` | scenario run + `src/ui/components/provider_screen_footer_tests.rs` |
| C3 | Corpus boot waits | `Code Puppy  Installed`, `Code Puppy  Installed, enabled`, `LLxprt  Installed` | Those literals appear in the availability rows at 100x32, 120x40 and 130x40 without truncation | `HAR-E005: literal … not observed within 15000 ms` | none | two-space separator and `, enabled` suffix preserved byte-for-byte | survey run, §7 |

---

## 3. Non-goals

Explicitly out of this change; each stays broken or unchanged and is reported.

1. **The #734 gate hole.** No edit to `.github/workflows/ci.yml`, to any gate
   script, or to `dev-docs/testing/scenario-execution-manifest.json`
   dispositions. No triage record.
2. **Rewriting `AgentTypesStatus`.** `src/ui/components/agent_types_status.rs`
   is not touched. It keeps compiling and keeps having no live caller; the pane
   is restored through the shared host-control projection over the same pure
   `agent_status_view` projection the component consumes.
3. **The pane's own footer hint row** (`Space Toggle  Enter Details  q Back`).
   The shared runtime owns footer hints; synthesising a fake list row for it
   would carry a hit target no key can service.
4. **Agent row content** (status glyph, `[N]` badge, git suffix, dirty marker) —
   #731.
5. **Preview content** and `Branch: (unknown)` — #731 item B.
6. **Panel focus routing** (`PaneFocus` never reaching `nav.panel_focus`) — #730.
7. **Repositories geometry**, filter band, card density — #731 item D.
8. **The footer's reverse-video band and padding.** Only the right-aligned
   identity label is added (row C2). The band's colours stay exactly as they are.
9. **#719 startup instability itself.** Any scenario that still fails after this
   change is reported, not fixed.

---

## 4. The corpus literals this must reproduce

Extracted from the scenario files, not guessed:

| Scenario | terminal | boot literal it waits on |
|---|---|---|
| `pid-commit-corner` | 100x32 | `Agent Types`, then `pid:` |
| `first-agent-tutorial` | 100x32 | `LLxprt  Installed`; asserts `Code Puppy  Installed` present and `Probe error` absent |
| `kennel-terminal-select` | 100x32 | `Code Puppy  Installed` |
| `code-puppy-chord-passthrough` | 130x40 | `Code Puppy  Installed` |
| `code-puppy-version-fields` | 120x40 | `Code Puppy  Installed, enabled`, `LLxprt  Installed, enabled` |
| `latest-version-fields` | 120x40 | `Code Puppy  Installed, enabled`, `LLxprt  Installed, enabled` |
| `llxprt-version-fields` | 120x40 | `Code Puppy  Installed, enabled` |
| `transient-agent-options` | 120x40 | `Code Puppy  Installed, enabled` |

Width budget check. Sidebar is 22 columns (`SIDEBAR_COLUMNS`); the availability
pane's chrome is `LIST_PANE_CHROME` = `Insets::new(2,1,1,1)`, so the content
width is `cols - 22 - 2`: 76 at 100 cols, 96 at 120, 106 at 130. The list
projection charges the 3-cell marker and the ` [Create …]` suffix (≤ 18) before
truncating the label, leaving ≥ 55 columns for
`Code Puppy  Installed, enabled` (30). No literal can be truncated at any of the
three widths.

---

## 5. Bounded vertical slices

### Slice 1 — the availability model (acceptance A1, A2, A3)

- **Owner / boundary:** host-owned product models. No rendering, no geometry.
- **Allowed paths:** `src/domain/internal_id.rs`,
  `src/workbench/descriptor.rs`, `src/host_panel_models.rs`,
  `src/host_panel_models_agent_types_tests.rs` (new), `src/lib.rs`
  (test-module declaration only).
- **RED:** `src/host_panel_models_agent_types_tests.rs` fails to compile /
  fails because `HostPanelModelSource::AgentTypeAvailability` does not exist.
- **GREEN:** the three rows above pass.
- **Stop and ask if:** the projection needs any new status vocabulary, or
  `agent_status_view` needs editing. It must not.

### Slice 2 — the declaration and the hiding rule (B1–B5)

- **Owner / boundary:** screen declaration plus the single application-hiding
  authority. No new geometry source, no parallel routing, no resurrected screen.
- **Allowed paths:** `src/workbench/screens.rs`, `src/screen_layout.rs`,
  `src/state/selectors.rs`, `src/screen_layout_agent_types_tests.rs` (new),
  `src/lib.rs` (test-module declaration only).
- **RED:** `src/screen_layout_agent_types_tests.rs` fails because the dashboard
  declares no `agent-types` panel.
- **GREEN:** B1–B5 pass, `src/workbench/screens_tests.rs` parity stays green,
  `src/screen_layout_tests.rs` stays green.
- **Stop and ask if:** the golden parity record would have to change, or the
  resolver would need a new concept to give the pane its rectangle.

### Slice 3 — the footer identity label (C2)

- **Owner / boundary:** shared screen chrome. Text only; no colour, background
  or padding change.
- **Allowed paths:** `src/ui/components/provider_screen.rs`,
  `src/ui/components/provider_screen_footer_tests.rs` (new) or the existing
  in-file test module if it stays under the size policy.
- **RED:** a unit test asserting the composed footer carries the identity label
  on its right edge fails.
- **GREEN:** the test passes and `pid-commit-corner.json` reaches `finish`.
- **Scope note:** see the ledger, row L3. This is the one row that is not the
  availability pane. It is here because #734's own delivery item 1 and the task
  directive both require `pid-commit-corner.json` green end to end, and that
  scenario asserts `pid:` at step 3.

### Slice 4 — evidence (C1, C3)

- Scenario run of `pid-commit-corner.json` through the real manifest runner.
- Survey run of the seven previously blocked scenarios.
- Gates: `cargo fmt --all --check`; workspace/all-targets/all-features clippy
  with `-D warnings`; the namespaced complexity gate.

---

## 6. Expected paths by architectural layer

| Layer | Path | Change |
|---|---|---|
| domain identity | `src/domain/internal_id.rs` | one `InternalId::AgentTypeItem` variant plus its wire string |
| screen contract | `src/workbench/descriptor.rs` | one `HostPanelModelSource::AgentTypeAvailability` variant plus its `control_kind()` arm |
| screen declaration | `src/workbench/screens.rs` | the `agent-types` panel descriptor and its layout child |
| host product model | `src/host_panel_models.rs` | `agent_type_availability(state)` and its dispatch arm |
| application hiding decision | `src/screen_layout.rs` | the dashboard's zero-agent rule, stated once |
| state selector | `src/state/selectors.rs` | `dashboard_agent_types_pane_active()` |
| shared chrome | `src/ui/components/provider_screen.rs` | footer identity label (slice 3) |
| tests | `src/host_panel_models_agent_types_tests.rs`, `src/screen_layout_agent_types_tests.rs`, footer test | new |
| test wiring | `src/lib.rs` | `#[cfg(test)] mod …;` declarations |

Untouched by construction: `src/ui/components/agent_types_status.rs`,
`src/agent_status_view.rs`, `src/provider_panel_view.rs`, `src/host_controls.rs`,
`tests/issue706_cutover_contracts.rs`, `dev-docs/testing/*`, `.github/**`,
`.llxprt/**`, `Cargo.toml`.

---

## 7. Verification commands

Cargo runs strictly serially.

1. `cargo build --locked --all-features --bin jefe --bin jefe-harness-probe --bin jefe-capture-shim --bin jefe-jsp-llxprt-fixture --bin tmux_scenario`
2. `cargo test --lib --all-features <focused filters>`
3. `python3 scripts/run-scenario-manifest.py --platform macos --scenario dev-docs/tmux-scenarios/pid-commit-corner.json …`
4. Survey: the same runner over the seven scenarios named in §4.
5. `cargo fmt --all --check`
6. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
7. `CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features -- -A clippy::all -A clippy::pedantic -A clippy::nursery -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::type_complexity -D clippy::struct_excessive_bools`

All logs under `tmp/issue734/`.

---

## 8. Scope ledger

| Row | Change | Maps to | Status |
|---|---|---|---|
| L1 | `InternalId::AgentTypeItem` | A1 | in scope — a list body cannot carry items without stable ids |
| L2 | `HostPanelModelSource::AgentTypeAvailability` | A1 | in scope — the sealed capability is how a host model reaches a declared panel |
| L3 | Footer identity label in `global_chrome` | C2 | **deviation from "availability pane only"**, taken deliberately: `pid-commit-corner.json` step 3 asserts `pid:`, and both #734's delivery item 1 and the task's GREEN target require that scenario green end to end. Bounded to adding the existing `process_identity_label` text on the footer's right edge; no colour, background or padding change. Reported, not hidden. |
| L4 | `dashboard_agent_types_pane_active()` selector | B1, B2 | in scope — the pre-cutover condition needs one name |
| L5 | Two new `_tests.rs` files + their `src/lib.rs` declarations | A*, B* | in scope — the source-size policy warns at 750 lines and `screens.rs` is already at 904 |

Nothing else. No workflow, manifest, dependency, agent-memory or quality-tool
change.

## 9. Review counters

Open Code Review: 0 of 2 pre-PR runs used.

## 10. Deferred / follow-up

- The pane's `Space Toggle  Enter Details  q Back` hint row (non-goal 3).
- Any scenario still failing after this change, reported in the survey with its
  exact error, attributed to #730/#731 or to #719.
