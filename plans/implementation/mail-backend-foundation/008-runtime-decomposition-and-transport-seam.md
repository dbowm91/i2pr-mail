# Mail Backend Foundation Milestone 008 — Runtime Decomposition and Transport Seam Stabilization

Status: blocked

Repository baseline: planning baseline `e1db79b55e3fdd128b9dafe701b50c5dbf5db9c5`; execute after M007 closure

Source roadmap:

- `plans/subsystems/mail-backend-post-m005-corrective-addendum.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`

Primary class: infrastructure + maintainability

## 1. Objective

Decompose the current monolithic runtime into explicit ownership modules while preserving behavior, and freeze the smallest router-integration seam needed by future M006 without binding to an unreleased i2pr SDK or introducing a localhost/direct-network bridge.

## 2. Why this milestone is ready

Blocked on M007 so typed state/request contracts stabilize before files and APIs move. No external i2pr interface is required for decomposition.

## 3. Current implementation evidence

At the baseline crates/mail-runtime/src/lib.rs is roughly 1.9k lines and owns transport/control, POP3 orchestration, SMTP orchestration, backend lifecycle/API, and runtime fixtures/tests.

MailTransport is currently synchronous. Upstream i2pr Plan 354 targets Tokio AsyncRead + AsyncWrite private SAM/I2CP seams and Plan 355 maps managed-app logical streams to those protocol connections. The final downstream SDK/channel execution model is not yet closed, so this corrective must not guess it.

## 4. Invariants that must not regress

- one MailTransport authority seam remains the only remote I/O path;
- mail-proto remains sans-I/O and Tokio-free;
- domain/mime/store remain router-agnostic;
- BackendService remains the single mutable lifecycle owner;
- POP3/SMTP restart semantics remain unchanged;
- no direct socket, DNS, localhost tunnel, SAM implementation, or i2pr internal dependency is added;
- credentials remain ephemeral;
- decomposition creates no second retry owner.

## 5. Scope

### In scope

Refactor i2pr-mail-runtime into internal modules equivalent to:

- transport — MailTransport, ByteStream, OperationControl, transport errors;
- pop3 — POP3 stream/sync/body-fetch orchestration;
- smtp — SMTP stream/submission orchestration;
- backend — request/result/event/content-handle types and BackendService;
- dedicated test modules/fixtures.

Exact filenames may vary, but ownership must remain explicit.

Add architecture documentation for the M006 adapter boundary and current upstream Plan 354/355 mapping.

### Explicitly out of scope

Changing MailTransport from sync to async, adding Tokio to lower crates, adding a blocking-thread bridge, implementing SAM/I2CP, depending on i2pr internals, M006, or semantic backend changes solely for decomposition.

## 6. Required production changes

### A. Physical decomposition

Move code without duplicating authorities. lib.rs becomes a stable composition/re-export surface rather than the implementation home for every subsystem.

### B. Test decomposition

Create reusable synthetic stream/transport fixtures and keep POP3, SMTP, backend, and transport-control tests near their owners. Preserve the M005 full fake-transport scenario.

### C. Transport integration contract

Document:

- mail runtime asks only for authorized logical mail transport;
- lower layers do not know SAM/I2CP/app framing;
- M006 adapts the closed i2pr app service stream above this seam;
- no localhost fallback is permitted;
- if the closed downstream SDK is async-only, M006 must record an architecture decision before changing MailTransport or adding a bridge;
- any SAM client dependency belongs at/above the adapter boundary.

Do not add an ADR merely to speculate before Plan 355 closes. The execution-model decision is explicitly deferred to M006 interface review.

### D. Boundary verification

Update check-boundaries only as needed after moves. Avoid line-count or filename-only quality gates.

## 7. Ordered work packages

WP1 extracts transport/control. WP2 extracts POP3/SMTP orchestration. WP3 extracts backend lifecycle/API. WP4 cleans test fixtures. WP5 records integration-boundary documentation.

## 8. Failure, cancellation, restart, and contention semantics

All semantics remain those accepted in M003-M005 and corrected in M007. Moving code must not change deadlines, cancellation, deletion reconciliation, DeliveryUnknown, outbox ownership, content limits, or request dedupe.

## 9. Compatibility and migration

No schema migration is expected. Preserve public re-exports where practical.

## 10. Required tests

Run all POP3, SMTP, M007 request/state, M005 end-to-end, cancellation/timeout, content-handle, and boundary tests. Add a dependency/source census proving no direct network/router dependency.

## 11. Required verification commands

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-runtime
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

## 12. Documentation updates

Update runtime, transport, backend API, corrective addendum, registry, and closure documentation.

## 13. Acceptance criteria

- runtime responsibilities are physically separated without duplicate owners;
- full M003-M005/M007 behavior remains green;
- lib.rs no longer owns transport + both protocol orchestrators + backend + all tests;
- no direct network path is added;
- one documented MailTransport integration seam remains;
- M006 has explicit Plan-355 evidence/stop conditions.

## 14. Stop conditions

Stop if decomposition requires semantic protocol changes, an async/sync bridge must be selected to finish, a router/SAM dependency becomes necessary, private machinery must become public, or M007 is not closed.

## 15. Closure evidence required

Before/after ownership map, public API/re-export diff, verification results, dependency/source census, M005 scenario proof, transport integration review, and M009 unblock audit.

## 16. Handoff notes

Treat this as a behavioral no-op. Do not combine M006 or async conversion with the refactor.
