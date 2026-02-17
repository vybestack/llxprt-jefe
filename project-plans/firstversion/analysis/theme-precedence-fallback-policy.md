# Theme Precedence and Fallback Policy

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Mandatory Baseline

- Default theme: `green-screen`
- Fallback theme: `green-screen`
- No bright/light default palette in v1

## Resolution Precedence

1. Explicit user-selected theme in current session
2. Persisted theme in `settings.toml`
3. Built-in default (`green-screen`)

If any selected/persisted theme is missing/invalid -> fallback to `green-screen`.

## Token-Level Fallback

When partial theme definitions are missing tokens:
1. Resolve provided tokens from selected theme.
2. Fill missing tokens from green-screen canonical token set.
3. If theme parse fails entirely, use full green-screen token set.

## Acceptance Checks

- THEME-001: startup with no settings -> green-screen active
- THEME-002: invalid persisted slug -> green-screen fallback + warning
- THEME-003: malformed external theme -> green-screen fallback
- THEME-004: partial token theme -> missing tokens sourced from green-screen

## Verification Mapping

- P12: implementation and tests
- P12A: verification
- P14: final no-regression gate
