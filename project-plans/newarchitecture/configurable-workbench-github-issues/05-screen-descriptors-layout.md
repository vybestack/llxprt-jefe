# CW-04: Sole internal screen descriptors and unified layout parity

## Outcome and consumed contracts

Replace all screen-specific geometry with one I/O-free descriptor registry and one executable layout resolver while preserving Dashboard, Repositories/Split, Issues, Pull Requests, and Actions behavior. Consume the deterministic harness operation/capture contract and the action registry’s immutable action IDs, contexts, availability reasons, and resolved chords. No external screen syntax, relationship, navigation stack, or editor is introduced.

## Source/symbol responsibility and parity inventory

| Source/symbol | Required ownership | Parity that must not change |
|---|---|---|
| `src/state/types.rs::ScreenMode` | deleted at feature-complete; stable `ScreenId` is runtime authority and the one-way persistence migration maps every old variant exactly once | every old variant maps exactly once; `ScreenMode` has zero references outside the migration module before this issue closes |
| `src/ui/screens/dashboard.rs` | thin renderer over `ResolvedLayout` | repository/agent selection, focus, empty/error states |
| `src/ui/screens/split.rs` | thin renderer | repository/agent/terminal proportions and PTY focus |
| `src/ui/screens/issues.rs` | thin renderer | list/detail/filter/search/scroll/mouse behavior |
| `src/ui/screens/pull_requests.rs` | thin renderer | list/detail/actions/merge state behavior |
| `src/actions_view.rs` | thin Actions panel renderer | action ordering/status/invocation |
| `src/layout.rs` | sole `resolve_layout` implementation | checked deterministic geometry |
| `src/mouse_routing.rs` | consume resolved hit regions only | existing click/wheel targets |
| `src/detail_wrap_map.rs` and selection projections | consume resolved content rectangles | wrapping, selection, viewport parity |
| `src/ui/components/terminal_view.rs` | consume visible nonzero PTY content rect | never resize PTY to zero |

Create `ScreenDescriptor`, `PanelDescriptor`, `LayoutNode`, and `ResolvedLayout` in an I/O-free workbench module; compiled descriptors are the sole definitions of all five screens.

## Closed types, invariants, and resolver

```rust
struct ScreenDescriptor { id: ScreenId, title: String, route: RouteId,
 panels: Vec<PanelDescriptor>, initial_focus: PanelId, focus_order: Vec<PanelId>, layout: LayoutNode }
struct PanelDescriptor { id: PanelId, panel_type: PanelTypeId, config: TypedMap,
 focusable: bool, required: bool }
enum LayoutNode { Leaf { panel: PanelId }, Split { axis: Axis, children: Vec<LayoutChild> } }
enum Axis { Horizontal, Vertical }
struct LayoutChild { node: LayoutNode, size: Size, min: u16, max: Option<u16>,
 collapsible: bool, collapse_priority: Option<i32> }
enum Size { Fixed(NonZeroU16), Weight(NonZeroU16) }
struct ResolvedLayout { screen_instance: ScreenInstanceId, outer: Rect,
 panels: Vec<ResolvedPanel>, too_small: Option<TooSmall> }
struct ResolvedPanel { id: PanelId, visible: bool, chrome: Rect, content: Rect,
 depth_first_index: usize, hit_region: Option<Rect> }
```

A panel appears exactly once in `panels` and layout; each focusable panel appears exactly once in focus order; initial focus is focusable. Limits: 64 screens, 16 panels/screen, depth 8, 2–8 split children, IDs 128 bytes. Borders/titles are inside child rectangles. The caller removes global chrome once.

Executable algorithm: validate using checked `u32`; flatten each split’s children in declaration order; subtract one internal separator cell per adjacent visible child; begin with all children visible; while visible minima exceed axis space, hide a collapsible child ordered by `(collapse_priority ascending, depth_first_index descending)`; if required minima still do not fit, return only the first required focusable panel in descriptor focus order using the entire rect and `TooSmall{needed,available}`. For remaining children, clamp fixed sizes to `[min,max]`; assign weighted children their minima; distribute remaining cells by `floor(remaining*weight/sum_weight)`; assign remainder one cell at a time in declaration order; when a child reaches max remove it and repeat distribution. Derive contiguous, nonoverlapping rectangles and recurse. Zero-width/height leaves are hidden. Repair focus to the first visible focusable panel at or after prior focus cyclically, then initial focus. Hidden panels have no hit, wrap, selection, scroll, or PTY resize region. Renderer, mouse, selection, focus, scrolling, wrapping, and PTY consume the same immutable snapshot.

The one-way persistence migration maps old persisted values `Dashboard -> core.dashboard`, `Split -> core.repositories`, `DashboardIssues -> github.issues`, `DashboardPullRequests -> github.pull-requests`, and current Actions state -> `github.actions`. An invalid old value warns and selects the compiled initial screen. Runtime never uses enum ordinal, and no runtime type carries the old variants — `ScreenMode` is deleted at feature-complete with the mapping living only inside the migration module.

