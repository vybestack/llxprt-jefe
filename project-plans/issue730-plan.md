# Issue #730 Plan: Restore the dashboard Agents row (glyph, color, badge, git suffix, grab marker, selection identity)

- Issue: vybestack/llxprt-jefe#730 (open, acoliver, one comment 2026-09-04)
- Branch: `issue730` at exactly `5febc492` (`5febc492c09050483e800a7afa61dd53bc1ab517`, current `origin/main` tip "Refresh baked commit after fast-forward pulls (#753) (#756)")
- Plan written: 2026-09-04, research only, no production/test/scenario edits
- Evidence base: `tmp/issue731-agentlist/` (FINDINGS.md, matched-pair frames, manifest runs, old-era source extracts) plus fresh source reads at `5febc492`

## 1. What was lost, and where it lives now

`#715` (`f5826508`, 2026-08-30, "Route Dashboard through the shared declared screen runtime") deleted `src/ui/screens/dashboard.rs` and moved the Agents pane to the shared host-control list. The shared list row is `marker + label + " [status]"` and nothing else:

- `src/host_panel_models.rs:270` (`agent_list`) builds `ListItem { label: agent_display_name(..), status: Some(format!("{:?}", agent.status)), description: None, count: None }` (`:270-312`).
- `src/host_controls.rs:458` (`project_list`) and `:534`/`:555` (`push_list_item_row`, `compose_list_item_row`) compose `marker + label + " (count)" + " [status]"`; the marker is `">> "` or `"   "` (`:469-475`).
- `src/ui/components/provider_screen.rs:311` (`render_panel`) paints every line one color: `Text(content: line, color: rc.fg)` (`:339-343`).

Every datum that predates the cutover and is not in that row was dropped in that one commit. Attribution (verified with `git log` at `5febc492`):

