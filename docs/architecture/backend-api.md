# Backend service API

`i2pr-mail-runtime::BackendService` is the single mutable lifecycle owner. Startup opens/migrates the store and recovers stale outbox submissions before accepting work. It exposes request-id keyed sync, local list/open/read/release/delete, draft create/update/delete, queue/send, remote-delete request, body fetch, service profile, outbox inspection, explicit DeliveryUnknown resolution, health, and shutdown operations.

Each method returns a typed `OperationResult<T>` with bounded events (at most two for a long operation).

## Request-ID lifecycle

Duplicate suppression is bounded over time rather than permanent. A request ID enters active ownership before its operation runs and leaves it on every terminal path, including credential, transport, store, protocol, validation, cancellation, and timeout failures. On completion the ID enters a recent-completion window of `MAX_RECENT_COMPLETED_REQUESTS` entries with deterministic oldest-first eviction. Reaching that window evicts the oldest entries; it never reports `Capacity`. An evicted old ID may be reused because no durable cross-session idempotency contract is claimed, and the window may reset on restart since request IDs are not persisted.

`Capacity` for request identity is therefore reserved for the one genuinely live ceiling, `MAX_ACTIVE_REQUESTS` in-flight requests. `BackendHealth` reports `active_requests` (which includes the in-flight health probe itself) and `retained_request_ids`, so release on terminal paths and bounded retention are directly observable.

Results carry domain values: `MessageSummary.state` is a `StoredMessageState` and `OutboxStatus` carries a `SubmissionState` with a `SubmissionProgress`. The API never returns state strings, protocol ordinals, filesystem paths, raw SQLite details, or arbitrary host/port networking.

Large content uses session-scoped opaque handles, with at most 32 active handles, 64 MiB total retained bytes, and 64 KiB per read. Handles release explicitly or when the service shuts down.

A DeliveryUnknown item cannot be sent until an explicit caller decision marks it confirmed Sent or permits a retry. `CredentialSource` supplies a non-serializable, non-debug credential value for a single POP3/SMTP operation; no secret persists in the store. `OperationControl` carries deadline and cancellation into transport ownership.

The service methods are synchronous in this foundation and serialize access through one `&mut BackendService`. Integrations must not run them on an async executor thread; an async command loop can be layered above this stable ownership surface later.