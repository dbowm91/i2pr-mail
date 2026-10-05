# i2pr-mail Planning Process

Status: canonical

This process mirrors CodeGG planning conventions.

## Document hierarchy

Canonical specification and terminology
-> accepted ADRs
-> long-term roadmap
-> subsystem roadmap
-> milestone implementation plan
-> implementation and verification
-> closure record
-> archive

When documents conflict, preserve canonical invariants and accepted ADRs. Repository evidence may correct operational plans but must not silently redefine product/security direction.

## Status vocabulary

proposed — exists but is not approved for execution.

ready — dependencies and stable interfaces are satisfied; may be handed off.

active — implementation is in progress.

blocked — a named dependency/evidence requirement prevents progress.

closing — production work landed and closure evidence is being gathered.

closed — closure record accepted with no unresolved high/medium finding.

conditionally closed — implementation substantially landed but a named correctness/operational evidence condition remains.

superseded — replaced by another document.

archived — historical only.

## Naming

ADR: plans/adrs/ADR-NNNN-short-title.md

Subsystem roadmap: plans/subsystems/subsystem-roadmap.md

Implementation: plans/implementation/subsystem/NNN-short-title.md

Closure: plans/closure/subsystem/NNN-status.md

Milestone numbering is local to the subsystem unless a roadmap explicitly says otherwise.

## Required implementation plan sections

1. Objective
2. Why this milestone is ready
3. Current implementation evidence
4. Invariants that must not regress
5. Scope
6. Required production changes
7. Ordered work packages
8. Failure, cancellation, restart, and contention semantics
9. Compatibility and migration
10. Required tests
11. Required verification commands
12. Documentation updates
13. Acceptance criteria
14. Stop conditions
15. Closure evidence required
16. Handoff notes

## Closure rule

Code landing is not closure. A closure record must map requirements to evidence, record exact commands/results, review failure/restart/security/migration behavior, list unresolved findings by severity, and perform an unblock audit against registry.md.

Corrective work receives a new implementation plan and does not rewrite an old closure record to conceal defects.

## Registration rule

registry.md contains active roadmaps, dependency-ready plans, active closure work, blocked work with explicit blockers, recent closure references, and deferred unregistered product work. Detailed implementation requirements belong in the source documents rather than the registry.