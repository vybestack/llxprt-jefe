# Issue #554 — `version[nightly]` cache never refreshes

> when an agent has version[nightly] it is caching it under that name and never
> updating to the latest nightly.

## Root cause

The active production install cache is `src/runtime/package_runtime.rs`, reached
on every versioned launch via `launch_compose::prepare_launch → local_candidate
→ finalize_local_invocation`. `cache_hit` treats an install as permanent once
the `.jefe-installed` marker string matches the selector's **effective** dist-tag
(e.g. `nightly` / `latest`) and the binary exists.

Because the effective dist-tag is a constant, the marker **always** matches, so
`npm install` never re-runs and the dist-tag is never re-resolved. Worse,
`npm install` writes a `package-lock.json` that pins the resolved version, so
even a forced re-install would reuse the stale resolution. Result: an agent on
`version[nightly]` is frozen on the first nightly it ever downloaded.

(`src/runtime/llxprt_install.rs` carries the identical latent bug, but its
public entry points (`ensure_installed`, `local_managed_bin_dir`, …) have **no
production callers** — the generalized `package_runtime.rs` superseded it. It is
recorded as a deferred follow-up, not modified here.)

## Fix

A version selector is **volatile** when it resolves to a moving dist-tag
(`Latest`, `LatestNightly`). For volatile selectors the cache must periodically
re-resolve:

1. The marker gains an install-time epoch line for volatile selectors
   (`package\nbinary\neffective\n<epoch_secs>\n`). Pinned selectors keep the
   unchanged 3-line marker (permanent hit — explicit versions are immutable).
2. `cache_hit` accepts a `now: SystemTime`. For volatile selectors an absent or
   TTL-expired timestamp line is a cache **miss** (re-resolve). Old 3-line
   markers (no timestamp) auto-heal: they are treated as expired and reinstalled,
   writing a fresh timestamped marker.
3. On a volatile re-resolve, the stale `package-lock.json` is removed so
   `npm install` re-resolves the dist-tag against the registry instead of
   reusing the locked (old) version.
4. `VOLATILE_SELECTOR_TTL = 12h` (nightlies publish daily; ~half the cadence
   bounds staleness to ≤ 12h without hitting the registry on every launch).

## Acceptance matrix

| # | Actor / path | Input | Success behavior | Failure / diagnostic | Test |
|---|---|---|---|---|---|
| AC1 | `finalize_local_invocation` for a volatile (`latest nightly`) selection | cache marker timestamp older than TTL | cache treated as **miss** → `npm install` re-runs, fresh timestamp written | — | `volatile_cache_re_resolves_after_ttl` |
| AC2 | same, marker timestamp within TTL | cache **hit** → `npm install` not invoked | — | `volatile_cache_stays_fresh_within_ttl` |
| AC3 | volatile selection, old 3-line marker (no timestamp) | legacy/stuck cache | treated as **miss** → reinstalled, timestamped marker written (auto-heal) | — | `volatile_old_marker_without_timestamp_re_installs` |
| AC4 | pinned (`Explicit`) selection, any age | permanent cache marker | cache **hit** regardless of age → `npm install` not invoked | — | `pinned_cache_remains_permanent_hit` |
| AC5 | volatile re-resolve path | stale `package-lock.json` present | lockfile removed before `npm install` so dist-tag re-resolves | — | `volatile_re_resolve_removes_stale_lockfile` |
| AC6 | `VersionSelector::is_volatile` | `Latest`/`LatestNightly` | `true`; `Direct`/`Explicit` → `false` | — | `agent_candidate_tests` |

## Non-goals

- Modifying the unused `llxprt_install.rs` (deferred follow-up: same latent bug,
  no production callers).
- Changing remote launches (remote uses `npm exec` structural argv, no jefe
  managed cache on the remote host — unchanged).
- Configurable / per-agent TTL (single named constant is sufficient).
- Resolving/`npm view` lookups before install (rely on `npm install`'s own
  dist-tag re-resolution once the lockfile is removed).
- UI / "upgrade detection" changes (the symptom is the cache; fixing the cache
  removes the stale-upgrade-offer loop).

## Vertical slices

1. **Domain**: `VersionSelector::is_volatile()` + unit tests.
2. **Runtime cache freshness**: marker timestamp, `cache_hit(now)`, lockfile
   removal, `finalize_local_invocation_at` testable seam. RED tests first.

## Scope ledger

| Item | Status |
|---|---|
| `src/agent_candidate.rs` (is_volatile + tests) | planned |
| `src/runtime/package_runtime.rs` (TTL/marker/cache_hit/lockfile/seam) | planned |
| `src/runtime/package_runtime_tests.rs` (RED→GREEN) | planned |
| `src/agent_candidate_tests.rs` (is_volatile) | planned |
| `project-plans/issue554-plan.md` (this doc) | added |
| `llxprt_install.rs` same-class fix | **deferred** (dead code) |

Review counters: OCR local pre-PR 0/2, OCR post-PR 0/2.
