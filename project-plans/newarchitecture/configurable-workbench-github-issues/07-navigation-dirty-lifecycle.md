# CW-06: Typed routes, local unwind, navigation, and dirty lifecycle

## Outcome and consumed contracts

Deliver one deterministic navigation/dirty reducer. It consumes immutable action IDs/availability, resolved screen descriptors and panel focus, custom route activation fields, and the closed post-commit effect/correlation contract. It does not define persistence, geometry, provider navigation, hot reload, instance reuse, or a persisted stack.

## Source and symbol inventory

| Source/symbol | Current concern | Required responsibility/parity |
|---|---|---|
| `src/state/types.rs::ScreenMode` | current screen identity | already deleted by the descriptor capability; the one-way persistence migration supplies the initial instance and `NavState` is sole runtime authority |
| `src/state/modal_ops.rs` | modal unwind | emit closed `LocalIntent`; retain modal focus behavior |
| `src/state/issues_ops.rs` | issue local state/navigation | own issue panel state only, never cross-screen mutation |
| `src/state/prs_ops.rs` | PR local state/navigation | own PR panel state only, never cross-screen mutation |
| `src/app_input/mod.rs` and submodules | mode-specific key handling | resolve action then emit `LocalIntent` or `NavIntent` |
| new `src/state/navigation.rs::reduce_navigation` | absent | sole route/stack/dirty transition owner |
| `src/messages.rs` | messages | carry closed intents/completions with instance and generation |

## Closed contract and algorithms

```rust
struct RouteDeclaration { id: RouteId, activation_schema: Vec<Field>, target_screen: ScreenId }
struct Activation { route_id: RouteId, values: TypedMap, source_instance: ScreenInstanceId,
    activation_generation: u64 }
enum NavIntent { Push(Activation), Replace(Activation), Back }
struct NavState { current: ScreenInstance, stack: Vec<SuspendedInstance> }
struct ScreenInstance { id: ScreenInstanceId, screen_id: ScreenId, activation: Activation,
    panel_state: PanelStateMap, subscriptions: SubscriptionSet, generation: u64, dirty: DirtyState }
enum DirtyState { Clean, Dirty { draft_token: DraftToken,
    save: SaveIntent, discard: DiscardIntent } }
enum DirtyChoice { Save, Discard, Cancel }
enum LocalIntent { CloseHostConfirmation, ResolveDirty(DirtyChoice), CloseChooser,
    CloseEditor, CloseSearch, ClearFilter, CloseOverlay, ClearPanelTransient }
```

Activation fields are closed nonsecret boolean, optional boolean, string, integer, enum, path, or string-list values; unknown/missing/wrong fields fail before mutation. Push validates first, suspends subscriptions, appends the exact current instance, and creates a fresh monotonically identified instance. Replace validates and constructs target first, then disposes current without stacking. Back disposes current and restores the exact prior instance/focus/panel state/subscriptions. Stack max is 32; attempt 33 leaves state unchanged and emits `NAV-E001`.

Exact Back precedence is: host confirmation, dirty guard, chooser, editor, search, filter, non-dirty overlay, focused panel transient, navigation stack. The reducer computes exactly one `LocalIntent` for one Back. If none exists and stack is empty, Back leaves state unchanged and emits no effect. Dirty Save emits the typed save effect and records pending choice; only matching successful completion navigates. Save failure retains draft/current and exposes Retry, Discard, Cancel. Discard restores the draft’s base authority then performs the pending navigation. Cancel clears pending navigation and restores exact modal predecessor focus. Ctrl-Q remains protected exit and does not masquerade as Back.

Every effect/completion carries owner, screen instance, screen generation, activation generation, semantic key, and correlation ID. Suspended/disposed/stale completion is ignored. Follow-up limit is 64. Route/field IDs max 128 bytes, activation fields 32, stack 32, serialized activation 262,144 bytes, nesting 16. Secrets are prohibited.

The one-way persistence migration maps the persisted selected screen to one current instance with empty stack, generation 1, clean state, and compiled activation defaults; no old screen-identity type participates at runtime. Stack, drafts, subscriptions, provider models, and modal state never persist.

## Complete flow, failures, and security

Action resolution produces a route activation from the current immutable snapshot; reducer validates route/schema/source/generation; Push/Replace/Back transition commits; state access is released; subscription/disposal/save effects execute; matching completion advances state. Invalid route, stale source, stack overflow, effect failure, or disposal failure never half-mutates navigation. Recovery is Retry/Discard/Cancel for dirty save and Back/current screen for invalid activation. No external owner can construct undeclared routes, inject secrets, reuse an instance, or bypass the host dirty guard.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Pull Request 42 ----------+ + Pull Request 42 ----------+
| Files  Conversation       | |>>Conversation             |
| q Back                    | | q Back                    |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Navigation ---------------+ + Navigation --------------+
| Route unavailable         | | invalid activation number|
| current screen retained   | | [Back] current retained  |
+---------------------------+ +---------------------------+
```
```text
DIRTY                          RECOVERY
+ Unsaved changes ----------+ + Save failed -------------+
|>>Save  Discard  Cancel    | | draft retained           |
| Tab moves; Esc cancels    | |>>Retry  Discard  Cancel  |
+---------------------------+ +---------------------------+
```
```text
SMALL
+Unsaved?-------+
|>>Save         |
| Discard       |
| Cancel        |
| Ctrl-Q Exit   |
+---------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW06-01 | WHEN Push validates, Jefe shall suspend exact current state and create a fresh target. | `typed-navigation-push-back.json`; reducer golden |
| CW06-02 | WHEN Replace validates, Jefe shall dispose old only after target construction succeeds. | replace success/failure transaction tests |
| CW06-03 | WHEN Back reaches the stack, Jefe shall restore exact prior instance and focus. | two-instance byte-equivalence fixture |
| CW06-04 | WHEN Back encounters local layers, Jefe shall unwind only the first precedence layer. | all-layers-stacked table test |
| CW06-05 | WHILE dirty Save is pending, Jefe shall defer navigation until matching successful completion. | `navigation-dirty-save.json` |
| CW06-06 | WHEN Discard or Cancel is chosen, Jefe shall respectively restore base and navigate or retain exact draft/focus. | dirty-choice matrix |
| CW06-07 | IF stack would exceed 32, Jefe shall retain state and show `NAV-E001`. | depth 32/33 property |
| CW06-08 | IF completion identity is stale/suspended/disposed, Jefe shall change nothing. | generation property |
| CW06-09 | WHEN persisted screen state migrates, Jefe shall create one clean current instance and no stack. | migration matrix |
| CW06-10 | WHEN each UI state renders, Jefe shall preserve protected Back/exit and accessible modal focus. | six distinct harness scenarios |

RED tests first, GREEN one reducer, REFACTOR deletes mode-specific cross-screen mutations and any pre-`NavState` screen-switching path after parity — one navigation authority remains, with the shim-token scan clean per the epic no-shim policy.

## Normative documentation and done

Update `dev-docs/standards/architecture.md` with route ownership, activation and generation invariants; update `dev-docs/standards/display-and-ui.md` with Back precedence, dirty modal keys/focus, and small-terminal behavior; update `dev-docs/standards/persistence-and-runtime.md` to state navigation is nonpersistent. Done requires all existing Esc/q behavior, exhaustive intent matching, ledger tests, superseded navigation paths deleted, and unchanged `make ci-check`; no new dependency, generic navigation payload, unsafe, unwrap/expect, suppression, or weakened gate.