# i2pr-mail Active Planning Registry

This file is the compact control surface for active interim planning. Detailed requirements remain in canonical documents, subsystem roadmaps, implementation plans, closure records, and Git history.

Canonical direction:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md
- plans/003-planning-process.md

## Status vocabulary

- proposed — exists but not approved for execution.
- ready — dependencies/interfaces satisfied; may be handed off.
- active — implementation in progress.
- blocked — named dependency/evidence requirement prevents progress.
- closing — implementation landed; closure evidence being gathered.
- closed — accepted closure record.
- conditionally closed — implementation substantially landed but named correctness/operational evidence remains.
- superseded — replaced by another document.
- archived — historical only.

## Active subsystem roadmaps

| Subsystem | Status | Roadmap | Current milestone | Dependencies or blockers |
|---|---|---|---|---|
| Mail backend foundation | active | plans/subsystems/mail-backend-foundation-roadmap.md | M001 ready | M001 has no hard dependency; later milestones sequence from it. |

## Dependency-ready implementation plans

| Subsystem | Milestone | Status | Plan | Why ready |
|---|---|---|---|---|
| Mail backend foundation | M001 workspace/domain/boundaries | ready | plans/implementation/mail-backend-foundation/001-workspace-domain-and-boundary-foundation.md | New repository; canonical product/security decisions are frozen by ADR-0001. |

## Active closure work

None.

## Blocked work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Mail backend foundation | M002 MIME + durable storage | blocked | plans/implementation/mail-backend-foundation/002-mime-and-durable-storage.md | M001 closure. |
| Mail backend foundation | M003 POP3 receive/sync | blocked | plans/implementation/mail-backend-foundation/003-pop3-receive-sync-and-reconciliation.md | M001 and M002 closure. |
| Mail backend foundation | M004 SMTP compose/outbox | blocked | plans/implementation/mail-backend-foundation/004-smtp-compose-outbox-and-submission.md | M001 and M002 closure. |
| Mail backend foundation | M005 backend convergence | blocked | plans/implementation/mail-backend-foundation/005-backend-service-convergence.md | M003 and M004 closure. |
| Mail backend foundation | M006 i2pr adapter | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | M005 closure plus stable corrected i2pr managed-app transport/gateway; current Plan-345 contract alone is insufficient and Plan 349 must close first. |

## Recently closed or conditionally closed work

None. Repository planning bootstrap is not an implementation milestone closure.

## Deferred unregistered product work

- graphical/desktop frontend and toolkit selection;
- HTML renderer and remote-content presentation policy beyond the backend no-fetch invariant;
- background polling/notification scheduler beyond explicit backend operations;
- generic IMAP;
- Gmail/Exchange/provider OAuth;
- conventional clearnet POP3/SMTP;
- contacts/calendar synchronization;
- OpenPGP/S/MIME product/key-management flows;
- full-text indexing/search;
- attachment metadata sanitization;
- multi-provider/service discovery;
- app-store/package/update UX owned outside the mail backend.

## Registration/unblock rule

When a milestone closes, audit the Blocked work table and source roadmap dependency graph. Move a downstream plan to ready only when all hard dependencies are closed and every interface dependency is stable. Record the change in the closure record and registry in the same planning update.