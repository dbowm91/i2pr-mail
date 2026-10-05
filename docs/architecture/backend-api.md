# Backend service API

`i2pr-mail-runtime::BackendService` is the single mutable lifecycle owner. Startup opens/migrates the store and recovers stale outbox submissions before accepting work. It exposes request-id keyed sync, local list/open/read/release/delete, draft create/update/delete, queue/send, remote-delete request, body fetch, service profile, outbox inspection, explicit DeliveryUnknown resolution, health, and shutdown operations.

Each method returns a typed `OperationResult<T>` with bounded events (at most two for a long operation). Duplicate request IDs are rejected within a service lifetime; the session accepts at most 4096 unique IDs. Large content uses session-scoped opaque handles, with at most 32 active handles, 64 MiB total retained bytes, and 64 KiB per read. Handles release explicitly or when the service shuts down.

The API returns domain ids/states, never protocol ordinals, filesystem paths, raw SQLite details, or arbitrary host/port networking. A DeliveryUnknown item cannot be sent until an explicit caller decision marks it confirmed Sent or permits a retry. `CredentialSource` supplies a non-serializable, non-debug credential value for a single POP3/SMTP operation; no secret persists in the store. `OperationControl` carries deadline and cancellation into transport ownership.

The service methods are synchronous in this foundation and serialize access through one `&mut BackendService`. Integrations must not run them on an async executor thread; an async command loop can be layered above this stable ownership surface later.
