# Mail Backend Post-M005 Corrective Addendum

Status: active

Parent roadmap:

- `plans/subsystems/mail-backend-foundation-roadmap.md`

Predecessor closures:

- `plans/closure/mail-backend-foundation/001-status.md`
- `plans/closure/mail-backend-foundation/002-status.md`
- `plans/closure/mail-backend-foundation/003-status.md`
- `plans/closure/mail-backend-foundation/004-status.md`
- `plans/closure/mail-backend-foundation/005-status.md`

Applicable ADR:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`

Upstream integration authority reviewed:

- dbowm91/i2pr `work/plans-352-353`
- managed-native-app Plans 345, 349, 352, and 353 closed
- Plan 354 listener-independent SAM/I2CP private seams ready
- Plan 355 router app-principal gateway blocked on Plan 354

## 1. Corrective trigger

Post-M005 review found that the backend capability is real and the M001-M005 closure evidence remains useful, but three follow-up classes should close before i2pr integration hardens the current seams.

First, the backend request ledger is session-exhaustible: completed request IDs remain in one set until shutdown, so a long-lived service eventually refuses all further work. The store/runtime also convert typed ReceiveState and SubmissionState values back into unconstrained strings and stage integers, permitting illegal persistence combinations. Protocol parsing has smaller correctness/bounds gaps, including POP3 negative-status token validation and incomplete auth/command length ceilings.

Second, mail-runtime has become one large composition unit containing transport, POP3, SMTP, backend lifecycle/API, and tests. It is functionally valid but is the wrong physical boundary to freeze immediately before an external router adapter.

Third, all closure evidence is local. The repository has no routine hosted verification workflow even though the backend now contains substantial persistence, protocol, and recovery behavior.

Historical M001-M005 closure records remain immutable. This addendum owns these post-closure findings.

## 2. Corrective invariants

- M001-M005 behavior remains available.
- No correction adds direct clearnet, LAN, loopback, or generic host networking.
- Mail protocol crates remain sans-I/O.
- Credentials remain ephemeral and absent from ordinary persistence/logging.
- POP3 deletion and SMTP DeliveryUnknown semantics remain restart-safe.
- Store/API state uses project-owned typed domain values at Rust boundaries.
- Long-running request processing cannot require restart merely to recover request-ID capacity.
- Runtime decomposition preserves one backend lifecycle owner and one MailTransport authority seam.
- Hosted CI remains deterministic and network-independent.
- M006 does not begin until this corrective sequence closes and i2pr Plan 355 supplies the stable router gateway needed by the downstream adapter.

## 3. Corrective dependency graph

```text
M001-M005 closed
     |
     v
M007 state/request/protocol correctness
     |
     v
M008 runtime decomposition + transport-boundary stabilization
     |
     v
M009 hosted verification + corrective closure readiness
     |
     +----------------------------+
                                  |
i2pr Plan 354 -> Plan 355 --------+--> M006 i2pr adapter
```

M007 and M008 are closed. M009 is conditionally closed on its hosted verification run. M006 hard-depends on M009 and interface-depends on upstream i2pr Plans 354 and 355 closure.

## 4. Corrective milestones

### M007 — Request lifecycle, typed durable state, and protocol-bound correctness

Class: invariant + corrective capability correctness.

Correct the session-exhaustible request ledger, replace unconstrained persistent/runtime state strings with project-owned typed state at Rust boundaries, make invalid database state fail closed, and close bounded POP3/SMTP command/parser gaps.

Exit: sustained request use exceeds the retention window without permanent capacity loss; illegal durable state cannot be constructed through normal APIs; legacy schemas migrate; malformed POP3 status and over-bound auth/envelope inputs fail deterministically.

### M008 — Runtime decomposition and integration seam stabilization

Class: infrastructure + maintainability.

Physically separate transport/control, POP3 orchestration, SMTP orchestration, backend service/API, and tests while preserving the public behavior and one transport authority. Document the exact downstream adapter boundary for M006 without prematurely binding to an unreleased i2pr SDK or inventing a localhost bridge.

Exit: runtime ownership is modular, existing behavior remains green, lower crates remain router-agnostic, and M006 has one documented adapter seam plus explicit stop conditions for transport execution-model mismatch.

### M009 — Hosted verification and corrective closure gate

Class: infrastructure + polish/qualification.

Add one bounded GitHub Actions verification job that runs the repository-owned deterministic verification floor. Reconcile corrective planning after M007/M008 closure and establish hosted evidence before integration/merge.

Exit: hosted verification is green on the corrective head; no live public/I2P network dependency exists in routine CI; registry/roadmap accurately gate M006 on both local corrective closure and upstream Plan 355.

## 5. M006 upstream dependency reconciliation

The old M006 blocker text stating that upstream only had Plan 345 is stale.

At this planning baseline, i2pr's managed-native-app runtime has closed Plans 345, 349, 352, and 353. Plan 354 is the next executable router milestone and extracts listener-independent SAM/I2CP private connection drivers. Plan 355 consumes those drivers to provide the trusted AppPrincipal/effective-capability router gateway.

That means i2pr-mail M006 remains blocked on a concrete sequence:

- i2pr Plan 354 closure;
- i2pr Plan 355 closure and stable downstream service-stream contract;
- i2pr-mail M007-M009 closure.

Plan 355 maps one managed-app logical service stream to one raw SAM or I2CP protocol connection. It does not promise a pre-connected arbitrary destination byte stream. M006 must review the actual closed interface and choose the smallest adapter above MailTransport. If that requires a SAM client dependency or changes the sync/async execution model, the decision must be recorded before implementation rather than smuggled into lower mail crates.

## 6. Non-goals

This corrective line does not implement i2pr Plan 354/355, M006, a GUI, background scheduling, clearnet/provider mail, generic IMAP, live network CI, or historical closure rewrites.

## 7. Completion definition

This addendum closes when M007-M009 have accepted closure records, no high/medium corrective finding remains, hosted deterministic verification is green, and M006's remaining blocker is only the named stable upstream integration dependency (if Plan 355 has not yet closed) or M006 is legitimately ready if it has.

## 8. Status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| M007 | closed | `plans/implementation/mail-backend-foundation/007-request-state-protocol-corrective.md` | `plans/closure/mail-backend-foundation/007-status.md` | none |
| M008 | closed | `plans/implementation/mail-backend-foundation/008-runtime-decomposition-and-transport-seam.md` | `plans/closure/mail-backend-foundation/008-status.md` | none |
| M009 | conditionally closed | `plans/implementation/mail-backend-foundation/009-hosted-verification-and-corrective-closure.md` | `plans/closure/mail-backend-foundation/009-status.md` | hosted run 37372987942 must complete green; GitHub Actions outage |
