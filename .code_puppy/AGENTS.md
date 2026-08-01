# Jefe Agent Rules

Follow the canonical bounded issue-delivery workflow in [`dev-docs/workflow/ISSUE-DELIVERY.md`](../dev-docs/workflow/ISSUE-DELIVERY.md) for every GitHub issue.

Before implementation, create a decision-complete acceptance matrix, explicit non-goals, bounded vertical slices, expected paths, and a scope ledger. Use behavioral RED → GREEN → REFACTOR and add a failing TUI scenario first for UI-visible features. Preserve the architecture, lint, complexity, safety, source-size, coverage, cross-platform, and full-verification requirements in `dev-docs/RULES.md` and `dev-docs/project-standards.md`.

Stop and request approval before adding an unplanned subsystem, public abstraction, workflow or agent-memory change, quality-tool change, dependency, unrelated refactor/test move, or behavior absent from the acceptance matrix.

Scope is functional, not numeric. There is no file-count or line-count budget: a change is in scope when it is required to deliver the issue's accepted behavior and stated done criteria. Finish the cutover the issue asks for rather than leaving a mechanism half-replaced to hit a size target, and keep unrelated work in its own issue regardless of how small it looks.

Classify review findings as Blocker—Fix, In-scope—Fix, Reject, or Defer. A valid out-of-scope suggestion belongs in a follow-up, not automatically in the current issue. Use no more than two local OCR reviews and two PR OCR reviews per issue/PR effort.

Finish when accepted behavior is proven, exact-head verification and required CI pass, required reviews are complete and triaged, the PR is conflict-free, and the scope ledger is clean. Do not continue optional hardening after that completion contract is met.
