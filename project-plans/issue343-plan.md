# Issue 343: supported first-agent tutorial regeneration

## Objective

Turn the issue 241 capture primitive into a discoverable maintainer workflow that
builds or validates the real Jefe and harness binaries, runs the existing
isolated first-agent scenario, promotes only the tutorial's selected SVGs, and
records enough provenance to detect stale committed assets.

## Acceptance matrix

| Row | Actor / launch path | Input and boundaries | Observable success | Observable failure / diagnostics | Permitted side effects | Proof |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Maintainer runs the documented regeneration command from a checkout | Absolute nonexistent run root; default binaries are built from the current checkout | The real first-agent scenario completes and selected publication-safe SVGs are promoted to `docs/assets` | Missing tools, build failures, invalid binaries, and scenario drift fail with a non-zero status and retained run diagnostics | Build under `target`, create one requested run root, replace only selected tutorial assets and provenance | Wrapper contract tests plus one real regeneration run |
| A2 | Maintainer relies on the workflow's isolation | Existing issue 241 capture boundary: isolated HOME, config, socket, fixture repository, deterministic LLxprt shim, and local tmux session | Normal Jefe state, user configuration, credentials, unrelated repositories, and remote resources are not read or modified | Unsafe publication content or invalid root is refused by the existing capture contract | Run-root files and selected committed docs assets only | Existing capture safety tests plus wrapper delegation assertions |
| A3 | Maintainer promotes generated output | Fixed allowlist of three semantic capture labels | Exactly the selected generated SVG bytes are copied without manual SVG editing | Missing or unsafe generated assets prevent promotion | Replace three `docs/assets/first-agent-*.svg` files and one provenance file | Contract test compares promoted bytes and rejects incomplete output |
| A4 | Reviewer or verifier checks committed tutorial assets | Provenance records source commit/version, source-contract fingerprint, and generated asset object IDs | Check command succeeds when sources and assets match provenance | Source-contract or asset changes produce a clear stale diagnostic and non-zero status | Read-only repository inspection | Contract tests mutate source and asset fixtures and prove rejection |
| A5 | Maintainer cleans a completed or failed run | Documented issue 241 manifest-scoped dry-run then confirmed cleanup | Only run-owned temporary paths are removed; evidence, publication, manifest, and committed assets remain | Invalid sentinel, manifest, symlink, or ownership entry refuses cleanup | Existing manifest-listed paths beneath the run root only | Existing cleanup tests and maintainer documentation |

## Explicit non-goals

- No automatic prose generation or edits to tutorial prose.
- No authenticated GitHub access, remote mutation, or network fixture.
- No runtime matrix, Windows publication path, CMS, generalized capture API, or
  revival of closed PR 279.
- No changes to the TUI, harness scenario semantics, dependencies, CI workflows,
  quality gates, or agent configuration.
- No promotion of unselected intermediate captures.

## Vertical slices

### Slice 1: maintainer regeneration and promotion contract

- Rows: A1-A3, A5.
- Boundary: one Unix maintainer wrapper over the existing issue 241 capture
  primitive; no production Rust architecture changes.
- RED: wrapper contract tests require explicit binary validation, successful
  allowlisted promotion, and refusal when publication output is incomplete.
- GREEN: add a first-agent-named script with a default build path and an explicit
  validated-binary path for deterministic contract tests.
- Allowed paths: `scripts/regenerate-first-agent-tutorial.sh`,
  `tests/first_agent_tutorial_regeneration.rs`,
  `dev-docs/testing/tmux-harness.md`.
- Stop if implementation requires a generalized capture framework, dependency,
  workflow, or harness/runtime behavior change.

### Slice 2: provenance and staleness verification

- Rows: A3-A4.
- Boundary: deterministic repository/file hashing and a committed text
  provenance record owned by the maintainer script.
- RED: contract tests prove source-contract and promoted-asset mutations fail
  verification with actionable diagnostics.
- GREEN: record source commit/version, a bounded source-contract fingerprint,
  and each selected asset object ID; add a read-only check command.
