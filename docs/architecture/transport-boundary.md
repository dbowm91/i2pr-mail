# Transport integration boundary

This document freezes the seam that the future i2pr adapter (M006) implements. It
describes the contract only; it deliberately does not choose an execution model
that upstream has not closed yet.

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

Consequences for the adapter:

- The adapter translates `MailService` into the authorized logical service stream
  the gateway offers. Mail code above it is unchanged.
- Plan 355 promises a logical service stream, not a pre-connected arbitrary
  destination byte stream. The adapter must request a stream per operation; it
  may not assume a persistent tunnel is already open.
- Any SAM client dependency belongs in the adapter crate at or above the seam.
  It must not leak into `mail-proto`, `mail-store`, or `mail-domain`.
- **No localhost fallback is permitted.** If the router gateway is unavailable,
  the operation fails with a transport error. A loopback listener that proxies to
  the router would reintroduce a second, unmanaged authority and a clearnet-visible
  surface.

## Execution-model stop condition

`MailTransport` and `ByteStream` are synchronous today. That is a deliberate
foundation choice, not an oversight.

If the closed Plan 355 SDK turns out to expose only asynchronous channels
(`AsyncRead`/`AsyncWrite`), then M006 **must record an architecture decision
before** changing `MailTransport` or adding any blocking bridge. That decision has
to choose between keeping the synchronous seam with a bounded thread handoff or
moving the seam to async, and it must state how cancellation and deadlines
propagate. Per `ADR-0001`, executor integration is a runtime composition
responsibility and may not call blocking I/O on an async executor thread.

M008 does not make this choice. No ADR is added for it, because speculating
before Plan 355 closes would document a guess as an accepted decision.

## Stop conditions for M006

Implementation must stop and re-plan if any of these turn out to be required:

- changing `MailTransport` from sync to async, or adding a blocking-thread bridge,
  without a recorded architecture decision;
- adding a SAM, I2CP, or router-internal dependency below the adapter boundary;
- exposing private router machinery as public API to satisfy the adapter;
- any localhost, clearnet, LAN, or loopback fallback path;
- giving the adapter authority over mail state, retry decisions, or deletion
  reconciliation.