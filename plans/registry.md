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
| Mail backend post-M005 corrective | active | plans/subsystems/mail-backend-post-m005-corrective-addendum.md | M007 ready | M001–M005 remain closed; M007 corrects request/state/protocol defects before router integration. |
| Mail backend foundation | active | plans/subsystems/mail-backend-foundation-roadmap.md | corrective M007 ready; M006 blocked | M006 additionally requires M007–M009 closure and stable upstream i2pr Plan 355 app-principal gateway. |

## Dependency-ready implementation plans

| Subsystem | Milestone | Status | Plan | Why ready |
|---|---|---|---|---|
| Mail backend post-M005 corrective | M007 request/state/protocol correctness | ready | plans/implementation/mail-backend-foundation/007-request-state-protocol-corrective.md | M001–M005 are closed; findings are local and require no external interface. |

## Active closure work

None.

## Blocked work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Mail backend post-M005 corrective | M008 runtime decomposition + transport seam | blocked | plans/implementation/mail-backend-foundation/008-runtime-decomposition-and-transport-seam.md | M007 closure. |
| Mail backend post-M005 corrective | M009 hosted verification + corrective closure | blocked | plans/implementation/mail-backend-foundation/009-hosted-verification-and-corrective-closure.md | M008 closure. |
| Mail backend foundation | M006 i2pr adapter | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | i2pr-mail M007–M009 closure plus upstream i2pr Plan 354 → Plan 355; Plan 355 owns the router app-principal SAM/I2CP gateway. |

## Recently closed or conditionally closed work

| Mail backend foundation | M001 workspace/domain/boundaries | closed | plans/closure/mail-backend-foundation/001-status.md | — |
| Mail backend foundation | M002 MIME + durable storage | closed | plans/closure/mail-backend-foundation/002-status.md | — |
| Mail backend foundation | M003 POP3 receive/sync | closed | plans/closure/mail-backend-foundation/003-status.md | — |
| Mail backend foundation | M004 SMTP compose/outbox | closed | plans/closure/mail-backend-foundation/004-status.md | — |
| Mail backend foundation | M005 backend convergence | closed | plans/closure/mail-backend-foundation/005-status.md | — |

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
