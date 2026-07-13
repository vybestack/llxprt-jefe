# CW-03: Action registry, source-derived default inventory, and single-chord keymaps

## Parent, dependencies, outcome, and current source

Parent: **Epic: Configurable Workbench v1**. Consumes the deterministic harness and schema-2 configuration/closed-effect contracts. The delivered outcome is one immutable action/binding snapshot used by keyboard and mouse dispatch, Help, keybind footer, menus, explain CLI, and the Keys editor. With no override, every existing input remains behaviorally identical.

| Source symbol/module | Current responsibility | Final responsibility |
|---|---|---|
| `src/input.rs` quit, Ctrl-C, scrollback, search, and encoding helpers | global interception and terminal rules | translate raw `crossterm::KeyEvent` to canonical `Chord` or protected terminal event |
| `src/app_input/mod.rs::handle_f12_toggle`, `handle_global_shortcut_key`, `handle_mode_help_key` | pre-message shortcuts/help | dispatch resolved action IDs; retain raw PTY forwarding only |
| `src/app_input/normal.rs::handle_normal_key_event` | dashboard/mode dispatch | consume one resolution result |
| `src/app_input/issues.rs::resolve_issues_key_event` | Issues/editor/chooser/search/filter dispatch | typed handlers referenced by registry handler keys |
| PR key/filter/inline modules under `src/app_input/` | PR dispatch | typed handlers referenced by registry handler keys |
| actions/workflow modules under `src/app_input/` | Actions dispatch | typed handlers referenced by registry handler keys |
| `src/messages/event_conversion.rs` | input-to-message conversion | convert handler output to smallest typed message |
| `src/ui/components/keybind_bar.rs::keybind_hints_for` | static footer strings | pure projection of resolved available actions |
| `src/ui/modals/help.rs` | static help | pure projection of the same snapshot |
| `src/mouse_routing.rs` | mouse hit routing | emit the action ID assigned to a hit target |

## Consumed contracts

| Contract | Use |
|---|---|
| Harness schema 1 | exact platform key translation, PTY bytes, mouse coordinates, frames, restart and capture assertions |
| Settings schema 2 | lossless `keymap.<context>.<action>` whole-list overrides; `[]` means explicit unbind; absent syntax inherits |
| Closed effects/correlation | availability completions carry owner/generation; stale results are ignored |
| Existing typed messages | handler keys produce typed messages; registry never stores closures, services, or generic payloads |

## Closed contracts and algorithms

```rust
pub struct Action { pub id: ActionId, pub label: String, pub description: String,
 pub category: String, pub contexts: Vec<ContextId>, pub handler: HandlerKey,
 pub protected: bool }
pub enum Availability { Available, Unavailable { reason: String } }
pub struct Binding { pub context: ContextId, pub action: ActionId,
 pub chords: Vec<Chord>, pub provenance: Provenance }
pub struct Chord { pub modifiers: ModifierSet, pub key: Key }
pub enum Modifier { Ctrl, Alt, Shift, Super }
pub enum Key { Char(char), Enter, Esc, Tab, BackTab, Backspace, Delete, Insert,
 Home, End, PageUp, PageDown, Up, Down, Left, Right, Function(u8) }
pub enum Resolution { Dispatch { action: ActionId, handler: HandlerKey },
 Unavailable { action: ActionId, reason: String }, ForwardToPty, Unbound }
```

Function keys are 1 through 24. A character key is one Unicode scalar. Canonical text orders modifiers `Ctrl+Alt+Shift+Super+Key`; named key spelling is exactly the enum spelling and functions are `F1` through `F24`. Duplicate modifiers, modifier-only input, unknown names, multiple scalars, sequences, duplicate chords, and more than eight chords for one action/context are invalid.

