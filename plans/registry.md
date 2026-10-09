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
| Mail backend post-M005 corrective | closed | plans/subsystems/mail-backend-post-m005-corrective-addendum.md | M009 closed | M007-M009 all closed; hosted verification green on run 37376165523. |
| Mail backend foundation | active | plans/subsystems/mail-backend-foundation-roadmap.md | M012 ready; M006 blocked | M001–M005 and M007–M011 are closed on main. The old M006 reachability blocker is superseded by upstream Plans 368–371 and 382–383, which now provide the real managed-app process/channel and production launch authority. M012 is the dependency-ready local corrective for managed-app framing, SAM session topology, and port-aware transport. |

## Dependency-ready implementation plans

| Subsystem | Milestone | Status | Plan | Why ready |
|---|---|---|---|---|
| Mail backend foundation | M012 managed-app wire + port-aware SAM transport corrective | ready | plans/implementation/mail-backend-foundation/012-managed-app-wire-and-port-aware-sam-transport-corrective.md | The managed-app application wire/runtime is concrete upstream through Plans 368–371 and 382–383, and the SAM 3.2/3.3 client grammar is stable enough for deterministic local implementation. M012 does not require a live router or a qualified OS sandbox. |

## Active closure work

None.

## Blocked work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Mail backend foundation | M006 i2pr adapter | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | The former app-runtime reachability blocker is cleared upstream: i2pr Plans 368–371 and 382–383 now supply the inherited manager/apphost process chain, managed-app v1 consumer, persistent grants/catalog, and real private SAM/I2CP streams. Three gates remain: local M012 must close the application-wire/SAM-client corrective; upstream SAM/368 must close port-aware SAM 3.2+ FROM_PORT/TO_PORT semantics because Postman uses nonzero I2P ports; upstream Managed native app runtime/385 must close a qualified `Secured` backend because M006 forbids `UnsafeDirect` and localhost fallbacks. |

## Recently closed or conditionally closed work

| Mail backend foundation | M001 workspace/domain/boundaries | closed | plans/closure/mail-backend-foundation/001-status.md | — |
| Mail backend foundation | M002 MIME + durable storage | closed | plans/closure/mail-backend-foundation/002-status.md | — |
| Mail backend foundation | M003 POP3 receive/sync | closed | plans/closure/mail-backend-foundation/003-status.md | — |
| Mail backend foundation | M004 SMTP compose/outbox | closed | plans/closure/mail-backend-foundation/004-status.md | — |
| Mail backend foundation | M005 backend convergence | closed | plans/closure/mail-backend-foundation/005-status.md | — |
| Mail backend foundation | M007 request/state/protocol corrective | closed | plans/closure/mail-backend-foundation/007-status.md | — |
| Mail backend foundation | M008 runtime decomposition + transport seam | closed | plans/closure/mail-backend-foundation/008-status.md | — |
| Mail backend post-M005 corrective | M009 hosted verification + corrective closure | closed | plans/closure/mail-backend-foundation/009-status.md | — |
| Mail backend foundation | M010 SAM 3.1 client codec + contract qualification | closed | plans/closure/mail-backend-foundation/010-status.md | — |
| Mail backend foundation | M011 foundation branch integration/merge/cleanup | closed | plans/closure/mail-backend-foundation/011-status.md | — |

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
