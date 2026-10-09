# Transport integration boundary

This document freezes the seam that the future i2pr adapter (M006) implements. It
describes the contract only; it deliberately does not choose an execution model
that upstream has not closed yet.

> M012 note (2026-10-09): the managed-app logical-stream seam below
> `MailTransport` is now concrete. M012 (`i2pr-mail-managed-app`) owns the
> application-role v1 framing over injected async I/O and exposes authorized
> logical `sam` streams as `AsyncRead + AsyncWrite` values behind a bounded
> multiplexer (one reader, one serialized writer, ≤64 pending requests, ≤128
> live streams, bounded per-stream/aggregate queues, fail-closed teardown).
> `MailTransport`/`ByteStream` stay synchronous per ADR-0002; the sync/async
> bridge and canonical SAM composition belong to M013. No lower crate gains an
> async or managed-app dependency. See
> `plans/closure/mail-backend-foundation/012-status.md`.

## What the runtime asks for

`MailTransport` is the single authorized remote I/O path in the backend. It is
owned by `crates/mail-runtime/src/transport.rs` and takes one logical
`MailService` value:

```rust
fn open(
    &self,
    service: MailService,
    control: OperationControl,
) -> Result<Box<dyn ByteStream>, TransportError>;
```

That is the entire request. An implementation must:

1. resolve `MailService` to whatever authorized transport the host provides;
2. honor `OperationControl` while connecting *and* while doing stream I/O;
3. return `TransportError::Timeout` or `TransportError::Cancelled` instead of
   blocking past the deadline or ignoring cancellation;
4. never name or resolve a host, port, or address of its own.

`ByteStream` is a synchronous `read`/`write_all` pair. Protocol drivers bound
their own lines: `transport::read_line` refuses to grow past the protocol line
ceiling and reports a neutral `LineError`, which each driver maps into its own
typed error (`SyncError` for POP3, `SubmitError` for SMTP).

## What lower layers must never know

`i2pr-mail-domain`, `i2pr-mail-mime`, `i2pr-mail-proto`, and `i2pr-mail-store`
have no transport concept at all. SAM, I2CP, managed-app framing, leases,
tunnels, and router principals are invisible below the seam. `scripts/check-boundaries.sh`
enforces the dependency direction and rejects network, filesystem, and async
capability references in the lower crates.

There is exactly one transport authority. A second path, a direct socket, a DNS
lookup, or a localhost bridge would be a second authority and is not permitted.

## Adapter obligations for M006

M006 implements `MailTransport` above this seam using the router interface
closed by upstream i2pr work:

- Plan 354 extracts listener-independent private SAM/I2CP connection drivers.
- Plan 355 consumes those drivers to provide the trusted app-principal router
  gateway and maps one managed-app logical service stream to one raw SAM or I2CP
  protocol connection.

Both are closed on upstream `main` (`2f82c799`). The exact contract, the interface
matrix, and the authority analysis are recorded in `docs/architecture/i2pr-integration.md`.

Consequences for the adapter:

- The adapter translates `MailService` into the authorized logical service stream
  the gateway offers. Mail code above it is unchanged.
- Plan 355 promises a logical service stream, not a pre-connected arbitrary
  destination byte stream. The adapter must request a stream per operation; it
  may not assume a persistent tunnel is already open.
- Any SAM client dependency belongs in the adapter crate at or above the seam.
  It must not leak into `mail-proto`, `mail-store`, or `mail-domain`. Upstream
  assigns SAM client implementation to a separate repository and will never ship
  it, so this repository owns the client codec itself: see `i2pr-mail-sam`,
  introduced by milestone M010.
- **No localhost fallback is permitted.** If the router gateway is unavailable,
  the operation fails with a transport error. A loopback listener that proxies to
  the router would reintroduce a second, unmanaged authority and a clearnet-visible
  surface. Upstream's loopback SAM listener on `127.0.0.1:7656` is reachable today
  and is deliberately not used; see `docs/architecture/i2pr-integration.md` §5.

## Execution-model stop condition

`MailTransport` and `ByteStream` are synchronous today. That is a deliberate
foundation choice, not an oversight.

This question was open when M008 closed, and M008 correctly declined to guess.
It is now answered by evidence and recorded in
`plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`.

The evidence: upstream i2pr Plan 355 closed the router app-principal gateway on
`main` (`2b96f1bc`), and its stream-opening surface is asynchronous only —
`AppGatewaySession::open_sam`, `open_i2cp`, `shutdown`, and `wait_closed` are all
`pub(crate) async fn`. The stop condition was therefore triggered.

ADR-0002 decides:

- `MailTransport` and `ByteStream` stay synchronous and unchanged;
- async is confined to the adapter crate at or above this seam;
- any app channel is adapted through a **bounded** bridge that observes
  `OperationControl`, propagates its deadline, caps one worker per open stream,
  and fails closed with a typed transport error;
- no crate below the seam gains an async runtime, an executor, or a blocking bridge.

Per `ADR-0001`, executor integration is a runtime composition responsibility and may
not call blocking I/O on an async executor thread, so the bridge must never run
synchronous mail I/O on an executor worker.

The consequence for this milestone's scope is unchanged: ADR-0002 decides the *shape*,
while M006 still owns the concrete adapter and its bridge implementation.

## Stop conditions for M006

Implementation must stop and re-plan if any of these turn out to be required:

- changing `MailTransport` from sync to async, or adding a blocking-thread bridge,
  without a recorded architecture decision;
- adding a SAM, I2CP, or router-internal dependency below the adapter boundary;
- exposing private router machinery as public API to satisfy the adapter;
- any localhost, clearnet, LAN, or loopback fallback path;
- giving the adapter authority over mail state, retry decisions, or deletion
  reconciliation.