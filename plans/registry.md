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
| Mail backend foundation | active | plans/subsystems/mail-backend-foundation-roadmap.md | M012 ready; M013 blocked; M006 blocked | M001–M005 and M007–M011 are closed on main. Upstream Plans 368–371 and 382–383 clear the old app-runtime reachability blocker. M012 is ready for the independent managed-app v1 client/multiplexer. M013 is registered behind M012 and canonical i2pr-sam M018 to compose MailTransport without a second SAM implementation. |

## Dependency-ready implementation plans

| Subsystem | Milestone | Status | Plan | Why ready |
|---|---|---|---|---|
| Mail backend foundation | M012 managed-app v1 client + multiplexer | ready | plans/implementation/mail-backend-foundation/012-managed-app-v1-client-and-multiplexer.md | The application-facing managed-app v1 runtime is concrete upstream through Plans 368–371 and 382–383. M012 implements only that independent wire/multiplexer layer and requires no live router, SAM server change, or qualified OS sandbox. |

## Active closure work

None.

## Blocked work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Mail backend foundation | M013 canonical SAM adoption + MailTransport composition | blocked | plans/implementation/mail-backend-foundation/013-canonical-sam-adoption-and-mailtransport-composition.md | Requires local M012 closure plus canonical dbowm91/i2pr-sam M018 closure/mainline integration. M018 itself is registered behind i2pr-sam M017. No live router or Secured backend is required for M013. |
| Mail backend foundation | M006 live i2pr adapter qualification | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | Local gate becomes M013 closure. Two upstream capability gates remain: i2pr SAM/368 must close port-aware SAM 3.3 FROM_PORT/TO_PORT behavior, and Managed native app runtime/386 must close a qualified `Secured` backend. `UnsafeDirect`, localhost SAM, and local I2PTunnel POP3/SMTP proxies are not substitutes. |

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
