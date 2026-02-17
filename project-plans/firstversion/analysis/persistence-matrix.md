# Persistence Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Storage Artifacts

| Artifact | Purpose | Format |
|---|---|---|
| `settings.toml` | user preferences (theme + display/runtime prefs) | TOML |
| `state.json` | operational state (repos, agents, selection, runtime metadata) | JSON |

No SQLite is allowed in v1 scope.

## Exact Path Policy

### `settings.toml` resolution order
1. `JEFE_SETTINGS_PATH` (absolute file path)
2. `JEFE_CONFIG_DIR/settings.toml`
3. Platform default:
   - macOS: `~/Library/Application Support/jefe/settings.toml`
   - Linux: `${XDG_CONFIG_HOME:-~/.config}/jefe/settings.toml`
   - Windows: `%APPDATA%\jefe\settings.toml`

### `state.json` resolution order
1. `JEFE_STATE_PATH` (absolute file path)
2. `JEFE_STATE_DIR/state.json`
3. Platform default:
   - macOS: `~/Library/Application Support/jefe/state.json`
   - Linux: `${XDG_STATE_HOME:-~/.local/state}/jefe/state.json`
   - Windows: `%LOCALAPPDATA%\jefe\state.json`

### Required operational behavior
1. Path override variables must be honored exactly.
2. Parent directories must be created before first write.
3. Atomic write temp files must be created in the same directory as the target file.
4. If an override path is invalid/unwritable, the error must be surfaced clearly and app state must remain safe.

## Persistence Scope

| Data Category | Persists? | Artifact | Write Trigger |
|---|---|---|---|
| Active theme | yes | settings.toml | on theme change |
| Repo/agent definitions | yes | state.json | on CRUD submit |
| Selection/context | yes | state.json | on meaningful state transition checkpoints |
| Runtime linkage metadata | yes | state.json | on runtime lifecycle transitions |
| Ephemeral render state | no | n/a | n/a |

## Integrity Rules

1. Parse + schema/version validation before apply.
2. Malformed payload => safe defaults + surfaced warning.
3. Writes are atomic (temp file + rename strategy).
4. Failed writes do not corrupt in-memory canonical state.

## Recovery Rules

- Missing file: create defaults in-memory and continue.
- Malformed file: ignore invalid payload, keep app interactive, show warning.
- Partial valid state: sanitize references before apply.

## Verification Mapping

- P05/P12: implementation
- P05A/P12A/P14: verification and regression
