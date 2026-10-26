# Issue #758 Plan: Restore the remaining required macOS scenario regressions

- Issue: vybestack/llxprt-jefe#758 (open, acoliver, no comments). Groups #726, #737, #739, #740, #743 for one bounded PR.
- Linked issues read in full: #726 (windowing evidence), #737 (Terminal Manager empty state, one comment 2026-09-04 adding `paste-enter-escape.json` and `terminal-manager.json` as RED cases), #739 (provider outcome destroyed by navigation), #740 (confirmation buttons/title/submit-id/checkbox), #743 (Agent Shell title).
- Branch: `issue758` at exactly `aa897813` (`aa8978135619f988952dd3c5a004180a743dc499`, current `origin/main` tip "Restore dashboard Agents row presentation (#730) (#757)").
- Plan written: 2026-09-06, research only. No production, test, or scenario files were edited for this plan; `cargo` was not run.
- Evidence base: `tmp/issue730/ci1/` (six `tui-scenarios-macos-{0..5}` result sets from the #757 PR-head run, base/cutover shard logs, PR metadata) plus fresh source reads at `aa897813` and recovered pre-cutover sources (`f5826508^`, `394f59aa^`).

## 1. Root causes, verified at `aa897813`

All five regressions share one history: the #706/#715/#720 cutover moved presentation onto the shared definition/control runtime and deleted the legacy screens. Each loss is a datum the shared projection never carried.

### 1.1 #739 — the navigate outcome destroys its own surface

Two mechanisms, both introduced by `f5826508` (#715), destroy the outcome row in the same tick:

1. `apply_provider_host_action_state` (`src/app_input/provider_dispatch.rs:427-438`) commits `ProviderMessage::DismissTerminals` *before* `state.enter_provider_route(...)`. The reducer handler (`src/state/provider_request_ops.rs:85-92`) drains terminal rows for the current (invoking) instance, so the completed request is deleted before the push. Pre-#715 the same function pushed with no dismissal (`f5826508^:src/app_input/provider_dispatch.rs:309-310`).
2. `provider_surface_projection` (`src/state/overlay_projection_ops.rs:154-206`) finds the request with `context_screen == current.screen && context_instance == current.id`. After the push, `current.id` is the pushed instance; the request belongs to the old one, so `find` returns `None` even if the row survived. Pre-#715 the projection was `requests.last()?` with no scoping (`f5826508^:src/app_shell.rs:47-62`), which is why the scenarios press Escape *after* the wait.

The authority check is a third, separate consumer: `authorize_provider_outcome` (`src/app_input/provider_dispatch.rs:449-465`) requires the request to belong to the exact current instance before an outcome may apply.

### 1.2 #740 — one deleted renderer, three presentation losses

`f5826508` deleted `src/ui/modals/confirm.rs` (#228 contract from `9da0d9cc`: focused choice in parentheses, unfocused in brackets, two-space separator — `( Cancel )  [ Confirm ]`). The replacement routes both confirmation kinds through `project_form`:

- `project_confirmation` (`src/overlay_controls.rs:170-201`) and `project_provider_confirmation` (`src/overlay_controls.rs:243-272`) resolve `decision` to the focused label alone and render it as a form field row (`Decision: Cancel`), so the unfocused choice never appears.
- `project_form` (`src/host_controls.rs:705-752`) renders fields as `label: value` (`Delete work directory: false`, not a checkbox) and appends the leak row `submit: host.overlay-submit` (`:748`), already ruled a defect for edit forms in #728.
- The provider branch passes the provider's own title through `project_form`, while `project_provider_progress/_error/_status` all set the host title `Provider Action` (`src/overlay_controls.rs:398, 429, 531`).

`confirmation_command` (`src/overlay_controls.rs:299-327`) and `provider_confirmation_focus` derive intent from the typed `FormBody` (decision string on `InternalId::OverlayDecision`, `Submit` → ChooseCancel/ChooseConfirm, `Scroll` → CycleFocus), not from row text — so the rendered rows can be replaced without touching the intent contract. Mouse hit-testing maps content lines to `row.target` (`confirmation_hit_target_at_content_line`, `src/ui/orchestration.rs:110-149`), so bespoke rows must keep their targets.

### 1.3 #737 — the Terminal Manager has no empty state

`session_list` (`src/host_panel_models.rs:558-608`) projects `project_managed_shell_rows` into `ListBody` items; an empty row list yields zero items, and `project_list` (`src/host_controls.rs:509`) has no empty-state branch. The deleted screen rendered `empty_message = Some("No shells.")` (`394f59aa^:src/ui/screens/terminal_manager.rs`).

### 1.4 #743 — the attached live shell lost its title (and the manager its hint)

`project_declared_content`'s PTY branch (`src/provider_panel_view.rs:262-267`) forces the title `Terminal` for every embedded PTY that is not the manager's read-only preview. The failing frame proves focus is already correct — the pane renders `║ Terminal (F12 hide shell)` after attach (focus hint selection at `src/ui/components/terminal_view.rs:94-98` follows `props.focused`, and `projection.focused = state.terminal_focused`). Only the title is wrong. Separately, `render_embedded_terminal` (`src/ui/components/provider_screen.rs:516-555`) passes `focused_hint: None`, so the manager context lost its pre-cutover hint `F12 list | F10 close shell` (`394f59aa^:src/ui/screens/terminal_manager.rs:229-236`).

### 1.5 #726 — windowing evidence cannot assert under label truncation

`dashboard-list-windowing.json` fixtures 25 repositories named `Repository 0`..`Repository 24` (in its embedded `config/state.json`, with `selected_repository_index: 24`, `selected_agent_index: 24`). At its fixed 100×25 terminal the repositories sidebar is 20 inner cells; every row elides to `   Repository… (0)` (verified in the ci1 failing frame), so the boot sentinel `Repository 24` and the window assertion `Repository 7` can never match. #726 names two options; the decision below takes Option 1 (short labels, no geometry change).

## 2. RED evidence (all from `tmp/issue730/ci1`, PR-head run for #757)

| Scenario | Shard | Failing step | Error |
|---|---|---|---|
| `issue704/atomic-success.json` | macos-5 | 4 (wait) | `HAR-E005: literal 'Navigate to vendor.panel.open' not observed within 15000 ms` |
| `issue704/restart-publication.json` | macos-5 | 4 (wait) | same literal |
| `package-panel-lifecycle.json` | macos-2 | 3 (wait) | same literal |
| `confirm-dialog-focus.json` | macos-1 | 5 (assert-frame) | `HAR-E006: frame does not contain '( Cancel )'` |
| `issue-dirty-copy-confirm.json` | macos-4 | 12 (assert-frame) | `HAR-E006: frame does not contain '( Cancel )'` |
| `provider-action-confirmation.json` | macos-1 | 4 (assert-frame) | `HAR-E006: frame does not contain 'Provider Action'` (same assert also requires `Deploy now`) |
| `dashboard-list-windowing.json` | macos-2 | 1 (wait) | `HAR-E005: literal 'Repository 24' not observed within 30000 ms` |
| `code-puppy-chord-passthrough.json` | macos-5 | 5 (wait) | `HAR-E005: literal 'No shells' not observed within 12000 ms` |
| `kennel-terminal-select.json` | macos-0 | 5 (wait) | same literal |
| `terminal-manager.json` | macos-5 | 4 (wait) | same literal (hits #737 first, then `Agent Shell` at steps 23/29/49 and `F12 list | F10 close shell` at 68/76) |
| `paste-enter-escape.json` | macos-3 | 5 (wait) | same literal |
| `agent-shell-overlay.json` | macos-3 | 18 (wait) | `HAR-E005: literal 'Agent Shell' not observed within 10000 ms`; failing frame pane reads `Terminal (F12 hide shell)` |

All five sub-issues are product-side RED: no scenario content needs to change to expose these failures (the product stopped rendering pinned literals). The two exceptions that DO need pin updates are #740's contradictory pins: `issue705/semantic-continuation-{macos,linux}.json` step 10 asserts `Decision: Cancel` and step 12 waits `Decision: Deploy alpha` — authored by the same `f5826508` — which conflict with the restored #228 row and must be re-pinned when the renderer lands (Slice 3).

That statement is about exposing the RED, and it holds: none of these scenarios had to change for the failures to reproduce. It is not a claim that scenario bytes stay untouched for the whole issue. Implementation did edit further scenarios, all additively: the seven A1 proof-strengthening files and the §14 item 2 Terminal Manager fixture labels, both enumerated in §7.

## 3. Decisions (settled here, before implementation)

### D1 (#739): carry the terminal request onto the pushed instance; authorization stays instance-scoped

The issue offers two designs: (a) separate the display query from instance-scoped authority, or (b) carry the terminal request onto the pushed instance. **Decision: (b), carry.**

Mechanics, in `apply_provider_host_action_state` (`src/app_input/provider_dispatch.rs`):

1. Delete the `DismissTerminals` `commit_pure_site` block (restores the pre-#715 push-only behavior).
2. After `state.enter_provider_route(*route, values.clone())` — and only when the current instance actually changed (`enter_provider_route` can refuse, e.g. `NAV-E001`, leaving the instance in place with `error_message` set; see `src/state/settings_registry_provider_tests.rs:700-723` for the refusal shape) — capture `(old_screen, old_instance)` before the push and `(new_screen, new_instance)` after, and call one new reducer method: `ProviderRequestState::rebind_terminal_context(&old_screen, &old_instance, &new_screen, &new_instance) -> usize`, retargeting only requests that are terminal AND belong to the old pair.
3. `rebind_terminal_context` lives beside `drain_terminal_for` (`src/state/provider_requests.rs:246`) and follows its retain-style contract. Pending confirmations and live requests are never rebound (a navigate outcome is terminal by construction; a pending confirmation on the origin instance stays owned by it).

Why carry beats query relaxation:

- Escape routing for the provider surface is triple-gated on instance-scoped lookups: `provider_surface_route_state` (`src/app_shell_key_routing.rs:160-176`, requires `latest_current_provider_request().is_some() || provider_surface_action().is_some()`), `provider_surface_message` (`src/app_input/provider_dispatch.rs:111-158`, whose terminal arm produces `DismissTerminals`), and the `DismissTerminals` reducer itself (drains for the current pair). Design (a) must relax all three coherently or the first Escape after navigation stops dismissing the row — and the scenarios press Escape after the wait (`issue704/atomic-success` steps 5-6: first Escape dismisses the outcome, the frame keeps `Package Review`; the second Escape pops back to `Agent Types`). Rebinding keeps one uniform rule: the displayed surface belongs to the current instance because the state says so.
- Authority stays exactly as-is: `authorize_provider_outcome` and `prepare_provider_navigation` run inside `prepare_provider_host_outcome_state`, before the push, against the invoking instance. Nothing in the authority path learns about rebinding.
- Retry from the pushed instance becomes possible (pre-cutover parity; the terminal surface offers `Enter Retry   Esc Close`). Accepted.

`provider_surface_projection` and `overlay_projection_ops.rs` are deliberately untouched. No new abstraction: one method on the existing state struct, one call site.

### D2 (#740): bespoke confirmation rows on the existing typed Form contract (#728 pattern)

Both `project_confirmation` and `project_provider_confirmation` keep building the typed `FormBody` (decision `String` on `InternalField::ConfirmationDecision` → `InternalId::OverlayDecision`; `DeleteWorkDir` Bool for the generic branch; continuation schema/values for the provider branch) with the `OverlaySubmit` affordance, exactly as today, so `overlay_intent`, `confirmation_command`, `provider_confirmation_focus`, `confirmation_delete_work_dir_value`, and `provider_confirmation_field_edit` keep answering unchanged. What changes is the *rendered rows*, assembled the way `bespoke_form_projection` (`src/overlay_controls.rs:624`) assembles definition-generated forms:

- message/body rows first (existing `prepend_detail_rows`);
- generic branch, when `show_delete_work_dir`: one checkbox row `[x] Delete work directory` / `[ ] Delete work directory`, target `Field(OverlayDeleteWorkDir)`;
- provider branch: continuation schema fields keep their editable `label: value` rows (they are genuine form fields; focus target unchanged);
- one two-choice button row restoring the #228 contract: focused choice in parentheses, unfocused in brackets, two-space separator — generic `( Cancel )  [ Confirm ]` (swapping with focus), provider `( Cancel )  [ {confirm_label} ]` (e.g. `[ Deploy now ]`, `[ Deploy alpha ]`). Row target stays `Field(OverlayDecision)` so mouse Activate resolves through the existing decision path.

Title: the provider branch sets the host title `Provider Action` (consistent with progress/error/status); the generic branch keeps its modal title (`Delete Agent`, `Working Copy Not Ready`, ...). The `submit: host.overlay-submit` row is never rendered for either branch (it simply is not among the bespoke rows; `project_form` itself is not modified, so edit forms keep their current behavior — their leak is #728 follow-up work, out of scope).

Width: `HostOverlayLayout::confirmation` is the bounded 50×10 modal; the longest row `( Cancel )  [ Deploy alpha ]` is 27 cells against a 48-cell content width. The provider surface uses 60. Both fit.

### D3 (#737): the empty state is one placeholder list item, projected by the session model

`session_list` projects exactly one `ListItem` labeled `No shells.` (pre-cutover bytes) when `project_managed_shell_rows` yields no rows; `selected_id` stays `None` (matching the existing pinned test `session_list_carries_no_selection_when_the_row_list_empties`, `src/host_panel_models_session_tests.rs:41`). `project_list`'s first-item default paints the `>> ` selection marker, so the rendered row is `>> No shells.`; the corpus pins only the literal `No shells`, which matches. No `ListBody`/protocol change, no empty-state concept added to the shared control: the placeholder is host-model content, the same layer that owns every other session row. Selecting/activating the placeholder resolves to `SessionItem(0)`, which the manager's selection handlers already treat as absent (the underlying row list is empty); no input-path change is needed.

### D4 (#743): title by attachment, hint by screen

- `project_declared_content`'s PTY branch (`src/provider_panel_view.rs:262-267`): while the attached live shell renders (`state.shell_overlay_active()`), the panel title becomes `Agent Shell`; otherwise it stays `Terminal`. The manager's read-only preview path (`TERMINALS_IDENTITY && !shell_overlay_active()`) is untouched.
- `render_embedded_terminal` (`src/ui/components/provider_screen.rs:516-555`): pass `focused_hint: Some("F12 list | F10 close shell")` when the current screen is the Terminal Manager (`TERMINALS_IDENTITY`), restoring the pre-cutover manager hint; every other screen keeps `None`, whose default renders `F12 hide shell` (`terminal_view.rs` focused arm). This satisfies both scenario families: `terminal-manager.json` steps 68/76 wait `F12 list | F10 close shell`; `agent-shell-overlay.json` steps 19/25 wait `F12 hide shell`.
- The focus-acquisition criterion is then met by construction: the failing frame already shows the focused pane selecting the hide hint; with the title fixed, a focused live shell reads `Agent Shell (F12 hide shell)` (or the manager hint) and never the acquisition hint. An unfocused live shell keeps `F12/t to focus` — pre-cutover parity, and the acquisition hint is correct in that state. `terminal_view.rs` itself needs no change.

### D5 (#726): short fixture labels, no geometry change (#726 Option 1)

Verification reconciliation, 2026-09-07: the original fixture-only decision below is retained as the planning record, not a description of the current diff. The working tree also implements selected-List-row viewport reconciliation in `src/provider_panel_view.rs` for host and provider panels, with matching dashboard clipboard projection in `src/selection/dashboard_content.rs`. Section 9 records it as `In-scope-Fix`, and `tmp/issue758/slice4b-final/audit.md` records a nine-step windowing scenario pass. Neither that audit nor the ledger supplies explicit approval for crossing Slice 4's original stop condition. That authorization remains undocumented in the evidence inspected by this verification task. No approval is inferred here. The current full test run also exposes a wrapped-control scrolling failure; see Section 13.

Rename the embedded fixture's repository names to `Repo 0`..`Repo 24` and update the scenario literals to match: step 1 wait `Repo 24` (boot sentinel), step 3 contains `Repo 7`. Agent names stay `Agent 0`..`Agent 24` (the Agents pane is wide enough; steps 2/6 already pass against it in wide panes). Geometry, sidebar width, and the `count` suffix convention (#745) are untouched.

- Fit check at the fixed 100-col terminal: sidebar content is 20 cells; the selected row `>> Repo 24 (0)` is 15 cells, unselected `   Repo 7 (0)` is 14. Distinguishable.
- Collision check: `Repo 7` is not a substring of `Repo 17` (the `7` follows `1`, not the space); `Repo 24` is unique. No assert becomes ambiguous.
- `issue621`'s `list-send-fixture` (17-char names, green in wide panes) is explicitly not audited here — #726 notes it, and it stays safe until its panes narrow; ledgered as a follow-up note, not work.

## 4. Acceptance matrix

| # | Actor / launch path | Boundary cases | Observable success | Observable failure & diagnostic | Scenario / test proof |
|---|---|---|---|---|---|
| A1 | Operator invokes a provider action (F2 keybind) whose outcome is `Navigate` (`issue704` fixtures, `package-panel-lifecycle`) | Terminal navigate outcome; destination route push in the same tick; outcome arrival while already on destination | `Navigate to {route_id}` row stays visible on the pushed screen until Escape; first Escape dismisses the row without popping nav; second Escape pops to the origin | Row vanishes with the push → `HAR-E005 'Navigate to vendor.panel.open'` at the wait | `issue704/atomic-success` (step 4), `issue704/restart-publication` (step 4), `package-panel-lifecycle` (step 3) |
| A2 | Same path | Authority staleness: outcome arriving after the instance changed by any other means | `authorize_provider_outcome` still rejects with `provider outcome authority is stale` — instance scoping intact | A stale outcome applying on a foreign instance (must not happen; covered by existing authority tests) | Existing `provider_dispatch_tests` authority cases stay green; new unit test pins display-not-authority separation |
| A3 | Operator opens any generic confirmation (ctrl-d delete agent, dirty-copy prompt, kill, preflight, server-lost, origin mismatch) | With and without the `Delete work directory` checkbox; focus on Cancel and on Confirm | Both choices render simultaneously as `( Cancel )  [ Confirm ]` (focused in parens); checkbox renders `[x]`/`[ ]`; no `submit:` row; modal title preserved | One-field `Decision: Cancel` form, `Delete work directory: false`, leaked `submit: host.overlay-submit` | `confirm-dialog-focus` (steps 5/7/9/16), `issue-dirty-copy-confirm` (steps 11-12) |
| A4 | Provider requests a host confirmation (`vendor.deploy.ship`) | Continuation schema present or empty; focus Cancel/Confirm; custom `confirm_label` | Overlay titled `Provider Action`; body rows plus `( Cancel )  [ {confirm_label} ]`; both choices visible; no internal id | Missing `Provider Action`/`Deploy now` → `HAR-E006` | `provider-action-confirmation` (step 4); re-pinned `issue705/semantic-continuation-{macos,linux}` |
| A5 | Confirmation intent paths (Tab/right/left/Enter/Esc, checkbox edit, continuation field edit) | Decision values `Cancel` vs confirm label; Bool field edit | `confirmation_command`/`provider_confirmation_focus`/`confirmation_delete_work_dir_value`/`provider_confirmation_field_edit` answer exactly as before (typed contract unchanged) | Focus cycling or activation regressions | Existing overlay/modal unit tests stay green; new bespoke-row unit tests |
| A6 | Operator opens the Terminal Manager (F7) with zero shells | Empty inventory after restore; stale `selected_index` | List pane renders `No shells.` through the shared List control | Empty pane → `HAR-E005 'No shells'` | `code-puppy-chord-passthrough` (step 5), `kennel-terminal-select` (step 5), `paste-enter-escape` (step 5), `terminal-manager` (step 4) |
| A7 | Operator attaches a live shell (F10 from manager row, F10/F12 shell overlay on a workbench screen) | Focused vs unfocused live shell; manager screen vs other screens; read-only preview when not attached | Attached live shell pane titled `Agent Shell`; focused manager shell shows `F12 list | F10 close shell`; focused overlay on other screens shows `F12 hide shell`; unfocused keeps `F12/t to focus` | `Terminal (F12 hide shell)` pane, no `Agent Shell` → `HAR-E005` | `agent-shell-overlay` (steps 18-19, 25, 33-35, 41), `terminal-manager` (steps 23/29/49, 68/76) |
| A8 | Harness runs `dashboard-list-windowing` at its fixed 100×25 terminal | 25 repositories; selected index 24; window scroll; tab+up | `Repo 24` boot sentinel visible; `Repo 7` and `Agent 24` visible in the initial window; `Agent 23` appears after tab+up; rows distinguishable in the 20-cell sidebar | Every row elides to `Repository… (0)`; sentinel unmatchable | `dashboard-list-windowing` (steps 1-6, green end-to-end) |

## 5. Non-goals (explicit)

- Dashboard AgentList behavior — completed in #730; not revisited.
- Repository pane geometry (#726 Option 2) — separate decision, tracked by #735-adjacent discussion; this PR changes fixture labels only.
- General panel focus routing (#731), dashboard pane chrome.
- Shell lifecycle, attach/detach semantics, agent row content, preview content, footer styling.
- Provider crash recovery; route authorization rule changes (authorization remains instance-scoped).
- The edit-form `submit:` leak pinned by `issue727/edit-{agent,repository}-presentation.json` — #728 follow-up; those scenarios are not touched.
- `issue621` `list-send-fixture` label audit.
- No legacy screen restoration, no parallel rendering/routing, no new abstraction, dependency, cache, renderer, or route. No `ListBody`/protocol field additions.

## 6. Planned vertical slices

Each slice is one commit of coherent green behavior, RED first. All land in one PR per the issue's delivery note.

### Slice 1 — Provider outcome survives navigation (#739)

- Acceptance rows: A1, A2.
- Owner: state/reducer boundary (`src/state`), integration at `src/app_input/provider_dispatch.rs`.
- Allowed files: `src/app_input/provider_dispatch.rs`, `src/state/provider_requests.rs`, new `src/state/provider_outcome_navigation_tests.rs` (+ its `#[path]` wiring in `src/state/mod.rs`), `src/app_input/provider_dispatch_tests.rs` if the dismissal-ordering case belongs there.
- RED: new unit test — register a terminal `Outcome::Navigate` against instance A (dashboard), `enter_provider_route(vendor.panel.open)` to instance B, assert `provider_surface_projection(24)` still yields the `Completed("Navigate to vendor.panel.open")` row AND `latest_current_provider_request()` finds the terminal request on B (the Escape path). Fails today on the instance-scoped `find`. The three scenarios are already RED (§2).
- GREEN: D1 mechanics (remove the `DismissTerminals` commit; add and call `rebind_terminal_context`, guarded on the instance actually moving).
- Non-goals: no change to `authorize_provider_outcome`, `provider_surface_projection`, or overlay projection.
- Verify: `cargo test -p jefe provider_outcome_navigation provider_dispatch` then the three scenarios via `run-scenario-manifest.py --scenario ...`.
- Stop if: the rebind appears to need live-request or confirmation retargeting, or any consumer other than the three named gates turns out to depend on the old context pair.

### Slice 2 — Terminal Manager presentation (#737 + #743)

- Acceptance rows: A6, A7.
- Owner: host-panel model + declared-content projection (`src/host_panel_models.rs`, `src/provider_panel_view.rs`), renderer hint (`src/ui/components/provider_screen.rs`).
- Allowed files: those three, `src/host_panel_models_session_tests.rs`, `src/provider_panel_view_tests_*` (the relevant one), `src/ui/components/provider_screen_*tests*.rs`.
- RED: session test — empty `shell_inventory` projects one item labeled `No shells.` with `selected_id == None` (fails today: zero items); panel-view test — `shell_overlay_active()` PTY projection carries title `Agent Shell` (fails today: `Terminal`); render test — manager pane title row contains `F12 list | F10 close shell` when focused, agent-screen overlay keeps `F12 hide shell`.
- GREEN: D3 and D4 mechanics.
- Non-goals: shell lifecycle, preview content, pane chrome, `terminal_view.rs` changes.
- Verify: focused unit tests; then `terminal-manager`, `kennel-terminal-select`, `code-puppy-chord-passthrough`, `paste-enter-escape`, `agent-shell-overlay` scenarios.
- Stop if: the hint or title appears to require pane-ordinal or focus-routing changes (#731 territory).

### Slice 3 — Confirmation overlays (#740)

- Acceptance rows: A3, A4, A5.
- Owner: overlay control projection (`src/overlay_controls.rs`); consumers in `src/ui/orchestration.rs` and `src/state/modal_ops.rs` must stay source-untouched (they call the same functions; only row content and title change).
- Allowed files: `src/overlay_controls.rs` (+ its inline tests), `dev-docs/tmux-scenarios/issue705/semantic-continuation-macos.json`, `dev-docs/tmux-scenarios/issue705/semantic-continuation-linux.json`, and the evidence re-pin (§8). A row-composition helper may be added inside `overlay_controls.rs` (module-private), not a new module.
- RED: unit tests per D2 — focused-Cancel generic projection renders a row containing both `( Cancel )` and `[ Confirm ]`; provider projection focused Cancel carries `[ {confirm_label} ]` and title `Provider Action`; no row text contains `submit:`; checkbox `[ ]`/`[x]` forms; `confirmation_command`/EditField intents unchanged. Scenario RED: the re-pinned `semantic-continuation-*` asserts (`( Cancel )`, `( Deploy alpha )`, absent `submit:`) fail until the renderer lands in the same slice.
- GREEN: D2 mechanics.
- Non-goals: edit-form leaks, overlay geometry, confirmation keymap.
- Verify: focused unit tests; `confirm-dialog-focus`, `issue-dirty-copy-confirm`, `provider-action-confirmation`, both `semantic-continuation-*` scenarios.
- Stop if: hit-target mapping (`confirmation_hit_target_at_content_line`) needs new target kinds, or any intent consumer depends on the `Decision:` row text.

### Slice 4 — Windowing fixture labels (#726)

- Acceptance row: A8.
- Owner: scenario evidence only.
- Allowed files: `dev-docs/tmux-scenarios/dashboard-list-windowing.json`, evidence re-pin (§8).
- RED: with labels renamed and literals updated (`Repo 24`, `Repo 7`), the scenario still fails if any windowing behavior is broken — proving the assertions now bite.
- GREEN: full scenario passes with no production change.
- Non-goals: geometry, label conventions elsewhere, `issue621` fixtures.
- Verify: `dashboard-list-windowing` scenario; then all six macOS shards.
- Stop if: steps 2-6 fail for reasons other than labels (e.g. the agents window not following the restored selection) — that is #730/#731-adjacent behavior, ledger it and stop for triage rather than widening this PR.

## 7. Expected files by layer (final map)

- State/reducer: `src/app_input/provider_dispatch.rs`, `src/state/provider_requests.rs`.
- Projection: `src/host_panel_models.rs`, `src/provider_panel_view.rs`, `src/overlay_controls.rs`.
- Renderer: `src/ui/components/provider_screen.rs`. (`src/ui/components/terminal_view.rs`, `src/ui/components/host_control_overlay.rs`, `src/host_controls.rs`, `src/state/overlay_projection_ops.rs` are expected to remain untouched; they are listed because the issue names them as expected areas.)
- Unit tests: new `src/state/provider_outcome_navigation_tests.rs` (+ wiring in `src/state/mod.rs`); extensions to `src/host_panel_models_session_tests.rs`, the relevant `src/provider_panel_view_tests_*.rs`, `src/ui/components/provider_screen_*tests*.rs`, and the `overlay_controls.rs` inline test module.
- Scenarios: `dev-docs/tmux-scenarios/dashboard-list-windowing.json`, `dev-docs/tmux-scenarios/issue705/semantic-continuation-macos.json`, `dev-docs/tmux-scenarios/issue705/semantic-continuation-linux.json`, and `dev-docs/tmux-scenarios/terminal-manager.json` under the §14 item 2 fixture-label approval.
- Scenarios beyond that map, added during implementation: `dev-docs/tmux-scenarios/issue704/atomic-success.json`, `dev-docs/tmux-scenarios/issue704/restart-publication.json`, `dev-docs/tmux-scenarios/package-panel-lifecycle.json`, `dev-docs/tmux-scenarios/issue705/tree-structured-diff-linux.json`, `dev-docs/tmux-scenarios/issue705/tree-structured-diff-macos.json`, `dev-docs/tmux-scenarios/issue705/workbench-runtime-linux.json`, and `dev-docs/tmux-scenarios/issue705/workbench-runtime-macos.json`. These seven are additive A1 proof strengthening: each gains assertions that the provider Navigate outcome survives the push, and no existing step, assertion, or semantic check was removed or weakened. They carry the same evidence linkage as the mapped scenarios: their bytes are re-pinned in `dev-docs/testing/scenario-owner-evidence.json`, which the repin tool under `tmp/issue758/repin/` recomputes for every entry in its `scenarios` collection, and their step, operation, and assertion projections are regenerated in `dev-docs/testing/scenario-execution-manifest.json`. The first five are named in that tool's `CHANGED_SCENARIOS` list; the workbench-runtime pair's projections were regenerated during the §13 run, which records invoking the script with those two paths added to its input collection. Every scenario file touched in this working tree now has a manifest projection matching its current bytes, including `workbench-runtime-{linux,macos}` at 29 steps with seven frame assertions, `terminal-manager` at 84 steps, and `dashboard-list-windowing` at nine.
- Evidence: `dev-docs/testing/scenario-owner-evidence.json` (per-scenario sha256 from final bytes; `scenario_manifest_sha256` recomputed) — the shape #757's merge used.

## 8. Verification and evidence commands

Iteration (`cargo xtask quick` = fmt + check + test):

```
cargo xtask quick
cargo test --workspace --all-features --locked
```

Complete required gates before any green checkpoint push (repo standards; no Makefile exists in this repo — `cargo xtask ci` is the aggregate):

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
CLIPPY_CONF_DIR=$PWD/.github/clippy cargo clippy --workspace --all-targets --all-features \
  -A clippy::all -A clippy::pedantic -A clippy::nursery \
  -D clippy::cognitive_complexity -D clippy::too_many_lines -D clippy::too_many_arguments \
  -D clippy::type_complexity -D clippy::struct_excessive_bools
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo xtask ci
```

Single-scenario iteration and the six macOS shards (exact CI invocation, `.github/workflows/ci.yml:193-230`):

```
cargo build --locked --all-features --bin jefe --bin jefe-harness-probe \
  --bin jefe-capture-shim --bin jefe-jsp-llxprt-fixture --bin tmux_scenario
python3 scripts/run-scenario-manifest.py --platform macos \
  --shard-index N --shard-count 6 \
  --tmux-scenario target/debug/tmux_scenario --jefe target/debug/jefe \
  --probe target/debug/jefe-harness-probe \
  --jsp-fixture target/debug/jefe-jsp-llxprt-fixture \
  --shim target/debug/jefe-capture-shim \
  --reports target/tmux-scenarios/macos-N
# single scenario during RED/GREEN loops:
python3 scripts/run-scenario-manifest.py --platform macos --scenario dev-docs/tmux-scenarios/<name>.json ...
```

Hash re-pinning from final bytes (never hand-edited digests): after the last scenario-byte change, `shasum -a 256 <scenario-file>` updates the matching `scenarios[].sha256` entries in `dev-docs/testing/scenario-owner-evidence.json`, and `scenario_manifest_sha256` is recomputed from the manifest bytes (`shasum -a 256 dev-docs/testing/scenario-execution-manifest.json`; the runner prints the same value in its report metadata). `git diff --check` before every commit. The Linux shards (these scenarios are cross-platform in the manifest) and native Windows/coverage gates are proven on the exact PR head in CI, as in #757.

## 9. Scope ledger

Empty at plan time. Template dispositions: Blocker-Fix / In-scope-Fix / Reject / Defer, per the workflow doc.

| Entry | Origin | Disposition |
|---|---|---|
| Selected repository ID can remain outside the retained shared List projection when restored selection and scroll offset disagree; reveal the selected List target in the existing visible row window without changing its count or geometry. | Slice 4 shortened labels made `dashboard-list-windowing` expose `Repo 0`..`Repo 18` while repository 24 and Agent 24 were selected. | In-scope-Fix |
| Reconcile the selected-window implementation with D5, Slice 4's fixture-only boundary, and its stop condition. The implementation and prior nine-step scenario pass are recorded; explicit approval is not present in the inspected plan or Slice 4B audit. | Final verification, 2026-09-07. | Unresolved authorization record; no approval inferred |
| Preserve scrolling to the disabled embedded control in a wrapped selected List item. The existing `wrapped_and_scrolled_controls_retain_structural_hit_targets` test fails at `src/provider_panel_view_tests_interaction.rs:282` after the selected-window change. | Full workspace test gate, `tmp/issue758/verification-current/test.log`. | Blocker-Fix. Closed during the §14 approved correction: the wrapped-control assertion was left unchanged and the selected-window reveal was adjusted to satisfy it. The §15 gate table records the locked all-feature workspace test passing with 7,784 tests and zero failures. |
| Resolve strict Clippy and source-size failures without lowering their limits: `session_list` 63/60 lines, title assignment, nested selection condition, and `provider_dispatch_tests.rs` 1034/1000 lines. | Final Clippy, complexity, and aggregate CI logs under `tmp/issue758/verification-current/`. | Blocker-Fix. Closed during the §14 and §15 approved corrections: `session_list` shrank by extracting the `empty_session_item` helper, `provider_dispatch_tests.rs` fell under the source-size limit by moving its navigation cases into `src/app_input/provider_dispatch_navigation_tests.rs`, and the `assigning_clones` and `collapsible_if` sites were rewritten as a let-chain and a direct assignment. No Clippy level, complexity threshold, or source-size limit was lowered: `.github/clippy/clippy.toml` and the 1000-line source-size limit are unchanged. The §15 gate table records format, strict Clippy, namespaced complexity Clippy, source-size, locked build/test, and `cargo xtask ci` all passing. |

Pre-identified candidates that must NOT ride along: edit-form `submit:` leak (`issue727` pins), `SpanRole`/`HostControlSpanRole` consolidation (ledgered in #730), copy-geometry unification (#730), `issue621` fixture audit, repository pane width.

## 10. Review counters and bounded review

- Open Code Review (zai profile): at most two runs before the PR opens, at most two after; spend only on the fully verified head.
- Review cycles follow the standing two-cycle rule: one full review, one findings-only follow-up verifying the original scope and initial findings, no widening.
- Every finding gets one disposition (§9 template); reviewer suggestions are not scope authorization.

## 11. Protected state and workspace constraints

- Worktree: branch `issue758` at `aa897813`; only `.llxprt/LLXPRT.md` is modified (pre-existing, version-controlled project memory — left untouched per standing instruction). Its sha256 at plan time: `f712056ed5313298d6c1e341427c8f77ecf42572580f8d80941cdecc50fc0b09`; it must read the same when the PR opens.
- `target/debug/jefe` PID 24067 is a live instance on this machine; do not kill it, and do not run `cargo` while planning (this plan ran none). Implementation slices coordinate builds around it.
- `git diff --check` is clean at plan time and must stay clean.
- This planning session edited only `project-plans/issue758-plan.md`.

Protected-state reconciliation, 2026-09-08. PID 24067 was recorded live on 2026-09-06. On 2026-09-08 `ps -p 24067` returns no process, and `sysctl -n kern.boottime` reports `Tue Sep  8 01:47:31 2026`, so the machine rebooted between the two observations. No task in this effort signaled, killed, or restarted that process, and none rebuilt or replaced the binary; §13 records that an orphaned isolated scenario process was stopped through its own verified process group while the protected PID was not signaled. The later sections track two protected hashes and only the first appears here. The first is `.llxprt/LLXPRT.md` at `f712056ed5313298d6c1e341427c8f77ecf42572580f8d80941cdecc50fc0b09`, pinned above. The second is the root `target/debug/jefe` binary at `b33949825eb045fbfa2c2b4e1193e5f539a0ea620b5c79f0a020fe4e4db51aeb`, first recorded in the later verification sections rather than in this one; every Cargo invocation in this effort used `CARGO_TARGET_DIR=tmp/issue758/target`, so the root build directory was never written. Read the statements in §15 and after that "both protected file hashes match Section 11" as: the `.llxprt/LLXPRT.md` hash matches this section, and the `target/debug/jefe` hash matches the later sections that introduced it.

## 12. Stopping rules (issue-specific)

- Any slice needing a new subsystem, public abstraction, protocol/wire field, dependency, or renderer → stop and report.
- Slice 4 discovering non-label windowing failures → stop and triage against #730/#731 rather than widening.
- The rebind touching consumers beyond the three named gates → stop.
- An interrupted, skipped, or stale-SHA verification is incomplete, not passed; re-run on the candidate head.

## 13. Final verification record, 2026-09-07

This task regenerated evidence and ran serial verification on the uncommitted `issue758` working tree at `aa8978135619f988952dd3c5a004180a743dc499`. It did not implement fixes, change scenario semantics, commit, push, open a PR, or consume a review cycle. The earlier planning-only workspace statements in Section 11 describe the original snapshot, not the implementation now present.

All retained evidence is under `tmp/issue758/verification-current/`. `commands.jsonl` records exact commands, exit statuses, manifest and source fingerprints, and test totals. `REPORT.md` indexes the results and remaining blockers. Every Cargo/build/scenario command used `CARGO_TARGET_DIR=tmp/issue758/target`. The aggregate subprocess target-directory inspection is recorded in `xtask-target-inspection.md`.

### Evidence regeneration

The existing repin script was invoked with the two workbench-runtime paths added to its input collection. All four evidence files were regenerated from current bytes. Both workbench-runtime projections now have 29 steps and seven frame assertions. Independent verification passed for 405 pins, including 403 raw-byte file pins plus the manifest and artifact-set pins. A second regeneration reported zero updates and identical evidence bytes. All 180 scenario files were byte-preserved by this task.

The current manifest SHA256 is `e3d6b5de5c0e3782d5e1e92f1311da7b242211dd257c75b64c3d4fe68ac6691f`. All completed shard attempts used this manifest. Successful completion markers for shards 1, 2, and 4 carry this exact digest.

### Required gates

| Gate | Result |
| --- | --- |
| Format check | Passed, exit 0 |
| Strict Clippy | Failed, exit 101: `session_list` 63/60 lines at `src/host_panel_models.rs:558`; `assigning_clones` at `src/overlay_controls.rs:288`; `collapsible_if` at `src/provider_panel_view.rs:737` |
| Complexity Clippy | Failed, exit 101: `session_list` 63/60 lines |
| Locked all-feature workspace build | Passed, exit 0 |
| Locked all-feature workspace test | Failed, exit 101: 5069 passed, one failed in the library target; remaining workspace targets were not reached |
| `cargo xtask ci` | Failed, exit 1 at source-size: `src/app_input/provider_dispatch_tests.rs` has 1034 lines against a 1000-line limit; later aggregate steps, including coverage, were not reached |
| `scenario_manifest` | Passed, 11 tests |
| `issue704_owner_evidence` | Passed, six tests |
| `issue705_owner_evidence` | Passed, 17 tests |
| Exact scenario-authority build | Passed, exit 0 |
| Six-shard completion gate | Failed; complete passing evidence for all six shards is absent |

The failed library test is `provider_panel_view::tests::wrapped_and_scrolled_controls_retain_structural_hit_targets`, at `src/provider_panel_view_tests_interaction.rs:282`. Its scrolled viewport no longer contains the expected unavailable embedded control. The selected-window correction is implemented, but the full test suite does not establish it as regression-free. No test or assertion was weakened here.

### Native macOS shard results

Counts below mean reports matching the manifest contract, including expected negative scenarios. They are not a count of reports whose raw status says `passed`.

| Shard | First attempt | One full-shard retry | Final state |
| --- | --- | --- | --- |
| 0 | Interrupted by SIGTERM; 21/26 matching reports, one mismatch, four reports missing | Exit 1; 25/26 matched | Failed: `terminal-scrollback`, step index 17, missing `Agent Runtime    [core.code-puppy]`; same mismatch is present before the first interruption |
| 1 | Interrupted by SIGTERM; 2/25 matching reports | Exit 0; 25/25 matched | Passed on retry, including workbench-runtime 29/29 |
| 2 | Exit 1; 17/25 matched | Exit 0; 25/25 matched | Passed on retry; initial startup and handshake failures retained |
| 3 | Exit 1; 21/25 matched | Exit 1; 23/25 matched | Failed: `dashboard-reorder` and `issue382/agent-unsupported-ui` |
| 4 | Exit 0; 25/25 matched | Not needed | Passed |
| 5 | Exit 1; 22/25 matched | Exit 1; 22/25 matched | Failed: `issue265-linux-keys`, `settings-keys-normal`, and `terminal-manager` |

First attempts remain in `shards/macos-N`; retries remain in `retries/macos-N`. No completion marker was manufactured for a failed or interrupted shard. There are 151 required macOS scenarios in the six-shard inventory. The latest full attempts match 145/151 contracts, including all six expected negative scenarios. Passing retries alone do not establish that initial failures were pre-existing or unrelated to this change. No third attempt was made.

Two foreground shell invocations received SIGTERM despite requesting a 7200-second timeout. The cause is not established. The orphaned isolated scenario process from the shard 1 interruption was stopped by its verified process group; the protected PID was not signaled. Remaining work used a managed background command with serial child execution. `interruptions.json` records the events.

### Acceptance and follow-ups

Provider Navigate outcome scenarios pass in the current full attempts, including atomic-success, restart-publication, package-panel-lifecycle, tree-structured-diff, and workbench-runtime. Confirmation scenarios, the standalone shell-overlay scenario, and the nine-step dashboard-list-windowing scenario also pass. Terminal Manager acceptance remains incomplete because `terminal-manager` repeatedly stops at step index 39 waiting for `Beta Repository`, before its later assertions. Linux evidence remains schema acceptance from the navigation audit, not native Linux execution. Native Linux, Windows, coverage, and exact committed-head CI are not proven by this task.

The coordinator must resolve the documented windowing authorization record and triage the lint, source-size, wrapped-control scrolling, and six remaining scenario failures. The initial shard failures remain evidence even where a retry passed. Required gates must be regenerated after any repair. This verification task did not make those repairs or infer approval.

## 14. Approved bounded correction, 2026-09-08

Andrew answered “yeah go.” to the explicit request for two repairs. [Approval recorded on issue758](https://github.com/vybestack/llxprt-jefe/issues/758#issuecomment-5579254743).

1. D5/A8 and Slice 4 may include selected List-row reveal on restored or changed selection, without overriding explicit manual scrolling. Preserve structural hit targets, clipboard/render agreement and resolver-owned geometry. The existing wrapped-control regression assertion stays unchanged. Add behavioral tests for offscreen restored selection, selection changes from a nonzero manual origin and persistent manual scrolling. This supersedes the unresolved authorization entry in Section 9 for this bounded correction only.
2. A6/A7 may shorten Terminal Manager repository fixture labels consistently in setup, typing and assertions, with no step, geometry or semantic-check removal. The retained index-39 truncation report supplies RED; native macOS execution must supply GREEN.

GREEN for item 2, recorded 2026-09-08. The mandated native macOS run is satisfied by the six-shard verification retained under `tmp/issue758/shards-post-fixes/`. `dev-docs/tmux-scenarios/terminal-manager.json` ran on shard 5 and passed 84/84 steps on both full attempts, each with `report_contract_matches: true`, `reported_steps: 84`, and app exit code 0. The per-attempt reports are `attempts/macos-5-attempt-1/dev-docs__tmux-scenarios__terminal-manager.json` and `attempts/macos-5-attempt-2/dev-docs__tmux-scenarios__terminal-manager.json`, and `acceptance.json` carries the same record under both `#737 Terminal Manager empty state` and `#743 attached shell presentation`. The scenario no longer stops at index 39, and its step count is unchanged from the RED run, so the repair came from the fixture labels rather than from removing steps or assertions.

The correction may touch the existing panel projection, per-instance host/provider presentation and input ownership, their focused tests, the Terminal Manager fixture and dependent evidence pins. No new public/protocol abstraction, cache, alternate renderer or routing path is authorized. #741, #744, #746, dashboard-reorder and terminal-scrollback remain excluded, as do lifecycle redesign, general focus, AgentList behavior and edit forms. This approval does not authorize repair of every macOS failure.

Verification and incremental artifacts for this task belong in `tmp/issue758/approved-window-fix/`. All Cargo/build/scenario work runs serially with `CARGO_TARGET_DIR=tmp/issue758/target`. No commit, push, PR or review is part of this task; review counters do not advance.

## 15. D2 confirmation mouse correction and verification, 2026-09-08

The task explicitly authorized repairing confirmation mouse actions within A3/A4/A5. The earlier D2 choice row retained the decision Field but removed the only displayed Submit target. The two existing mouse tests reproduced the missing `confirm.accept` target unchanged. A new native mouse scenario also reproduced the behavioral defect: clicking visible Cancel from Confirm focus only cycled focus and left the modal open.

The correction adds crate-private cell ranges to the existing `HostControlRow`, populated by the existing confirmation projection. Cancel and Confirm occupy their displayed cells; the two-space separator, padding, borders, clipped ellipsis, and removed submit row have no action. Orchestration passes the content column into that shared lookup. Generic mouse input resolves Cancel, Submit, and the optional checkbox through the existing action registry. Only mouse Confirm selects Confirm focus before the existing accept handler; keyboard Enter still activates the current choice. The typed `FormBody`, decision Field metadata, checkbox editing, provider title/custom labels, and continuation semantics remain intact. No renderer, target enum variant, public abstraction, protocol, dependency, edit-form behavior, or legacy route was added.

This supersedes Slice 3's original expectation that orchestration would remain untouched: column-aware lookup is required at that existing projection/input boundary. In addition to `overlay_controls.rs` and its tests, the bounded correction touches `host_controls.rs`, `ui/orchestration.rs`, `mouse_action_routing.rs`, `mouse_action_routing_tests.rs`, and the mouse-only dispatch arm of `app_shell_key_routing.rs`. It is a Blocker-Fix within D2, not an extension into provider end-to-end mouse routing or general panel input.

The original mouse tests retain their action, handler, geometry, and non-action assertions. Their obsolete mouse-cycle expectation now requires direct Cancel. The restored row has two visible choices, not a third visible cycle surface; making the separator clickable or retaining an invisible submit row would defeat the contract. Added coverage checks both focus values with and without the checkbox, real cancellation and confirmation, checkbox on/off, inert gaps and borders, clipped cells, Unicode provider labels, and unchanged typed commands. All checked scenario bytes are preserved. The new native mouse scenario is retained as task evidence under `tmp/issue758/confirmation-mouse-fix/`, not added to the checked execution manifest.

Final executable-source verification ran serially with `CARGO_TARGET_DIR=tmp/issue758/target`; before/after source inventories match. Format, normal-config strict Clippy, separate namespaced complexity Clippy, source-size, locked all-feature workspace build/test, and `cargo xtask ci` pass. The standalone workspace test reports 7,784 passed, zero failed, zero ignored. Aggregate CI completed all ten steps, including 74.51% line coverage against the 30% floor. Focused overlay, confirmation, UI, mouse, provider-input, selected-window, clipboard, and owner-evidence tests pass. Initial strict/aggregate failures in two new test stack arrays were repaired with reference arrays without changing their assertions; those first-run logs remain retained.

One later standalone `scenario_manifest` attempt failed its existing timeout-fixture marker assertion, despite passing in full workspace and aggregate runs. One unchanged retry passes all 11 tests. The failed attempt is retained; a startup-versus-500ms-timeout race is a hypothesis, not an established cause. No harness, test, or timeout policy was changed to hide it.

Native macOS passes: `confirm-dialog-focus` 22/22, `issue-dirty-copy-confirm` 42/42, `provider-action-confirmation` 13/13, `issue705/semantic-continuation-macos` 23/23, additional `origin-mismatch-confirm` 20/20, and the new mouse regression 23/23. These are selected scenario runs, not six-shard completion. Provider label targets have projection coverage; the provider native scenarios prove keyboard/continuation behavior, not new provider mouse support.

Evidence regeneration updated six issue705 pins from actual bytes. Independent verification passes all 405 pins. Subsequent regenerations update zero pins and are byte-idempotent; all 180 checked scenario files are unchanged by this task. The manifest digest remains `e3d6b5de5c0e3782d5e1e92f1311da7b242211dd257c75b64c3d4fe68ac6691f`. Exact commands, statuses, source inventories, reports, and retained failures are indexed by `tmp/issue758/confirmation-mouse-fix/REPORT.md` and `verification-summary.json`; final-run logs are in its `final/` directory.

The branch remains `issue758` at `aa8978135619f988952dd3c5a004180a743dc499`, with no staged paths, commit, push, PR, or review. Both protected file hashes match Section 11 and the continuation report. PID 24067 remains absent and was never signaled or restarted. Broader historical macOS shard failures, native Linux/Windows execution, exact committed-head remote CI, and final review remain outside this completed repair verification. No readiness claim for the whole issue is made.

## 16. Review record, 2026-09-08

One full Open Code Review ran against the uncommitted `issue758` working tree at `aa8978135619f988952dd3c5a004180a743dc499`. Provider `zai-anthropic`, model `glm-5.3`, 48 selected items, 14 findings. Session `511d6e90-f1f9-4f60-9207-c0e66e3b27d2`; raw output, findings, and the model identity are retained under `tmp/issue758/final-review-zai3/`. This consumes one of the two pre-PR review runs allowed by §10.

Dispositions, one per finding:

| Finding | Subject | Disposition |
|---|---|---|
| F0 | Wrap-boundary coupling in `assert-frame` contains entries across seven scenarios; a one-cell layout change moves the greedy wrap and fails the assert with no behavior change. | Defer to a follow-up issue. The assertions pass on native macOS today and the remedy touches scenario authoring across four issue families, which is separate work. |
| F1 | `PanelHitTarget::Field(_)` in `src/mouse_action_routing.rs:71` drops field-id discrimination and relies on two non-local invariants. | Reject. The hazard is unreachable in the current tree: `project_confirmation` emits exactly one `Field` row, and `confirmation_choice_row` always sets non-empty `cell_targets`, which take precedence in `HostControlRow::hit_target_at`. The proposed remedy adds a new public domain helper, which §14 does not authorize. |
| F2 | `src/mouse_action_routing_tests.rs` pins absolute screen geometry on a zero-margin boundary. | In-scope-Fix, applied. Both tests now anchor on the render through the existing `projected_action_point` helper. |
| F3 | The delete-work-dir checkbox row keeps a whole-row target, so its clipped ellipsis cell still toggles the destructive flag on narrow terminals. | Defer to a follow-up issue. Reachable only below roughly 25 content cells, and the fix belongs with the checkbox row's own cell-target model rather than this review pass. |
| F4 | `trim_end_matches('…')` strips every trailing ellipsis while truncation appends exactly one. | In-scope-Fix, applied. `strip_suffix` now removes exactly one, matching the model in `src/overlay_controls_tests.rs:410`. |
| F5 | `confirmation_hit_target_at_content_line` names a line while its contract is per-column. | In-scope-Fix, applied. Renamed to `confirmation_hit_target_at_content_cell`, with the single caller and the issue705 symbol pin updated. |
| F6 | `host_list_window_bounds` re-projects the whole screen per scroll event to read two values. | Defer to a follow-up issue. It is a cost concern, not a correctness one, and the suggested remedy changes the `scroll_host_panel` signature and its callers. |
| F7 | `changed` is computed against the projected origin while the stored offset is what gets written, so state can mutate under a `false` return. | Defer to a follow-up issue. It needs its own RED test across the host scroll path and touches shared host input ownership. |
| F8 | `apply_snapshot` drops a retained manual viewport on every reactivation, because `prior_selection` is always `None` after `start_activation` clears the accepted model. | Blocker-Fix, applied. The post-assignment selection is now compared against the retained `manual_scroll_selection` pin, which is behavior-identical on the continuous path. A RED-first regression test in `src/provider_panel_view_tests_windowing.rs` covers suspend/resume and retry republication plus a changed selection still revealing. |
| F9 | `src/state/provider_panels_ops.rs:384` is 111 columns, over rustfmt's 100-column default. | Blocker-Fix, applied. The signature is wrapped and `cargo fmt --all --check` passes. |
| F10 | §9's Blocker-Fix rows never receive a closing disposition. | In-scope-Fix, applied. Both rows now record which approved task repaired them, that no limit was lowered, and where the passing gate is recorded. |
| F11 | §7's final map understates the scenario files edited, and §2 carries a blanket no-scenario-change claim. | In-scope-Fix, applied. §7 enumerates the seven additional files with their A1 justification and evidence linkage, and §2 is qualified. |
| F12 | The protected-state record is unreconciled between §11, §13, and §15. | In-scope-Fix, applied. §11 records the reboot at `Tue Sep  8 01:47:31 2026`, that PID 24067 was never signaled by this work, and names the second protected hash referent. |
| F13 | §14.2 makes native macOS execution the GREEN condition for the Terminal Manager labels, and no section supplies it. | In-scope-Fix, applied. §14 records the 84/84 passes on both full attempts under `tmp/issue758/shards-post-fixes/`. |

Totals: 2 Blocker-Fix, 7 In-scope-Fix (three code changes in F2, F4, F5 and four plan records in F10 through F13), 4 Defer, 1 Reject.

The four deferred findings and the rejected one are valid observations that do not belong to this issue's authorized scope; they go to a follow-up issue rather than widening this branch. No reviewer suggestion was treated as scope authorization.

Verification after these fixes ran serially with `CARGO_TARGET_DIR=tmp/issue758/target`. Logs are under `tmp/issue758/review-fixes/`.
