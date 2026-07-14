Resolved the current OCR findings against the latest merged source.

Implemented:
- Fixed dead cancellation ordering in PR repository navigation, Issue mode entry/reload/filter/search transitions, PR/Issue list replacement, list previews, and Issue deletion. Pending comment requests are now canceled before their detail container is replaced or dropped, and detail-removal paths clear comment loading.
- Added monotonic comment-request high-water marks across replaceable Issue and PR detail snapshots. A stale callback from an old detail can no longer collide with request ID 1 from a newly assigned detail.
- Preserved newer in-flight Issue comment requests when an older failure arrives; added a navigation-away/back regression that verifies the old request is stale while the replacement request remains loading.
- Made Issue comment token conversion explicitly cursor-only and added PageNumber rejection coverage.
- Reworked the PR no-detail result test to assert the stale event is ignored and does not clear unrelated state.
- Made silent-refresh fixtures use the actual repository identity.
- Canonicalized terminal fixture tokens to PageToken::Done.
- Reworked the shifted-comment mutation test so a real insertion moves the edited comment and comment_id remains authoritative.
- Added regression coverage for detail/list deletion and replacement cleanup.

Already fixed and confirmed:
- replace_items clamps selection.
- Issue comment page success has the current repo/detail guard.
- PR correlated stale failures only surface errors when accepted.
- Unbound GitHub comment lists are rebound to stable identities in Issue detail load, PR detail load, and silent PR detail refresh.

Evaluated as invalid or no-action:
- Blanket Clone + PartialEq bounds on PaginatedList would unnecessarily restrict item-only APIs; the compile-time unbounded-identity test documents the intentional segmented impl bounds.
- Clearing loading on AcceptOutcome::Stale is unsafe because it would hide a newer correlated request. Detail-removal transitions now explicitly own orphan cleanup.
- PageToken::Done and PageToken::from_cursor(None, false) are semantically identical; fixtures now use Done for clarity.
- expect() is forbidden by the project Clippy policy, so let-else panic remains the test convention.
- cancel_pending fully owns the retired detail container's pending correlation; the new state-level high-water mark additionally prevents request-ID reuse after detail reassignment.
- PrCommentsPageDispatchFailed is intentionally only for failures before a request starts. Network, parsing, context, and panic failures carry the allocated request ID in PrCommentsPageFailed and route through the same PR load-error branch.
- The UI render thread reported no code issue.

Verification on the final tree:
- fmt, source-size, architecture, Clippy policy: pass
- strict Clippy and complexity Clippy: pass
- locked build: pass
- tests: 1,794 + 574 + 6 + 267 + 2 = 2,643 passed, 0 failed
- coverage: 72.48% lines (30% required)