Resolution is deterministic: canonicalize platform event; if terminal capture is active, intercept only protected exit, `F12`/`t` leave-capture, and scrollback controls, otherwise return `ForwardToPty`; then search modal, focused editor/chooser, focused panel, screen, and global contexts in that order. An explicitly declared child binding shadows its parent. An implicit shadow or two actions sharing a chord in one context is `KEY-E401` and rejects the complete candidate. Availability is computed once after registry composition. An unavailable resolution performs no handler/effect and exposes exactly the same reason in dispatch notice, Help, footer, menu, and editor.

Protected actions are `core.emergency-exit`, `core.leave-terminal`, `core.back`, and the tiny-layout focused Back. They cannot be unbound or shadowed, and reachability is validated for macOS and Linux. Effective bindings are capped at 2,048; contexts/action IDs are lowercase ASCII IDs 1–128 bytes; labels are 128 cells; descriptions 4,096 bytes.

## Exact source-derived v1 action inventory

This table is the required compiled inventory and golden input. Context-specific rows with the same chord are separate actions; editor text insertion is raw input, not an action. Alt/Option digit rows exist for digits 1 through 9.

| Context | Canonical chord(s) | Action ID / observable behavior |
|---|---|---|
| global | `Ctrl+Q`, rapid bare `q`,`q`,`q` within the existing one-second sequence window | `core.emergency-exit` / quit |
| global | `F1`, `?`, `h` | `core.help` / open or close Help |
| global | `F12`, `t` | `core.toggle-terminal` / focus or leave terminal |
| global | `Alt+1` through `Alt+9` | `core.jump-agent.1` through `.9` / select corresponding agent slot |
| terminal | `PageUp`, `PageDown`, `Home`, `End`, `Up`, `Down` when scrollback routing conditions in `src/input.rs` hold | `core.terminal-scroll-page-up`, `-page-down`, `-top`, `-tail`, `-up`, `-down` |
| terminal | ordinary keys including `Ctrl+C` | raw PTY forwarding, not registry dispatch |
| dashboard | arrows, `j`, `k`, Tab/Shift-Tab | repository/agent/pane navigation using current handlers |
| dashboard | `Enter` | `core.activate-selection` |
| dashboard | `n`, `N` | current New Agent/New Repository actions according to focused pane |
| dashboard | `d`, `Delete` | current delete selection action |
| dashboard | `k` where current pane maps kill, `r` where current pane maps restart/relaunch | current typed runtime actions; context disambiguates navigation uses |
| dashboard | `s` | `core.open-split` |
| dashboard | `i` | `github.open-issues` |
| dashboard | `p`, `P` | `github.open-pull-requests` |
| dashboard | `a` where current dashboard mapping opens Actions | `github.open-actions` |
| split | arrows, `j`, `k` | split selection/navigation |
| split | `Enter` | split grab/move action selected by current split state |
| split | `Esc`, `q` | `core.back` |
| issues list | `Up`,`Down`,`PageUp`,`PageDown`,`Home`,`End` | issue list navigation |
| issues list | `Left`,`Right`,Tab,Shift-Tab | reverse/forward pane focus |
| issues list | `Enter` | open/activate issue |
| issues list | `n`,`N` | new issue composer |
| issues list | `f` | open filter controls |
| issues list | `/` | focus search |
| issues detail | `Up`,`Down`,`PageUp`,`PageDown` | detail scrolling |
| issues detail | Tab,`j`,Shift-Tab,`k` | detail subfocus next/previous |
| issues detail | `e` | edit focused body/comment when allowed |
| issues detail | `c` | new comment composer when allowed |
| issues detail | `r` | reply to focused comment when allowed |
| issues detail | `S` | send to agent chooser when agents exist |
| issues | `a`,`Esc` after local unwind | exit Issues; detail `Esc` first refocuses list |
| PR list/detail | arrows, `j`,`k`,Tab,Shift-Tab,`PageUp`,`PageDown` | current list/detail focus, selection, subfocus and scroll handlers |
| PR list/detail | `f`,`/` | filter/search |
| PR detail | `c`,`r`,`R`,`e` | comment, reply, resolve-thread, edit where current capability allows |
| PR list/detail | `S` | send to agent chooser |
| PR list/detail | `o` | open selected PR in browser or shared unavailable notice |
| PR list/detail | `m` | merge chooser where current merge capability allows |
| PR | `a`,`Esc` after local unwind | exit PRs; detail `Esc` first refocuses list |
| Issues/PR inline editor | Unicode character, Enter, Backspace, Delete, arrows | raw editor operation, not remappable action |
| Issues/PR inline editor | `Ctrl+Enter` | submit editor |
| Issues/PR inline editor | `Ctrl+C`, `Esc` | cancel editor/local unwind |
| Issues/PR agent chooser | Up,Down,Enter,Esc | chooser previous/next/confirm/cancel |
| Issues/PR filter | Tab,Shift-Tab,Space,Enter,Esc,`Ctrl+C` | field next/previous/cycle/apply/close/clear |
| Actions | arrows,`j`,`k`,`PageUp`,`PageDown`,Enter | workflow/run selection, detail scroll, activation/dispatch according to current focus |
| modal confirm | Tab,Shift-Tab,Left,Right,Enter,Esc | focus previous/next, confirm, cancel |
| Help | Up,Down,PageUp,PageDown,Home,End,Esc,`?`,`h`,F1 | scroll or close Help |

