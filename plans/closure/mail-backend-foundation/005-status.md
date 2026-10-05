# Mail Backend Foundation M005 Closure — Backend Service Convergence

Status: closed

Closed: 2026-10-05

## Outcome

The backend service convergence milestone is implemented. `BackendService` provides one mutable owner for startup recovery, deterministic commands, request deduplication, asynchronous operation events, bounded opaque content handles, explicit shutdown, and health/profile inspection. Credentials enter only through the ephemeral credential source. POP3, SMTP, storage, and domain details remain behind backend-facing interfaces.

The end-to-end service scenario covers POP3 synchronization, message listing/open/read, draft create/delete, outbox queue and send, Sent visibility, request ID rejection, content read bounds, handle release/staleness, local delete persistence, and restart recovery of an interrupted submission as `DeliveryUnknown`. Sending remains blocked until an explicit delivery-ambiguity resolution decision.

## Closure evidence

| Requirement | Evidence |
|---|---|
| Stable backend command/event/lifecycle surface | `crates/mail-runtime/src/lib.rs`: `BackendService`, `BackendRequest`, `BackendEvent`, `OperationResult`, health and shutdown methods. |
| Request, content, and resource bounds | Request IDs are validated and deduplicated; active/retained content and read sizes are bounded; released handles fail closed. Covered by runtime tests. |
| Secret boundary | Credentials are acquired as ephemeral `Credentials`; no serialization or debug formatting path is provided. Existing secret-sentinel storage/outbox tests pass. |
| Restart and uncertain-delivery correctness | Startup recovery changes interrupted submission to `DeliveryUnknown`; end-to-end test verifies explicit `PermitRetry` is required before another send. |
| Future-plan unblock audit | M006 hard dependency M005 is satisfied, but the i2pr interface dependency is not. Current upstream registry provides only Plan 345 contract/architecture work, with no managed-app gateway or destination stream API. M006 remains blocked; no other eligible plans remain. Evidence and links are recorded in M006. |

## Verification

Commands run from the repository root:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets` — passed, 38 tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed, including the forbidden-dependency positive control.
- `bash scripts/verify.sh quick` — passed.

No known high- or medium-severity correctness issue remains open for this milestone. Networked i2pr integration is outside M005 and remains blocked by the upstream interface dependency.

## Planning updates

- M005 plan status: closed.
- Registry and source roadmap identify M001–M005 as closed.
- M006 remains blocked with its current external evidence recorded; it is not marked complete and has no closure record.
