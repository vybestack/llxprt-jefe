# Component 003 Pseudocode — Persistence + Theme Resolution

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

Requirements: REQ-FUNC-001,009,010; REQ-TECH-005,007,009

```text
01: FUNCTION resolve_paths(env, os)
02:   settings_path = env.JEFE_SETTINGS_PATH
03:     ?? join(env.JEFE_CONFIG_DIR, "settings.toml")
04:     ?? os_default_settings_path(os)
05:   state_path = env.JEFE_STATE_PATH
06:     ?? join(env.JEFE_STATE_DIR, "state.json")
07:     ?? os_default_state_path(os)
08:   return { settings_path, state_path }

09: FUNCTION load_startup(env, os)
10:   paths = resolve_paths(env, os)
11:   settings = read_settings_toml_or_default(paths.settings_path)
12:   state = read_state_json_or_default(paths.state_path)
13:   validate settings schema/version
14:   validate state schema/version
15:   if invalid -> record warning + fallback safe defaults
16:   sanitize cross-references (repo/agent/runtime bindings)
17:   return canonical app bootstrap payload

18: FUNCTION read_settings_toml_or_default(path)
19:   if file missing -> return defaults
20:   parse toml
21:   if parse fails -> return defaults + warning

22: FUNCTION read_state_json_or_default(path)
23:   if file missing -> return defaults
24:   parse json
25:   if parse fails -> return defaults + warning

26: FUNCTION save_settings_atomic(settings, path)
27:   ensure parent dir exists
28:   serialize toml
29:   write temp file same filesystem
30:   fsync temp
31:   rename temp -> path
32:   return success/failure

33: FUNCTION save_state_atomic(state, path)
34:   ensure parent dir exists
35:   serialize json
36:   write temp file same filesystem
37:   fsync temp
38:   rename temp -> path
39:   return success/failure

40: FUNCTION resolve_theme(slug, sources)
41:   if slug found and valid -> return resolved
42:   else return built-in green-screen resolved palette
43:   fill missing tokens from green-screen fallback tokens
44:   persist selected/normalized active slug
```
