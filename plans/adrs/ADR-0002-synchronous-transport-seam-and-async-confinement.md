# ADR-0002: Synchronous transport seam and async confinement

Status: accepted

Date: 2026-10-06

Decision owners: i2pr-mail maintainers

Related canonical sections:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md

Supplements (does not supersede):

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Affected roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md

Affected documents:

- docs/architecture/transport-boundary.md
- docs/architecture/runtime.md

## Context

ADR-0001 fixed `MailTransport` as a narrow synchronous seam and deliberately deferred its execution model. `docs/architecture/transport-boundary.md` then recorded a stop condition: if the closed upstream managed-app SDK exposes only asynchronous channels, an architecture decision must be recorded *before* `MailTransport` changes or any blocking bridge is added.

That condition has now been met by evidence rather than speculation. Upstream i2pr Plan 355 closed the router app-principal gateway on `main` (`2b96f1bc`), and its stream-opening surface is asynchronous:

- `AppGatewaySession::open_sam` is `pub(crate) async fn` (`crates/i2pr-daemon/src/app_gateway.rs:148`).
- `AppGatewaySession::open_i2cp` is `pub(crate) async fn` (`:194`).
- `shutdown` and `wait_closed` are `pub(crate) async fn` (`:261`, `:310`).
- The gateway yields an in-process `AppGatewayConnection` over `tokio::io` streams, not an OS handle (`:9`, `#![allow(dead_code)]`).
- The sibling SAM/I2CP drivers it consumes are likewise tokio task-based (Plan 354 closure).

So the future app channel is async-only. Upstream has deliberately not chosen the process IPC: ADR 0032 assigns it to a future user-space AppManager and declines to decide a "process protocol adapter", and the v1 contract specifies framing but states it "does not provide an app runtime, transport, sandbox, DNS resolver, or network broker."

The current seam is synchronous by construction:

```rust
fn open(&self, service: MailService, control: OperationControl)
    -> Result<Box<dyn ByteStream>, TransportError>;
```

`ByteStream` is a sync `read`/`write_all` pair, and `mail-runtime` has no async runtime, no executor, and no bridge.

## Decision drivers

- keep lower crates sans-I/O, deterministic, and replayable;
- avoid an executor dependency leaking into `mail-domain`, `mail-mime`, `mail-proto`, or `mail-store`;
- keep one transport authority and one network authority;
- preserve cancellation and deadline semantics that already exist in `OperationControl`;
- avoid unbounded thread creation, blocking an executor thread, or hiding a second network path inside a bridge;
- keep the mail protocol engines testable with scripted byte streams and no runtime at all.

## Considered options

### Move the seam to async

Rejected. `ByteStream`/`MailTransport` are consumed synchronously by the POP3 and SMTP drivers, by `BackendService`, and by M003–M009 closure evidence. Converting the seam would push an executor into `mail-runtime` and would force async into every lower-layer test. It converts a deferred, well-understood choice into a large unforced rewrite while the adapter is still blocked.

### Call blocking I/O on an async executor thread

Rejected explicitly. ADR-0001 states executor integration is a runtime composition responsibility and may not call blocking I/O on an async executor thread. A bridge that ran sync mail I/O on a tokio worker would violate that and would starve the runtime under concurrent requests.

### Keep the synchronous seam and confine async to the adapter

Accepted. `MailTransport` and `ByteStream` remain synchronous and unchanged. Async is permitted in exactly one place: the adapter crate that owns the app channel. That crate converts async channel I/O into the synchronous `ByteStream` through a **bounded** handoff.

## Decision

`MailTransport` and `ByteStream` remain synchronous. This ADR records the execution-model decision required by `docs/architecture/transport-boundary.md`.

Async is confined to the adapter crate at or above the transport seam. No crate below the seam may gain an async runtime, an executor, a blocking bridge, or an `async fn`.

Any future app channel is adapted by a **bounded** synchronous bridge owned by the adapter, with these required properties:

1. **Bounded.** The bridge holds at most one bridge worker per open stream, and that count is capped by the same concurrency ceiling the request ledger already enforces. No unbounded thread spawn, no unbounded buffering. If no worker is available, `open` fails with a typed transport error rather than queueing.
2. **Cancellation-aware.** The bridge observes `OperationControl::check()` while blocked. A cancelled or expired operation wakes the worker and closes the underlying stream instead of leaving a task parked past its deadline.
3. **Deadline-propagating.** `OperationControl` already carries an absolute deadline. The adapter must not introduce a second, independent timeout or retry clock. One layer owns retry timing.
4. **Fail-closed.** If the app channel is unavailable, `open` returns `TransportError::Unavailable` or `TransportError::Denied`. It never falls back to a direct socket, a loopback listener, or a `TcpStream` of any kind.
5. **No buffering across the boundary.** `ByteStream` is a byte stream, not a message queue. The adapter must not buffer a whole POP3/SMTP response to hand across the boundary.

The sans-I/O property is preserved end to end: the SAM 3.1 client codec introduced by milestone M010 is a pure state machine over byte slices with no runtime, and the POP3/SMTP engines continue to be driven by injected streams.

`OperationControl` is unchanged. It remains the single carrier of cancellation and deadline state.

## Consequences

Positive: the blocked adapter is now unblocked at the *decision* level, so M006 and M010 can proceed without re-litigating the seam; lower crates keep their deterministic, executor-free character and their existing closure evidence remains valid; the bridge's required properties are explicit and testable rather than implicit.

Negative: the adapter carries a real concurrency cost — one bounded worker per open stream. This is accepted because mail operations are low-rate and already serialized behind the request ledger's bounded window. If a future workload proves this insufficient, that is a reason to revisit this ADR, not to widen it in place.

Neutral: if upstream's eventual AppManager publishes a *blocking facade* rather than an async channel, the bridge collapses to a direct synchronous read/write and this ADR costs nothing. The decision is deliberately compatible with that outcome.

## Compatibility and migration

No public API changes. `MailTransport`, `ByteStream`, `OperationControl`, and `TransportError` are unchanged by this ADR. `docs/architecture/transport-boundary.md` is updated to record that its deferred execution-model question is now answered by this ADR rather than by M006 speculatively.

## Security and reliability implications

Confining async to one crate keeps a single auditable place where router interaction happens, which preserves ADR-0001's narrow-network-authority goal. The fail-closed rule is restated here because a bridge is exactly where a fallback would be tempting: if the app channel is down, mail operations fail with a typed error and the user sees a failure, not a silently downgraded connection.

The bounded-worker rule is a reliability requirement as much as a security one: an unbounded bridge would convert a router outage into unbounded thread growth in the client.

## Verification

`scripts/check-boundaries.sh` must reject an async or executor reference in any crate below the seam, and the M010 closure must record that `mail-runtime` and the lower crates still compile and test with no async runtime in the dependency graph. The negative no-fallback evidence remains M006's obligation, unchanged.

## Supersession

A future ADR may supersede this decision only if it records a reason that the synchronous seam plus bounded bridge is insufficient for a demonstrated workload, and only if it preserves the fail-closed rule and the single-transport-authority invariant from ADR-0001.