Before implementation, a generated golden enumerates every concrete `(context, canonical chord, action ID, handler key, availability predicate, Help row, footer row)` by auditing all `KeyCode` matches in `src/input.rs` and `src/app_input/`; CI fails if a source dispatch has no row or a row has no dispatch. The table above fixes semantic scope while that machine golden captures every current context refinement without inventing behavior.

Serialized override example:

```toml
[keymap."core.dashboard"]
"core.help" = ["F1", "?"]
"core.open-settings" = [","]
"core.optional-action" = []
```

`jefe explain binding CHORD [--context ID]` performs no TUI/provider/probe/write. It prints normalized chord, searched contexts in order, winner, availability/reason, shadows, and provenance. Exit is 0 resolved, 2 invalid/unresolved, and 64 usage.

## Harness translation and capture compatibility

The harness schema-legacy adapter translates existing `send_key` spellings to canonical `Chord` without changing old scenario bytes. macOS Option events normalize to `Alt`; Shift-Tab normalizes to `BackTab`; uppercase characters preserve their scalar and explicit Shift provenance; function/named keys preserve names. Capture reports both original platform event and canonical chord, then the single resolution (`action`, `unavailable`, `forward`, or `unbound`). PTY capture asserts exact encoded bytes separately, so registry migration cannot turn forwarded input into actions. Mouse capture records frame sequence, cell coordinate, resolved layout hit target, and emitted action ID. Existing scenario expected frames remain unchanged unless the issue explicitly adds the Keys UI.

## UI state mocks

**Normal**
```text
+ Keys ------------------------------+
| Context: Dashboard                 |
|  F1       Help          compiled   |
|  ,        Settings      compiled   |
+ Enter Edit  Delete Unbind  r Reset +
```
**Focused**
```text
+ Keys ------------------------------+
|> F1       Help          compiled   |
|  ?        Help          compiled   |
+ focused row has `>` and border ----+
```
**Unavailable**
```text
+ Help ------------------------------+
| m  Merge  Unavailable: no PR       |
| dispatch and Help share this text  |
+------------------------------------+
```
**Error**
```text
+ Keys ------------------------------+
| Ctrl+M conflict: Merge / Move      |
| KEY-E401; Save disabled            |
+ [Fix selected binding] ------------+
```
**Dirty**
```text
+ Save key changes? -----------------+
| [Save]  [Discard]  [Cancel]        |
+ Tab/Shift-Tab  Enter  Esc ---------+
```
**Recovery**
```text
+ Protected bindings ----------------+
| F12 Leave terminal: active         |
| Ctrl+Q Emergency exit: active      |
| invalid override was not published |
+------------------------------------+
```
**Small terminal**
```text
+Keys-----------+
|>F1 Help       |
| ! KEY-E401    |
| q Back Ctrl-Q |
+---------------+
```

