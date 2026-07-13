# Issue #269 — Launch selectable LLxprt versions with npm exec

## Goal

Add an LLxprt-only `Version` field to agent create/edit forms and an LLxprt-only `Default Version` field to repository create/edit forms. A blank selector preserves direct/resolved `llxprt`; a nonblank selector launches the same LLxprt arguments through:

```text
npm exec --yes --package=@vybestack/llxprt-code@VERSION -- llxprt ...
```

The repository value is copied into newly opened agent forms. It is not dynamically inherited by existing agents.

## Architecture

1. Model the normalized selector as a serde-transparent domain value with blank as the direct-launch invariant. Add defaulted fields to `Repository`, `Agent`, and `LaunchSignature`; do not bump the persistence schema.
2. Keep editable form values as strings. Normalize at create/update boundaries. LLxprt fields render and participate in focus traversal only when the selected runtime is LLxprt; runtime switching preserves dormant values.
3. Represent executable selection as a target-neutral runtime command plan. Direct LLxprt and Code Puppy remain direct plans. Versioned LLxprt becomes structural argv beginning with `npm exec --yes --package=... -- llxprt`; all existing profile/mode/continue/sandbox/debug behavior remains shared.
4. Materialize local launches as distinct argv tokens. Materialize remote launches through the existing shell-escaping boundary, escaping every program/argument token. A versioned remote launch requires `npm` and bypasses global/path-local LLxprt resolution and Setup Env Default. Blank remote launches keep current resolution/setup behavior.
5. Make availability checks selector-aware where needed so local and remote versioned launches require npm rather than a preinstalled LLxprt. Preserve all pre-side-effect issue/PR/relaunch gates and carry the selector through restart/runtime bindings and fresh prompt transformations.
6. Keep UI pure, state transitions deterministic, process orchestration in runtime modules, and persistence I/O unchanged.

## Test-first sequence

1. Add a deterministic TUI harness scenario and fixture that exercise `Default Version` and `Version`, including hiding/skipping them after switching to Code Puppy. Run it before production changes and record the expected RED failure.
2. Add domain and persistence RED tests for normalization, round trips, legacy missing-field defaults, and nested runtime binding signatures.
3. Add repository/agent form RED tests for typing/cursors, create/edit normalization, copy-on-create, non-retroactivity, runtime switching, hidden-field focus traversal, and selection-content projection.
4. Add runtime command-plan RED tests for blank direct behavior, stable and nightly selectors, preservation of every existing LLxprt argument/environment, Code Puppy exclusion, local token integrity, and remote metacharacter quoting.
5. Add availability/probe RED tests proving versioned local/remote LLxprt requires npm, remote versioned launches do not probe/install LLxprt, and blank launches retain existing direct/setup behavior.
6. Add signature/relaunch/startup/issue-send/PR-send tests proving the selected version survives all launch paths.
7. Implement the smallest production changes to make each slice green, extracting modules before source-size/complexity limits are exceeded.
8. Run the TUI scenario to GREEN, then `make quick-check`, `make ci-check`, rustreviewer, and detached Open Code Review. Remediate all valid High/Medium findings and rerun verification.

## Key edge cases

- Trim surrounding whitespace; whitespace-only is blank/direct.
- Preserve npm-supported selectors such as exact versions, prereleases/nightlies, and dist tags rather than requiring strict semver.
- Reject only structurally unrepresentable input such as embedded NUL before destructive runtime actions.
- Keep an adversarial selector as one local argv token and one safely quoted remote shell token.
- Code Puppy must never invoke npm because of a dormant LLxprt selector.
- Editing a repository default must not alter existing agents or their restart/send behavior.
- Reattaching a live session must not replace it solely because persisted configuration changed; the selector applies to new/restarted launches.
- Versioned package resolution failures remain visible in the retained tmux pane with npm's diagnostic.

## Verification

```sh
cargo fmt --all --check
scripts/check-clippy-allows.sh
scripts/check-source-file-size.sh
make quick-check
make ci-check
```

Run `dev-docs/tmux-scenarios/llxprt-version-fields.json` with an isolated harness config before and after implementation. The final diff must not modify `.llxprt/`, lint/complexity thresholds, or add suppression directives.
