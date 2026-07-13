# CW-12: Exact Git Merger reference package

## Outcome and consumed contracts

Ship a relocatable `com.example.git-merger` package proving contextual actions, host confirmation, generated config, persistent host-rendered status, strict provider supervision, and no host branch on plugin identity. Consume the Settings registry editors; selected static package manifest/trust/layout; exact provider envelope/lifecycle/outcomes; panel/config DTOs; current PR typed references; and host-owned navigation/confirmation. No new host capability or dependency is introduced here.

## Exact artifact and source inventory

| Path/symbol | Required responsibility |
|---|---|
| `packages/com.example.git-merger/manifest.json` | closed schema-1 declarations for version, provider, action, panel, config, permissions |
| `packages/com.example.git-merger/config-schema.json` | strategy and optional secret-reference fields |
| `packages/com.example.git-merger/provider/Cargo.toml` | provider crate using only already approved workspace dependencies |
| `packages/com.example.git-merger/provider/src/main.rs` | synchronous protocol state machine and exact merge adapter |
| `packages/com.example.git-merger/resources/README.md` | installed user description, configuration, trust and recovery |
| `tests/fixtures/packages/com.example.git-merger/` | canonical installed macOS/Linux trees and transcripts |
| release resource index | install manifest/schema/provider/resources with exact modes/hashes |
| host PR snapshot adapter | construct opaque repository/PR/current-head refs; no plugin-ID conditional |

Installed layout is `<root>/com.example.git-merger/1.0.0/{manifest.json,config-schema.json,bin/git-merger-provider,resources/README.md}`. Directory/provider/resource modes are 0755/0755/0644. Manifest references are relative, slash-separated, and remain within version root. No development path.

## Closed declarations and wire data

Manifest declares ID `com.example.git-merger`, version `1.0.0`, API/protocol 1, persistent provider `bin/git-merger-provider`, action `com.example.git-merger.merge`, panel type `com.example.git-merger.status`, config schema, and only process permissions needed for exact `git` and `gh` argv below. Action contexts contain only `github.pull-request.detail`; destructive true; timeout 600; confirmation `provider-continuation`; outcomes only request-host-confirmation, notice, refresh, replace-panel.

```text
MergeRequest={repository_ref:OpaqueRef,pull_request_ref:OpaqueRef,
 strategy:"merge"|"squash"|"rebase",expected_head_oid:Oid}
MergeContinuation={confirmation_id:Id,approved:true,values:{}}
Oid=[0-9a-f]{40}|[0-9a-f]{64}
Config={strategy:"merge"|"squash"|"rebase",gh_token?:{env:EnvName}}
```

Host constructs refs and head from the current immutable PR detail snapshot; provider cannot accept a repository path, arbitrary PR URL, command, or extra argument. Provider resolves opaque refs through the declared host context and uses argv elements, never a shell.

Exact destructive command flow after continuation:

1. Host refreshes the current PR snapshot and compares head to `expected_head_oid`; mismatch returns `HEAD_CHANGED` and does not invoke provider continuation.
2. Provider sends progress 1 `Verifying pull request head`.
3. Provider executes `gh pr view <number> --repo <owner/name> --json headRefOid,state,isDraft,mergeStateStatus` with a 30-second bound and parses closed JSON; require open, not draft, exact head, and mergeable status not `DIRTY`, `BLOCKED`, or `UNKNOWN`.
4. Provider sends progress 2 `Merging pull request`.
5. Provider executes exactly one command: merge=`gh pr merge <number> --repo <owner/name> --merge --match-head-commit <oid>`; squash uses `--squash`; rebase uses `--rebase`. No admin/auto/delete-branch flag.
6. On exit 0, provider sends progress 3 `Refreshing pull request`, then one refresh outcome for the input PR and one bounded success notice. On nonzero/timeout/malformed output, send typed error and no refresh/success.

Before terminal A, provider may verify context but performs no merge. Terminal A is request-host-confirmation with title `Merge pull request`, strategy/body including repository, PR number and expected head, label `Merge`, destructive true, empty continuation schema. Cancel starts no continuation. Confirmation expires after 5 minutes and is owner/action/context/generation/single-use bound. There is no automatic destructive retry.