## Failure and recovery

| Failure | Result | Recovery |
|---|---|---|
| grammar/conflict/protected violation | reject entire candidate; retain bytes and prior snapshot | correct/reset override and restart |
| unavailable action | zero handler/effect; shared reason | satisfy capability or select another action |
| stale availability | ignore completion | current generation remains authoritative |
| malformed settings or broken provider | explain CLI remains provider-free | inspect/reset configuration offline |

## Test-first criterion ledger

| ID | Singular EARS criterion | Scenario | Test | Fixture evidence |
|---|---|---|---|---|
| CW03-01 | WHEN the frozen inventory runs, Jefe shall dispatch every recorded default exactly once. | `current-action-default-parity.json` | `every_current_default` | generated complete context/chord/action/handler/help/footer golden |
| CW03-02 | WHEN a chord arrives, Jefe shall resolve at most one action by the declared context order. | `contextual-keymap-override.json` | `context_resolution` | all six context levels and explicit parent shadow |
| CW03-03 | WHEN availability changes, Jefe shall project identical status to dispatch, Help, footer, menu, and editor. | `keymap-projection-consistency.json` | `availability_projection` | available/unavailable rows with byte-equal reason |
| CW03-04 | IF conflict or implicit shadow exists, Jefe shall reject the candidate with `KEY-E401`. | `keymap-conflict-startup.json` | `conflict_validator` | same-context, implicit-child, alias, duplicate and protected cases |
| CW03-05 | WHEN Unbind or Reset saves, Jefe shall omit the action or inherit the prior layer exactly. | `keymap-unbind-reset.json` | `keymap_merge_roundtrip` | comments/order plus `[]`, removed syntax and restart captures |
| CW03-06 | WHEN explain runs, Jefe shall report provider-free normalized resolution and provenance. | `binding-explain-flow.json` | `explain_cli` | valid, invalid, unresolved and usage exit/output goldens |
| CW03-07 | WHILE terminal capture is active, Jefe shall keep leave-capture and emergency-exit reachable on macOS and Linux. | `terminal-protected-recovery-macos.json`, `terminal-protected-recovery-linux.json` | `terminal_protected` | exact platform events and canonical captures |
| CW03-08 | WHILE terminal capture is active, Jefe shall forward ordinary input and Ctrl-C byte-for-byte. | `terminal-passthrough.json` | `terminal_input_parity` | original events, canonical classification, encoded PTY bytes |
| CW03-09 | WHEN mouse activation occurs, Jefe shall emit the action ID from the resolved hit target. | `mouse-action-consistency.json` | `mouse_action_capture` | frame/cell/hit/action tuples for each current clickable surface |
| CW03-10 | IF effective bindings exceed 2,048 or chords exceed eight, Jefe shall reject at the owning path. | `keymap-resource-bounds.json` | `keymap_bounds` | exact 8/9 and 2048/2049 fixtures |

RED commits scenarios/fixtures first. GREEN routes every surface through one resolver. REFACTOR removes duplicate shortcut/help/footer maps only after parity.

## Normative documentation updated

* `dev-docs/standards/architecture.md`: action registry dependency direction and one-resolution invariant.
* `dev-docs/standards/display-and-ui.md`: replace static keybind authority with shared resolved action projections and exact protected terminal behavior.
* `dev-docs/standards/testing-and-quality.md`: inventory completeness guard, platform translation captures, and mouse/PTY parity.
* `dev-docs/testing/tmux-harness.md`: legacy key translation, original/canonical/resolution capture fields.
* `docs/technical-overview.md`: action composition, availability, dispatch, and explain flow.

## Definition of done

All ten criteria and exact inventory rows pass; no source dispatch lacks a golden row; dispatch/Help/footer/menu/editor agree; protected recovery remains reachable; no arbitrary shell, new dependency, generic payload, UI I/O, lint suppression, unsafe, unwrap/expect, or quality-gate weakening exists. Run unchanged `make ci-check`.