| Datum | Introduced | Dropped by |
|---|---|---|
| Status glyph `* + x ! ? - o` | `e7050bcb` initial stack; `~` unconfirmed arm `67511917` 2026-08-02 (#541/#595) | `f5826508` |
| Per-status glyph color (`SpanRole`) | same commits | `f5826508` |
| `[N]` shortcut badge | `db72e1f7` 2026-02-27 (v0.0.16 agent shortcuts) | `f5826508` |
| `origin @ branch` suffix and ` *` dirty marker | `27a26819` 2026-07-10 (#170/#177) | `f5826508` |
| `↕` grab marker | `59715e3a` 2026-07-05 (#118/#125) | `f5826508` |
| Selection highlight + copy identity (`SelectablePane::AgentList`) | pre-cutover `agent_list.rs:200` | `f5826508` |

Nothing lost was added after the cutover plan was written. The issue's one comment (2026-09-04) dogfoods main `76478739` and confirms the row is still the simplified form; it directs keeping the full scope open until all of it is restored.

## 2. The data is all still there (line-level, at `5febc492`)

- `status_icon` / `status_role` / `agent_prefix` / `to_selectable_row` / `agent_list_props`: `src/ui/components/agent_list.rs:56, 75, 90, 105, 164`. The module's only production caller today is the text-selection copy path (`src/selection/dashboard_content.rs:15`, `agent_list_lines`), which is why copied text can disagree with rendered text. The render path does not call it.
- `agent.shortcut_slot` still restores and still drives `Alt+1..9`; only the badge is gone.
- `GitRepoInfo::list_suffix` (`src/git_info/mod.rs:123-140`) yields `"{origin} @ {branch}{ *}"` with the marker only when `branch.is_some() && dirty == Some(true)`.
- `resolve_dashboard_git_info` (`src/dashboard_git_info.rs:38`) resolves one `DashboardGitInfoSnapshot { agents: Vec<GitRepoInfo>, preview }` for the selected repository's visible agents. Today it feeds only the selection/clipboard path (`src/mouse_routing.rs:584-591` binds it into `state.selection_dashboard_git_info` at drag start; `src/pane_content_projection.rs:20-27` consumes it).
- `GitRepoInfo::resolve` is cached by a process-global TTL store (`src/git_info/mod.rs:41-58`: branch/dirty 5 s, origin 5 min, 3 s probe timeout), so one resolve per frame does not multiply git subprocesses and shares its cache with the selection path.
- Grab state lives in `AppState.dashboard_grab: Option<DashboardGrabPane>` (`src/state/navigation.rs:53`) with `Agent { local_index, .. }`; grab/reorder input still works (`src/state/dashboard_grab_ops.rs:54-160`, `dev-docs/tmux-scenarios/dashboard-reorder.json`).
- Pane identity already resolves at the geometry layer: `panel_to_selectable("agents") -> SelectablePane::AgentList` (`src/selection/resolved_panes.rs:25`), and `row_highlight_range` (`src/selection/text.rs:284`) is the shared highlight math the old components used.

The gap is supply and shape, not data: nothing carries a role through the shared projection, and nothing hands the resolved git snapshot to the render path.

## 3. Decisions (settled here, before implementation)

### D1. Typed span/role shape: three typed host-only fields on `ListItem`, one row-span type on `HostControlRow`

The row grammar after restoration is `marker + glyph(role) + " " + badge + label + suffix(dim)`. Rejected alternatives first:

- Generic `Vec<Span>` on `ListItem`: makes the shared control re-learn span-level fitting and leaves `count`/`status` protection (#745) interacting with free-form spans. More mechanism than the row needs.
- Folding everything into `label` and carrying color offsets: offsets break the moment the label is elided; the #745 `count` doc (`src/runtime/provider/panel_model.rs:203-211`) already records why content and presentation must stay separate fields.
- Reusing `ui/components/selectable_list.rs::SpanRole` directly: wrong DAG direction. `host_controls` cannot import `ui/components` (`src/ui/components/host_control_overlay.rs:5` already imports `crate::host_controls`); moving `SpanRole` down would churn the issue/PR/actions lists this issue does not touch. Consolidation of the two role enums is recorded as a follow-up, not done here.

Accepted shape, following the `count` precedent (additive, host-only, absent from the provider wire):

1. `src/runtime/provider/panel_model.rs`, next to `ListItem`:
   - `pub struct ListItemGlyph { pub text: String, pub role: ListItemGlyphRole }`
   - `pub enum ListItemGlyphRole { Bright, Dim, Red, Yellow, Blue }` (the exact arms `status_role` returns, `agent_list.rs:75-86`)
   - `ListItem` gains `pub glyph: Option<ListItemGlyph>`, `pub badge: Option<String>` (rendered `[{badge}] ` between glyph and label), `pub suffix: Option<String>` (rendered dim). Each documented "Host projections only. The provider wire has no such key", exactly like `count`. The wire DTO conversion layer never sees them, so provider snapshots are unaffected.
2. `src/host_controls.rs`, next to `HostControlRow` (`:159`):
   - `pub(crate) struct HostControlSpan { pub(crate) text: String, pub(crate) role: HostControlSpanRole }`
   - `pub(crate) enum HostControlSpanRole { Themed, Bright, Dim, Red, Yellow, Blue }`
   - `HostControlRow` gains `pub(crate) spans: Vec<HostControlSpan>` (empty = one uniform themed segment, today's behavior) plus a `spanned(spans, target)` constructor; `text` stays the authoritative concatenation for width math, hit targets, and copy.
3. Role resolution lives once on the render side: `Bright -> rc.bright`, `Dim -> rc.dim`, `Red/Yellow/Blue -> iocraft named colors`, `Themed -> rc.fg`. This is byte-for-byte the mapping `selectable_list::resolve_role` has used since the pre-cutover row (`src/ui/components/selectable_list.rs:184-191`), so the restored colors match the pre-cutover colors by construction.

### D2. Role survives truncation by extending the #745 rung system, not by spanning a fitter

`compose_list_item_row` (`src/host_controls.rs:555`) already composes first-fit rungs and drops whole suffixes rather than slicing them. The change: it returns spans instead of a bare string, the glyph joins the protected prefix beside the marker, and the droppable set grows by one rightmost entry:

| Rung | Form |
|---|---|
| 1 | `marker` `glyph` `badge` label fitted `" (count)"` `" [status]"` suffix (full) |
| 2 | drop suffix (rightmost whole suffix first) |
| 3 | drop status (as today) |
| 4 | drop count (as today) |
| 5 | bare count, then bare status (as today) |
| 6 | `marker` `glyph` `badge` label fitted, no trailing suffixes |
| 7 | label alone, full width |

The marker, glyph, count and status are never sliced (half a glyph or half a count states something false, the #745 argument); the label remains the only elidable span; the badge drops whole at rung 6 (left of the label, so after the trailing suffixes but before the label is sacrificed). Rows without glyph/badge/suffix (repositories, STATUS buckets, provider lists, sessions) traverse exactly today's rung sequence and produce byte-identical text, which keeps every existing list test green by construction. Width arithmetic stays unicode-width aware (`UnicodeWidthStr`, as `labelled_row` does today at `:582`).

### D3. Grab marker rides projection input as host-local interaction state

The marker is the control's concern and grab is interaction state, so `project_list`'s input gains `marked_id: Option<Id>`: `marked` paints `"↕ "` and takes precedence over selected (`">> "`), then `"   "`: the same precedence `agent_prefix` had (`agent_list.rs:90-98`). Plumbing: `HostPanelModel` gains `grabbed_id: Option<Id>` (agent_list fills it from `state.dashboard_grab`; every other source leaves `None`), `project_host_model` passes it through `ModelProjectionInput` into `ProjectionInput`. No control factory learns the word "dashboard".

### D4. Carry-through: spans ride `PanelProjection` parallel to `lines`

`ProjectedRow` (`src/provider_panel_view.rs:488`) gains `spans: Vec<HostControlSpan>`; `project_model_rows` (`:520`) passes them through; `PanelProjection` (`:41`) gains `pub spans: Vec<Vec<HostControlSpan>>` aligned with `lines` (empty inner = uniform). Every other producer of `lines` (filter band, shell preview, unavailable panels, provider panels) leaves it empty and `render_panel` (`provider_screen.rs:311`) renders exactly today's single `Text(color: rc.fg)` for them. Only span-carrying rows take the segmented path: a one-row `Box` of per-span `Text` elements. `PanelProjection` keeps deriving `PartialEq/Eq`; the two constructors in tests (`provider_screen.rs:601`) gain the empty default.

### D5. Git supply: the render boundary resolves one snapshot and hands it in; the state layer stays probe-free

`project_host_panel` gains a parameter: `project_host_panel(state, source, git: Option<&DashboardGitInfoSnapshot>)`.

- Render path: `project_declared_content` (`src/provider_panel_view.rs:221`) resolves `resolve_dashboard_git_info(state)` exactly once, only when the panel's model source is `AgentList`, and passes `Some(&snapshot)`. This mirrors the documented boundary pattern of `pane_content_projection.rs` ("Resolve boundary-owned display data, then run the pure pane projection") and restores what pre-cutover `orchestration.rs` did per frame (one resolve per frame; `src/ui/orchestration.rs` has zero git references today).
- Input path: `state/host_panel_input_ops.rs:70,127` passes `None`. The reducer path uses the model for ids, selection and scroll only (verified: `apply_host_panel_action` / `scroll_host_panel_kind` never read row text), and the architecture table forbids I/O in the state layer. Note: `agent_preview` already calls `resolve_preview_git_info` inline (`src/host_panel_models.rs:394`, a #733 decision); this plan does not widen that wart and does not re-litigate it.
- Duplicate probing: none. `GitRepoInfo::resolve`'s process-global TTL cache is the one probe authority; render, drag-start bind and copy all read the same cache (branch 5 s, origin 5 min). No new probe site is added anywhere.
- `agent_preview` (Preview pane) is not touched. #733 already moved the preview render to `resolve_preview_git_info`; `from_configured_origin` remains only where no branch can exist.

The model then fills, per visible agent: `glyph: Some ListItemGlyph { text: status_icon(agent), role: status_role(agent) }` (with the unconfirmed-running arm from `state_is_unconfirmed`, `src/domain/mod.rs:851-856`), `badge: shortcut_slot.map(|slot| slot.to_string())`, `label: agent_display_name(agent)` unchanged, `status: None` (the recorded issue decision drops the redundant textual `[Running]`; the workbench card keeps its own glyph-plus-word convention and is out of scope), `suffix: git.map(|info| format!("  {}", info.list_suffix()))` when non-empty. The two-space join and dim role match `to_selectable_row` (`agent_list.rs:112-140`). The glyph/role mapping functions move from `agent_list.rs` into `src/host_panel_models.rs` (pure, iocraft-free, next to `agent_display_name` at `:110`) so the model layer owns them; `agent_list.rs` then dies (see D7).

### D6. Selection identity: one projection feeds render, highlight and copy

- Highlight: `render_panel` maps `panel_to_selectable(panel.id)`; for `AgentList` panels only, each line index goes through `row_highlight_range(state.selection, i)`; a covered range paints that row's covered columns with `rc.sel_bg` background / `rc.sel_fg` foreground, split exactly the way `scrollable_text::split_for_highlight` splits (`src/ui/components/scrollable_text.rs:15`), which is the old components' inverse-video contract (`selectable_list.rs:205-212`). Scoped to `AgentList` for this issue; other panes keep current behavior (generalization is a follow-up, see ledger).
- Copy: `agent_list_lines` (`src/selection/dashboard_content.rs:15`) is re-pointed from the dead renderer to the shipped one: `project_host_panel(state, AgentList, git)` + `project_model_rows` (made `pub(crate)`) + a shared `pub(crate)` visible-window helper extracted from `project_host_model`'s `clip_to_content` use, applied at the copy path's existing derived pane height and `state.agent_scroll_offset`. After this, copied text equals rendered row text in every state, which is the honest content of "row identity for text selection". The copy path keeps its own width/height derivation (unifying copy geometry with the resolved layout snapshot is a separate boundary refactor, ledgered below).
- Then `src/ui/components/agent_list.rs` has no production caller and is deleted: `agent_list_props`, `AgentListView`, `AgentListSelection`, `AgentListWindow`, and the `ui::components` re-exports (`src/ui/components/mod.rs:122`). `selectable_list` itself stays untouched; issue/PR/actions lists still use it.

### D7. The 13 dead `agent_list_props` tests: name-by-name disposition

The issue's "13" counts distinct test names: 11 names appear in both harness copies (`src/ui/components/selectable_list_tests.rs` and `selectable_list_main_tests.rs`, 19 functions total) plus 2 names in `agent_list.rs` itself. Dispositions:

| # | Test name (files, current lines) | Disposition |
|---|---|---|
| 1 | `agent_list_props_status_glyph_color_per_status` (tests `:234`, main_tests `:246`) | Re-point: projection-level test that the shipped row's glyph role matches `status_role` for all eight `AgentStatus` cases including the unconfirmed arm (the issue's item 3). |
| 2 | `agent_list_props_running_selected_spans` (tests `:168`, main_tests `:192`) | Re-point: selected row composes marker `">> "`, glyph, badge, name, suffix in order via `project_model_rows`. |
| 3 | `agent_list_props_grabbed_prefix` (tests `:203`, main_tests `:221`) | Re-point: grabbed row marker is `"↕ "` and outranks selection, driven through `project_host_panel` with `dashboard_grab` set. |
| 4 | `bright_selected_agent_row_keeps_fixed_glyph_color` (tests `:305`, main_tests `:311`) | Re-point: ANSI canvas test over the real `ProviderScreen` (`write_ansi`, precedent `host_control_overlay.rs:92`): the glyph keeps its role color on the selected row. |
| 5 | `agent_list_props_with_git_info_adds_suffix_span` (tests `:341`, main_tests `:341`) | Re-point: a snapshot entry adds the dim suffix span. |
| 6 | `agent_list_props_with_empty_git_info_no_suffix` (tests `:375`, main_tests `:369`) | Re-point: empty `list_suffix` yields no suffix span. |
| 7 | `agent_list_props_git_infos_shorter_than_agents` (tests `:403`, main_tests `:391`) | Re-point: snapshot shorter than the agent list neither panics nor misaligns. |
| 8 | `agent_list_git_info_suffix_renders` (tests `:434`, main_tests `:416`) | Re-point: ANSI canvas test, suffix paints dim in the rendered panel. |
| 9 | `agent_list_dirty_suffix_renders_marker` (main_tests `:441`) | Re-point: `dirty == Some(true)` yields trailing `" *"` through the dashboard projection (the issue's item 4). |
| 10 | `agent_list_unknown_dirty_suffix_no_marker` (main_tests `:478`) | Re-point: `dirty == None` yields no marker. |
| 11 | `agent_list_clean_suffix_renders_no_marker` (main_tests `:507`) | Re-point: `dirty == Some(false)` yields no marker. |
| 12 | `an_unconfirmed_running_agent_is_not_shown_as_a_healthy_one` (`agent_list.rs:278`) | Re-point (issue-mandated): `"~"`/Yellow vs `"*"`/Bright asserted against the shipped projection, not dead code. |
| 13 | `twenty_five_row_pane_keeps_twenty_fifth_agent_visible` (`agent_list.rs:215`) | Delete: it pins component-layer windowing that no longer exists at that layer; the shared control's offset clipping is covered by `dev-docs/tmux-scenarios/dashboard-list-windowing.json` (manifest-verified, 9 steps) and the shared window helper tests. |

New unit test homes follow the existing `#[path]` split-file convention: a new `src/host_panel_models_agent_row_tests.rs` for projection tests; ANSI/painted tests beside `provider_screen_count_render_tests.rs`.

## 4. Acceptance matrix

| # | Actor / surface | Input / state | Observable success | Observable failure | Permitted side effects | Proving test |
|---|---|---|---|---|---|---|
| A1 | Dashboard Agents row, any operator | Agent with any of the 8 statuses | Row shows the symbolic glyph (`* + x ! ? - o`, `~` when running-unconfirmed) and no `[Running]`-style text | Glyph absent, or textual status present | None | Scenario S2 frame asserts; unit U1 (8 roles) |
| A2 | Same | Same row | Glyph paints in its per-status color (Bright/Red/Yellow/Blue/Dim), others of the row keep `rc.fg`, suffix dim | Uniform row color | None | ANSI canvas tests U4/U8 (frames cannot see color, `tmp/issue731-agentlist/FINDINGS.md` §3.8) |
| A3 | Same | Agent with shortcut slot 1..9 | Badge `[N] ` renders before the name | Badge absent | None | Scenario S2 frame; unit U2 |
| A4 | Same | Narrow pane (22-col rail stress, #745 lesson) | Suffix drops whole before any glyph/badge/name slicing; glyph never sliced | Half suffix or sliced glyph | None | Composition unit tests (rung table) |
| A5 | Same | Repository with github_repo + local work dir, git present | Row ends `  owner/repo @ branch` | Suffix absent | git probe via existing TTL cache | Scenario S1 step 3 (`owner/repo-230 @ main *`) |
| A6 | Same | Work tree dirty (`Some(true)`) / clean (`Some(false)`) / unknown (`None`) | ` *` only when dirty | Marker when clean/unknown | None | Units U9/U10/U11 through the dashboard projection with literal snapshots |
| A7 | Same | `dashboard_grab` holds the row (`space`) | Row marker is `↕ `, outranking `>> ` | No marker change | None | Scenario S2 after `space`; unit U3 |
| A8 | Operator drag-selects in the Agents pane | Mouse down/drag/up over rows | Covered rows paint inverse video (`sel_bg`/`sel_fg`); clipboard text equals the rendered row text for the same window | Highlight or copy diverges from render | Selection state in `AppState` | ANSI canvas test U12; copy-vs-render unit U13 at fixed width |
| A9 | Required scenario `send-to-agent-details.json` (macOS, manifest-required, 23 steps) | Runs at candidate head | Passes 23/23 including step 3 `HAR` wait for `owner/repo-230 @ main *` | `HAR-E005` at step 3 (today's red) | None beyond the scenario's own fixtures | `scripts/run-scenario-manifest.py --platform macos --scenario dev-docs/tmux-scenarios/send-to-agent-details.json` |
| A10 | Scenario corpus accounting | New scenario S2 added | `tests/scenario_manifest.rs` proves manifest classifies the whole corpus and owner evidence matches | Missing/mismatched manifest or evidence pins | Manifest + evidence file updates (repin) | `cargo test --test scenario_manifest` |

Failure diagnostics: scenario failures name the step and the unobserved literal (HAR codes); unit failures print the composed spans/row.

## 5. Non-goals

- Preview pane content and its `Branch:`/todo rows (#733 territory, already restored on main for render).
- Panel focus routing, footer chrome, top bar, search band, Repositories-screen geometry (card grid density, filter band, page indicator), sidebar count form `(N)` vs `[N]`, marker width `>> ` vs `> `: the cutover forms stand; each remaining item is tracked separately per the issue and `tmp/issue731-agentlist/FINDINGS.md` §4.
- Any change to `GitRepoInfo` probing, caching, or TTL.
- Workbench card content and the Repositories STATUS block.
- Screen descriptors, `shipped-screen-definition-parity.json`, the provider wire DTO schema (glyph/badge/suffix must never reach the wire; if the DTO conversion layer needs touching, stop).
- Selection highlight for panes other than `AgentList`, and unifying the copy path's derived geometry with the resolved layout snapshot (follow-ups).
- Consolidating `selectable_list::SpanRole` with the new `HostControlSpanRole`, and the `agent_preview` inline probe (`:394`).

## 6. Vertical slices (RED, GREEN, verify per slice)

### Slice 1: shared span/role shape and list-control composition (D1, D2, D4)

- RED: add projection tests first: a `ListItem` with glyph/badge/suffix composes the rung table's spans at a tight width (suffix dropped whole, glyph never sliced) and rows without them stay byte-identical. Tests fail to compile/pass because the fields do not exist yet.
- GREEN: `panel_model.rs` fields; `host_controls` span types, `compose_list_item_row` returning spans, rung extension; `ProjectedRow`/`PanelProjection` carry-through; `render_panel` segmented painting (empty spans = today's paint).
- Evidence: `cargo test -q` all existing list/render suites green untouched (no-regression proof), new tests green. No scenario change; no visible behavior change alone.
- Allowed files: `src/runtime/provider/panel_model.rs`, `src/host_controls.rs`, `src/provider_panel_view.rs`, `src/ui/components/provider_screen.rs`, new/updated test files. Stop if the provider DTO conversion layer must change.

### Slice 2: the agent row and its git supply (D3, D5), the visible restoration

- RED first, at no cost: `send-to-agent-details.json` already fails at step 3 on `5febc492` (evidence: `tmp/issue731-agentlist/manifest-new/`). Record a fresh run at exact head before touching production. S2 (below) is built and shown red in this slice too.
- GREEN: glyph/badge/suffix/status fields in `agent_list`; `grabbed_id` plumbing and `marked_id` marker override; `project_host_panel(state, source, git)` parameter with the render-boundary resolve in `project_declared_content`; `None` at `host_panel_input_ops` and existing tests; move `status_icon`/`status_role` into `host_panel_models.rs`; new `host_panel_models_agent_row_tests.rs` (U1-U3, U5-U7).
- Evidence: `send-to-agent-details.json` passes 23/23 at this slice's head; S2's glyph/badge/suffix assertions pass; unit suites green. Commit one coherent green behavior.
- Allowed files: `src/host_panel_models.rs`, `src/host_controls.rs` (marked_id only), `src/provider_panel_view.rs`, `src/state/host_panel_input_ops.rs` (two `None` arguments), test files, `dev-docs/tmux-scenarios/issue730/*`, `scripts/issue730-git-shim.sh`, `scripts/issue730-tmux-shim.sh`, manifest + evidence repins. Stop if anything else in `src/state/` needs editing.

### Slice 3: scenario S2 green (A1, A3, A7 at 120x40)

- RED: S2 added in Slice 2's working set; at exact head its glyph assertions fail (`HAR-E005`), proving the scenario bites.
- GREEN: after Slice 2's production change, S2 passes end to end: boot frame carries `* [1] Alpha One`, `o [2] Alpha Two`, `x [3] Alpha Three`, `~ [4] Alpha Four` and the `owner/widgets @ main *` suffix; after `a` (focus the Agents pane) and `space`, the frame carries `↕`. Alpha Two intentionally uses persisted `last_known: "unknown"`, which durable restore projects as Queued; Alpha Three uses `last_known: "stopped"`, which projects as Dead. This fixture ordering exercises durable construction rather than changing status semantics. Pin `expect` from the first green run (steps total, assertion counts, operations).
- The deterministic-shim decision (recorded because it gates the `*` and `~` assertions): the strict shim accepts the exact prefixed argv emitted at startup. It answers `-V` with `tmux 3.4`; `has-session` and `list-panes ... #{pane_dead}` report live panes for `jefe-agent_widgets_one` and `jefe-agent_widgets_four`; `list-sessions -F #{session_name}` reports only the Alpha One session; and both batched and per-session `list-windows` reconciliation forms return an empty successful result. The incomplete Alpha Four inventory makes startup preserve the persisted Running value without inventing a runtime binding, so the ordinary domain predicate projects it as unconfirmed. Every other invocation exits 64. The Alpha One fixture binding uses the runtime-derived `jefe-agent_widgets_one` identity. A stale `jefe-widgets-1` binding would be inconsistent ownership evidence and startup would correctly demote it. The git shim remains separate and strict.

### Slice 4: selection identity (D6, A8) plus the dead-test cutover

- RED: ANSI canvas test U12 (selection over the Agents row paints `sel_bg`/`sel_fg`) and copy-vs-render unit U13 fail first (no highlight exists in `render_panel`; copy comes from the dead renderer).
- GREEN: `render_panel` AgentList highlight; `agent_list_lines` re-pointed to `project_host_panel` + `project_model_rows` + the shared visible-window helper; delete `src/ui/components/agent_list.rs` and the `mod.rs` re-exports; execute the 13-test disposition table (12 re-points, 1 delete).
- Evidence: new tests green; `cargo test -q` whole workspace green with no test pinning an unrendered row shape; both scenarios still green.

## 7. Scenario S2 specification

- Path: `dev-docs/tmux-scenarios/issue730/agent-row-status-glyphs.json`. Schema 1, `platform: macos`, `terminal: 120x40` (issue requirement).
- Fixture (workspace): one repository (`github_repo: "owner/widgets"`, `remote.enabled: false`) with four agents `Alpha One..Alpha Four`, shortcut slots 1..4, work dirs `work/one..work/four`, and persisted `last_known` values Running, Unknown, Stopped, and Running. Durable restore maps those values to Running, Queued, Dead, and Running (`src/state/durable_restore.rs:261-281,294-299`), which intentionally puts queued Alpha Two before dead Alpha Three. Alpha One carries the runtime-derived session `jefe-agent_widgets_one`. Alpha Four carries `session_id: null`, so restore gives it `AgentStatus::Running` with `runtime_binding: None`; that is exactly the domain predicate `Agent::state_is_unconfirmed` (`src/domain/mod.rs:841-853`). The `~` row therefore comes from the ordinary host projection of a true unconfirmed-running domain value. No product liveness path or special-case status code changes for the scenario. One uncommitted file per work dir is unnecessary because the git shim answers dirty deterministically.
- Shims: `scripts/issue730-git-shim.sh` exact-matches `rev-parse --abbrev-ref HEAD` and `status --porcelain=v1 -z` for the four fixture work dirs. `scripts/issue730-tmux-shim.sh` exact-matches the startup vocabulary recorded in Slice 3, including the prefixed version probe and the two persisted-running agents' liveness and registration probes. Both exit 64 on any unknown invocation. Harness PATH is `${workspace}/bin` only (`src/harness/v1/env.rs:20-24`), so installed shims are the only git/tmux executables the binary can see.
- Steps: `launch` -> `wait "Alpha One"` -> `assert-frame` containing the four glyph+badge literals and the suffix, absent `["[Running]", "[Dead]", "[Queued]", "[Waiting]"]` (pins the drop-the-textual-status decision; the dashboard frame has no other bracketed status words) -> `key a` -> `key space` -> `wait "↕"` -> `assert-frame` contains `↕` -> `finish`.
- Manifest entry (`dev-docs/testing/scenario-execution-manifest.json`): `scenario_schema: 1`, `criteria: ["CW00B-02", "CW00B-04"]` (the same pair every dashboard scenario carries; confirm against `validate_criteria` during implementation), `platforms` macos required / linux+windows unsupported with the standard reason strings, `ci_job: tui_scenarios_macos` (mandatory for a macos scenario, `tests/scenario_manifest.rs:247-258`), installs `jefe=cargo-bin:jefe`, `tmux=repo:scripts/issue730-tmux-shim.sh`, and `git=repo:scripts/issue730-git-shim.sh`, with `timeout_ms` 180000 and `expect` pinned from the first green run.
- Deterministic repin file set (precedent: `db85c5a9` added `issue731/dashboard-focus-chrome.json` touching exactly these): the manifest; `dev-docs/testing/scenario-owner-evidence.json` (`scenario_manifest_sha256`, the new owned-scenario entry with the scenario file's sha256/mode/criteria/command, and the manifest's own artifacts entry); `dev-docs/testing/issue704-owner-evidence.json` and `issue705-owner-evidence.json` (both pin the shared manifest and owner-evidence hashes and must be re-pinned whenever the manifest changes). `tests/scenario_manifest.rs` and its driver/projection tests enforce that chain (`manifest_exactly_classifies_the_recursive_corpus`, `owner_evidence_matches_the_active_manifest`). Issue #705 also protects eleven files changed by this slice, so its artifact entries and aggregate `artifact_set_sha256` must be recomputed from the final bytes: `host_controls.rs`, `host_controls_tests.rs`, `host_panel_models.rs`, `provider_panel_view.rs`, `provider_panel_view_tests_host.rs`, `provider_panel_view_tests_origins.rs`, `runtime/provider/panel_model.rs`, `runtime/provider/panel_reader.rs`, `state/host_panel_input_ops.rs`, `state/navigation_provider_relationship_tests.rs`, and `workbench/screens_tests.rs`.

## 8. Exact commands and evidence plan

- Fresh RED record at exact head, before Slice 2 production edits:
  `cargo build --locked --all-features --bin jefe --bin tmux_scenario --bin jefe-harness-probe --bin jefe-capture-shim --bin jefe-jsp-llxprt-fixture`
  `python3 scripts/run-scenario-manifest.py --platform macos --scenario dev-docs/tmux-scenarios/send-to-agent-details.json --tmux-scenario target/debug/tmux_scenario --jefe target/debug/jefe --probe target/debug/jefe-harness-probe --jsp-fixture target/debug/jefe-jsp-llxprt-fixture --shim target/debug/jefe-capture-shim --reports tmp/issue730/reports-red`
  Expected: step 3 `HAR-E005: literal 'owner/repo-230 @ main *' not observed within 15000 ms`.
- S2 RED checkpoint: `tmp/issue730/red-new2/reports-final/` contains the manifest-driver report, runner logs, completion record, and `final-frame.txt`. The driver accepts the expected scenario exit and reports step 2 `assert-frame` failed with `HAR-E006: frame does not contain '* [1]'`. The frame has `Alpha One [Running]`, `Alpha Two [Dead]`, `Alpha Three [Queued]`, and startup-normalized `Alpha Four [Dead]`. It has none of `* [1]`, `x [2]`, `o [3]`, `~ [4]`, or `owner/widgets @ main *`; it still has `[Running]`, `[Dead]`, and `[Queued]`. Runner stderr is empty, the app remains live, and neither report nor frame contains an unexpected-shim warning. This is usable presentation RED rather than fixture or harness failure.
- Slice GREEN runs: the same command per scenario (`--scenario` repeated), reports under `tmp/issue730/reports-<slice>`; S2 red first, then green; save both reports as evidence.
- Iteration: `cargo xtask quick` (fmt + check + test).
- Exact-head full gate before each push: `cargo xtask ci` (format check, clippy-allow policy, source-size, architecture policy, clippy complexity gates, 30% line-coverage gate, workspace build, full test suite) plus the scenario manifest tests `cargo test --test scenario_manifest --test scenario_manifest_driver --test scenario_manifest_projection` (names per `tests/scenario_manifest/`), plus both scenario runs above at the exact candidate head.
- Complexity watch: `compose_list_item_row`/`push_list_item_row` must stay under the 60-line / cognitive-15 gates (`clippy.toml`); the rung extension is where they approach the limit, so extract the span-fitting helper rather than growing the match.

## 9. Expected paths (complete list by layer)

Production:
- `src/runtime/provider/panel_model.rs` (ListItemGlyph/Role; ListItem fields; wire untouched)
- `src/host_controls.rs` (HostControlSpan/Role; HostControlRow.spans; rung composition; marked_id)
- `src/provider_panel_view.rs` (ProjectedRow/PanelProjection spans; git resolve at `project_declared_content`; `project_model_rows` + visible-window helper `pub(crate)`)
- `src/ui/components/provider_screen.rs` (segmented row painting; role resolution; AgentList highlight)
- `src/host_panel_models.rs` (agent row fields; grabbed_id; moved glyph/role fns; signature)
- `src/state/host_panel_input_ops.rs` (two `None` arguments only)
- deletion: `src/ui/components/agent_list.rs`, re-exports in `src/ui/components/mod.rs`
- `src/selection/dashboard_content.rs` (copy-path re-point)

Tests: `src/host_panel_models_agent_row_tests.rs` (new), `src/ui/components/provider_screen_agent_row_render_tests.rs` (new, ANSI), updates to `host_panel_models_chrome_tests.rs` (label fallback still passes; status-text expectations flip to the new row), re-points/deletes in `selectable_list_tests.rs` + `selectable_list_main_tests.rs`, `src/ui/components/mod.rs` re-export removal.

Scenarios/evidence: `dev-docs/tmux-scenarios/issue730/agent-row-status-glyphs.json` (new), `scripts/issue730-git-shim.sh`, `scripts/issue730-tmux-shim.sh` (new), the four evidence/manifest JSON files in §7.

## 10. Data and ownership flow (after restoration)

```text
AppState (agents, dashboard_grab, agent_scroll_offset, selection)
   │
   │ render frame
   ▼
provider_panel_view::project_current_screen  ── resolve_dashboard_git_info(state)  [once, AgentList only]
   │                                          (TTL cache shared with selection path)
   ▼
project_host_panel(state, AgentList, Some(&snapshot))
   │  glyph/role + badge + suffix + grabbed_id per visible agent   [pure]
   ▼
host_controls::project_list  ── marker (marked_id > selected) + rung composition  -> Vec<HostControlRow{text, spans, target}>
   ▼
provider_panel_view::project_host_model -> PanelProjection { lines, spans, hit_targets }   [clipped window]
   ▼
provider_screen::render_panel
   ├── per-span role -> color (Bright/Dim/Red/Yellow/Blue/Themed)
   └── AgentList rows: row_highlight_range(selection, i) -> sel_bg/sel_fg highlight

copy path (drag): mouse_routing binds snapshot at drag start -> pane_content_projection
   -> dashboard_content::agent_list_lines -> project_host_panel + project_model_rows + shared window helper
   -> lines identical to rendered rows
```

Ownership: the model layer stays pure (snapshot in, items out); the probe lives at the render boundary; the control owns row grammar; the renderer owns color; selection math stays in `selection/`.

## 11. Scope ledger and OCR counters

- OCR budget per `dev-docs/workflow/ISSUE-DELIVERY.md` §8: at most 2 local OCR runs before the PR, at most 2 after opening; counters recorded here as spent (1/2 pre-PR, 0/2 post-PR), each run only on a stable verified checkpoint.
- Ledger for newly discovered work (none authorized by this plan): selection highlight for panes beyond AgentList; copy-path geometry unification with the resolved snapshot; `SpanRole`/`HostControlSpanRole` consolidation; `agent_preview` inline probe relocation. Each becomes a follow-up issue rather than diff growth here.
- Every changed file above maps to an acceptance row A1-A10 or the S2 manifest repin; anything else requires explicit approval before implementation.

## 12. Stop conditions

- S-1: the tmux shim cannot pin the bound-running `*` probe deterministically (liveness vocabulary differs from `liveness.rs:35-68`), or the `*` row still flips in frames. Stop; the acceptance row needs reshaping with the owner.
- S-2: the glyph/badge/suffix fields would have to reach the provider wire DTO (`src/runtime/provider/` dto/conversion) to work. Stop; that is a protocol change needing approval.
- S-3: a screen descriptor, `shipped-screen-definition-parity.json`, or focus/layout behavior must change. Stop; out of scope.
- S-4: `GitRepoInfo` probing/caching/TTL must change, or a second probe site appears. Stop; non-goal.
- S-5: `origin/main` advances more than five commits or touches this slice's contract set (`host_panel_models.rs`, `host_controls.rs`, `provider_panel_view.rs`, `provider_screen.rs`, manifest) before a slice lands: fetch, pause, rebase or true merge per ISSUE-DELIVERY §6.
- S-6: an OCR run reports a blocker that requires widening beyond §9's file list. Triage per ISSUE-DELIVERY §8; expand only with approval.

Decisions requiring owner approval if challenged: dropping the textual `[Running]` suffix (recorded preference in the issue), the `>> ` marker form remaining (cutover form stands), AgentList-only highlight scope, and the dedicated tmux shim for S2.

## 13. Open Code Review triage

Local OCR count after the first implementation review: 1/2 pre-PR, 0/2 post-PR. The eight findings in `tmp/issue730/ocr1/19-findings.json` have these dispositions:

| # | Finding | Disposition | Evidence and action |
|---|---|---|---|
| 1 | The tmux shim rejects the product's batched `list-panes -a -F #{session_name}:#{window_index}:#{pane_dead}` query | **In-scope-Fix** | `src/runtime/liveness.rs:463-480` uses this exact query. The pre-fix audit returned 64, so the scenario emitted an unexpected-shim warning instead of exercising the supported query. The shim now returns only `jefe-agent_widgets_one:0:0`; all unknown commands still return 64. |
| 2 | The git shim's `printf '...\\0'` is not portable | **Reject** | POSIX specifies the `printf` utility's format operand and permits implementation-defined extensions; macOS `/bin/sh` uses the macOS `printf` behavior exercised by this macOS-only scenario. A byte audit through `/bin/sh` produced `204d207365656465642d6368616e67652e74787400`, including the required trailing NUL, and the scenario consumed it successfully. Changing an accepted fixture form without a failure would not improve issue #730 coverage. |
| 3 | Move Git resolution off the render path into another asynchronous cache | **Reject** | `src/git_info/mod.rs:30-54` already provides the required process-global resolver with a 5-second branch/dirty TTL, a 5-minute origin TTL, and a 3-second probe timeout. This behavior predates issue #730. The issue excludes changes to Git probing, caching, and TTL, and the proposed state cache would add a second subsystem without a demonstrated failure. |
| 4 | Alpha Four succeeds under `has-session` while session discovery omits it | **Reject** | `src/runtime/liveness.rs:211-218` states that `list-sessions` and `list-panes -a` are independent tmux queries whose results may differ during concurrent session creation or destruction. The shim comment records that contract. Alpha Four's immediate `has-session` and per-session pane checks succeed, while persisted state has no runtime binding, so the ordinary domain projection remains Running-unconfirmed without a delay or timeout simulation. |
| 5 | The scenario does not assert `[Waiting]` absent | **In-scope-Fix** | Plan section 7 requires all four textual status forms to be absent. `[Waiting]` was added to the same `assert-frame` step and the evidence chain is deterministically re-pinned. |
| 6 | Highlighting re-derives the visible origin from `AppState` | **In-scope-Fix** | A RED test set state offset 0, projected origin 3, and selected content row 3. `PanelProjection` now carries the origin returned by the one visible-window helper, and the renderer uses that value. Provider-backed offset projection and the screen-level copy/render test also assert the carried origin. |
| 7 | Span-to-line equality is debug-only | **In-scope-Fix** | A RED projection test showed mismatched spans were accepted. `split_projected_rows` now enforces concatenation with an unconditional assertion before constructing `PanelProjection`, so release builds fail fast instead of painting content that differs from hit targets or copy text. |
| 8 | The copy/render parity test compares duplicate pipelines | **In-scope-Fix** | The duplicate expected-value construction was removed. The test now compares `agent_list_lines` with the actual AgentList `PanelProjection` from `project_current_screen` at fixed 90x20 geometry, 24 agents, and scroll origin 2, preserving A8 while exercising resolver-owned geometry. |

No finding is deferred. No Git cache, probe, async, provider-wire, or unrelated subsystem change was made.