Panel snapshots show only current request state/progress/error and are memory-only. Config migration is not required for schema version 1; migration UI is N/A, while generated config fields are required. Token resolves only into owning Configure and is redacted from argv capture, JSONL, stderr, diagnostics, reports, panel, and artifacts.

Limits: package/manifest/schema 1,048,576 bytes; strings 262,144; path 4,096; description 4,096; provider line 1,048,576; stderr 262,144; snapshot 524,288; progress 256; action 600 s; each `gh` child 30 s; shutdown phases 2/2/2 s.

## End-to-end, migration, recovery, security

Install disabled, trust and enable exact version in Settings, select config, restart, complete persistent handshake, and expose action only in PR detail. Invoke builds typed request; provider requests confirmation; cancel ends; confirm refreshes/rechecks head then starts fresh continuation; exact view and merge commands run; success refreshes current resource. Disable/restart removes all contributions while retaining dormant config. Head change, closed/draft/unmergeable PR, child failure, protocol failure, or timeout performs no success refresh; recovery is Refresh, Back, Disable, or an explicit new invocation after current context is loaded.

## Distinct UI states

```text
NORMAL                         FOCUSED
+ Pull request actions -----+ + Pull request actions -----+
| Merge: squash            | |>>Merge with Git Merger   |
+---------------------------+ +---------------------------+
```
```text
UNAVAILABLE                    ERROR
+ Git Merger ---------------+ + Merge failed ------------+
| unavailable: provider down| | HEAD_CHANGED             |
| Refresh or Disable        | | expected/current shown   |
+---------------------------+ +---------------------------+
```
```text
DIRTY/CONFIRMATION             RECOVERY
+ Confirm squash merge? ----+ + Git Merger recovery -----+
| expected head 01234567    | | config retained          |
|>>Merge  Cancel            | |>>Refresh Disable Back    |
+---------------------------+ +---------------------------+
```
```text
SMALL
+Confirm merge?+
| destructive  |
|>>Merge       |
| Cancel       |
| Ctrl-Q Exit  |
+--------------+
```

## Test-first EARS ledger

| ID | Singular requirement | Evidence |
|---|---|---|
| CW12-01 | WHEN package is installed, Jefe shall validate exact files/modes/hashes without a host identity branch. | relocated macOS/Linux package test and source guard |
| CW12-02 | WHEN enabled/restarted, Jefe shall expose merge only in PR detail. | context registry scenario |
| CW12-03 | WHEN invoked, Jefe shall construct only the closed current-snapshot request. | request golden and unknown-field rejection |
| CW12-04 | WHEN provider requests confirmation, Jefe shall perform no destructive command before approval. | child invocation capture |
| CW12-05 | WHEN approved with unchanged head, provider shall run exact view then one exact strategy command. | three strategy argv captures |
| CW12-06 | IF head/state/draft/mergeability differs, provider shall execute no merge and emit no success refresh. | invariant matrix |
| CW12-07 | WHEN confirmation cancels/expires/reuses, Jefe shall start no valid continuation. | zero-invocation captures |
| CW12-08 | IF child/provider fails, Jefe shall perform no automatic destructive retry. | timeout/exit/malformed fixtures |
| CW12-09 | WHEN disabled/restarted, Jefe shall remove contributions and preserve dormant config. | disable round-trip |
| CW12-10 | WHEN artifacts and observations are scanned, they shall contain no resolved secret/development path. | exhaustive scan |
| CW12-11 | WHEN each UI state renders, Jefe shall preserve host focus/confirmation/protected exit. | distinct scenarios |

RED exact package/transcripts/argv fixtures first; GREEN minimal provider; REFACTOR proves relocation and generic host composition.

## Documentation and done

Update `packages/com.example.git-merger/resources/README.md`, `dev-docs/standards/persistence-and-runtime.md` with the command/confirmation/security flow, and `dev-docs/standards/testing-and-quality.md` with reference-package relocation/secret scans. Done requires exact commands, all negative invariants, no plugin branch, and unchanged `make ci-check` plus packaging tests; no shell, guessed key, dependency, suppression, or gate weakening.