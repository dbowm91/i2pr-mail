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
| Mail backend foundation | active | plans/subsystems/mail-backend-foundation-roadmap.md | M011 closed; M006 blocked | M001–M005 and M007–M010 are closed and were integrated to `main` by history-preserving fast-forward in M011; the work branch `codex/foundation-planning` is retired. No dependency-ready local implementation work remains. M006 is the only active capability plan and remains blocked on the upstream app-side managed-app runtime. |

## Dependency-ready implementation plans

None. Every locally executable milestone is closed; M006 is blocked on an external upstream dependency.

## Active closure work

None.

## Blocked work

| Subsystem | Milestone | Status | Plan | Blocker |
|---|---|---|---|---|
| Mail backend foundation | M006 i2pr adapter | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | One condition remains. Its local gate is satisfied: all foundation and corrective work through M010 is closed, and M011 integrated that qualified work to `main`, so M006 starts from mainline authority rather than a retired work branch. The remaining blocker is upstream reachability, re-audited 2026-10-06 against i2pr `main` `144c54da`: Plan 355's router-side gateway is not an app runtime/channel — `AppGatewaySession`, `AppGatewayAuthorization`, `AppGatewayLimits`, and `AppGatewayComposition` remain `pub(crate)` in `crates/i2pr-daemon/src/app_gateway.rs`, referenced nowhere else in the repository, under a `#![allow(dead_code)]` reading "No production app-runtime caller exists yet" — and the package/lifecycle + AppManager successor remains unregistered and eligible only for a future plan. M006 cannot execute until that upstream app-side milestone is planned and closed. |

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