## Five-screen parity table

| Screen | Panels in focus order | Required/collapsible | Must preserve |
|---|---|---|---|
| `core.dashboard` | repositories, agents | repositories required; agents collapsible | hide-idle, selection, create/resume |
| `core.repositories` | repositories, agents, terminal | repositories+terminal required; agents collapsible | split sizing, terminal capture |
| `github.issues` | issue-list, issue-detail | list required; detail collapsible | filter/search/detail wrap |
| `github.pull-requests` | pr-list, pr-detail, pr-actions | list required; detail then actions collapsible | detail threads/actions/merge |
| `github.actions` | action-list, action-detail | list required; detail collapsible | availability and action execution |

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Repositories ---+ Agents +  + Repositories ---+ Agents +
|  repo-a         | agent-1 |  |>>repo-a         | agent-1 |
+-----------------+---------+  + focus: repositories -----+
```
```text
UNAVAILABLE                    ERROR
+ Issues -------------------+  + Pull Requests -----------+
| GitHub unavailable        |  | Load failed: rate limit  |
| Authenticate to continue  |  | [Retry] q Back           |
+---------------------------+  +--------------------------+
```
```text
DIRTY                          RECOVERY
N/A: descriptors create no     + Actions -----------------+
draft; existing dirty overlay  | layout repaired to list |
gets geometry parity tests.    | invalid legacy ID warned|
                               +--------------------------+
```
```text
SMALL
+Too small--------+
|>PR list         |
| need 40x10      |
| q Back Ctrl-Q   |
+-----------------+
```

Tab/Shift-Tab follows resolved focus order; panel-local arrows/j/k are unchanged; q/Esc invokes existing Back; F12/t terminal; Ctrl-Q exit. Focus and status are textual, clipping is grapheme-safe.

## Migration, failure, security, and flow

Startup validates five compiled descriptors, migrates legacy ID, creates instance, resolves once per size/state change, then passes the snapshot to every consumer. Invalid compiled descriptors fail tests/startup before publication. Arithmetic overflow is a typed layout error, never panic. Resize computes a new snapshot atomically. Tiny fallback preserves Back/exit and never emits zero PTY geometry. No I/O, arbitrary config, provider data, secret, or unsafe arithmetic enters the resolver.

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW04-01 | WHEN no override exists, Jefe shall instantiate exactly the five parity descriptors. | `shipped-screen-definition-parity.json`; descriptor golden |
| CW04-02 | WHEN each screen renders normally, Jefe shall match its legacy visual and action behavior. | five normal goldens and old-screen comparison |
| CW04-03 | WHEN focus advances, Jefe shall follow descriptor order and repair hidden focus deterministically. | five focused scenarios; focus property |
| CW04-04 | WHEN layout resolves, every geometry consumer shall receive the same snapshot identity. | render/mouse/wrap/selection/scroll/PTY integration |
| CW04-05 | WHEN fixed, weighted, min, max, and remainder allocation applies, Jefe shall execute the stated algorithm. | exhaustive small-axis property/golden |
| CW04-06 | IF optional minima do not fit, Jefe shall collapse in the stated order. | collapse-priority/depth fixture |
| CW04-07 | IF required minima do not fit, Jefe shall show the first required panel and TooSmall notice. | dimensions 1x1 through 80x24 |
| CW04-08 | WHEN a PTY panel is visible, Jefe shall provide a nonzero content rectangle. | terminal-leaf property |
| CW04-09 | WHEN old persisted screen state migrates, Jefe shall map it to the exact stable ID inside the one-way migration, and `ScreenMode` shall have zero references outside that module. | migration matrix plus superseded-symbol absence assertion |
| CW04-10 | WHEN each applicable unavailable/error/recovery state renders, Jefe shall retain screen-specific parity. | five-screen state ledger; dirty overlay parity |

RED fixtures precede implementation; GREEN migrates one consumer at a time; REFACTOR deletes the duplicate geometry, per-screen layout arithmetic, and `ScreenMode` (outside the migration module) only after parity — no consumer keeps a non-snapshot geometry path at feature-complete.

## Documentation and done

Update `dev-docs/standards/display-and-ui.md` with descriptor invariants, allocation pseudocode, snapshot consumers, focus repair, tiny behavior, and the five-screen table; update `dev-docs/standards/architecture.md` to name layout as sole geometry authority. Done requires all old layout/mouse/selection/PTY tests, ledger tests, `ScreenMode` and duplicate geometry deleted with the shim-token scan clean per the epic no-shim policy, and unchanged `make ci-check`; no dependency, suppression, threshold change, unsafe, or production unwrap/expect.