- Allowed paths: the Slice 1 files plus
  `docs/assets/first-agent-tutorial.provenance` and the three selected SVGs.
- Stop if a self-modifying or repository-wide generated-content system is
  required.

## Expected paths and ownership

- Maintainer orchestration boundary: `scripts/regenerate-first-agent-tutorial.sh`.
- Existing bounded capture dependency: `scripts/issue241-capture.sh` (unchanged
  unless a demonstrated wrapper integration defect requires a narrow fix).
- Behavioral contract: `tests/first_agent_tutorial_regeneration.rs` and its
  bounded fake-capture fixture under `tests/fixtures/first_agent_tutorial/`.
- Discoverability and cleanup: `dev-docs/testing/tmux-harness.md`.
- Generated publication output: three existing SVGs and one provenance file in
  `docs/assets/`.
- Delivery evidence: this plan.

Target: at most 8 changed files and below 800 net changed lines.

## Scope ledger

| Discovery | Disposition | Reason |
| --- | --- | --- |
| Existing issue 241 script already owns isolation, publication validation, diagnostics, and cleanup | Reuse unchanged | The supported wrapper should compose the proven bounded primitive rather than duplicate or generalize it |
| Exact commit IDs are self-referential if verification requires generated assets to equal the commit that contains them | Record plus fingerprint | Record the source commit/version for provenance, while staleness verification compares a deterministic fingerprint of bounded source inputs and object IDs of selected assets |
| Wrapper tests need to avoid rebuilding and running a real TUI for every contract branch | In-scope validated-binary path | Acceptance explicitly permits building or validating required binaries; explicit paths preserve production validation and enable deterministic fake binaries |
| PR OCR identified duplicated command construction, subprocess chmod, incomplete subprocess diagnostics, permissive fake argument parsing, and delegated absolute-root validation | In-scope—Fix | Centralized fixture invocation, used Unix permission APIs, included both output streams in assertion diagnostics, made the fake capture parser fail closed, and added wrapper-level absolute-root validation with behavioral coverage |
| PR OCR recommended adding timeout process management to the integration-test command helper | Reject | The referenced helper is private production code, no test timeout dependency exists, and adding a new process-management/termination subsystem is an explicit issue-workflow stopping condition outside this documentation contract |

No unapproved scope changes.

## Review counters

- Open Code Review before PR: 2 / 2 attempted; both external OCR invocations
  were terminated without producing output, so no findings were available to
  triage.
- Open Code Review after PR: 2 / 2; five findings were classified
  In-scope—Fix and remediated, and one timeout-subsystem recommendation was
  rejected under the bounded-workflow stopping rules.

## Verification evidence

- RED: all three regeneration contract tests failed because the supported
  maintainer script did not exist.
- Focused GREEN: three regeneration, incomplete-publication, and staleness
  contract tests pass; shell syntax and focused Clippy pass.
- Real TUI regeneration: the canonical build path completed from a fresh
  platform temporary root, ran the existing first-agent scenario, reproduced
  all three committed SVGs byte-for-byte, and wrote verifiable provenance for
  Jefe 0.0.29 at source commit `0cab70a159b8475e59b445c14876a1ab192597e4`.
- Cleanup dry-run listed only the six manifest-owned runtime paths and preserved
  evidence, publication assets, and the manifest.
- `make quick-check`: passed, including all workspace tests.
- `make ci-check`: passed at the candidate head, including format, policy,
  source-size, both Clippy gates, 30% coverage gate, locked build, locked tests,
  and doctests.
- `scripts/regenerate-first-agent-tutorial.sh check`: passed after promotion.
- Initial PR CI completed except one transient native-Windows psmux readiness
  timeout; the failed job log identified `puppy-four`, and the unchanged exact
  head passed the complete native-Windows job on rerun.
- Review-remediation head CI: all required checks passed after rerunning two
  native-Windows attempts that failed in different existing guarded psmux/TUI
  readiness timeouts; the same head then passed the complete Windows job.
- Final absolute-root remediation head CI: pending.

## Deferred findings / follow-ups

None.
