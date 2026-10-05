# Mail Backend Foundation Milestone 005 — Backend Service Convergence

Status: closed

Repository baseline: planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; execute after M003 and M004 closure

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: capability

## 1. Objective

Converge receive, storage, compose, and submission into one frontend-neutral backend lifecycle and API. Prove a complete mail workflow with scripted/fake I2P transport before any graphical frontend or i2pr-specific adapter is introduced.

## 2. Why this milestone is ready

Blocked until both M003 and M004 close. Those milestones provide independently qualified receive/send engines over the same domain/store/transport architecture.

## 3. Current implementation evidence

At planning time no backend service or executable exists. Earlier milestones are expected to expose internal runtime operations but not a stable consumer surface.

## 4. Invariants that must not regress

- frontend never accesses SQLite/blob paths directly;
- consumer API exposes domain ids/handles rather than POP3 ordinals/SMTP state-machine internals;
- bulk raw message/attachment transfer is handle/stream based rather than unbounded control JSON;
- one backend lifecycle owner coordinates account sync/send/store;
- credentials are supplied through a secret-provider/session boundary and not persisted in normal config;
- no network authority is granted to a frontend;
- all commands/events/queues are bounded;
- restart reconciliation runs before new remote mutations where required.

## 5. Scope

### In scope

Frontend-neutral operations such as:

- account/service-profile inspect/configure for initial single-account product;
- connect/auth status;
- sync;
- list mailbox/message summaries;
- fetch/open message;
- request body/raw entity/attachment via bounded content handle/stream;
- create/update/delete draft;
- queue/send draft;
- inspect/resolve DeliveryUnknown without implicit resend;
- local delete / remote-delete policy operation;
- typed progress/change/error events;
- health and graceful shutdown.

Support multiple AccountId in schema/API where already cheap, but do not build multi-account UX.

Create a minimal backend binary/service only if needed to prove lifecycle/API ownership. No GUI toolkit.

### Explicitly out of scope

- i2pr-specific transport implementation;
- web/desktop/mobile UI;
- localhost HTTP API;
- remote HTML resources;
- generic plugin framework;
- search/indexing beyond simple metadata query;
- background always-on polling unless required for lifecycle proof.

## 6. Required production changes

### Service/API model

Define project-owned command/result/event DTOs. Keep them language-neutral enough for a future UI bridge, but do not freeze a network wire protocol unnecessarily.

Every request with asynchronous completion has a stable request id. Events that are not replies are mechanically distinct.

Return content handles for large payloads with ownership/lifetime/size metadata.

### Lifecycle

Backend startup:

1. open/migrate/recover store;
2. reconcile stale local receive/send states without network guesses;
3. become ready for consumer commands;
4. open network streams only on explicit operation/policy.

Shutdown cancels/drains operations according to bounded deadlines, closes transport owners, and leaves durable states restart-reconcilable.

### Secret boundary

Define SecretProvider/CredentialsSource or equivalent whose implementation can supply POP3/SMTP username/password for a session. Domain/store APIs receive only the minimum needed ephemeral credential values and never serialize them.

## 7. Ordered work packages

### A — Consumer contract

Freeze command/result/event/content-handle types and bounds.

Acceptance evidence: serialization/strictness tests if serialized, max+1 tests, wrong-kind request tests.

### B — Lifecycle/composition owner

Compose store/runtime/transport and implement startup/shutdown/cancellation.

Acceptance evidence: deterministic startup/restart/shutdown scenarios.

### C — End-to-end fake transport

Script POP3 and SMTP servers in memory and drive the full service.

Acceptance evidence: sync -> list -> open -> draft -> send -> restart -> offline read -> reconcile.

### D — Security/resource closure

Add queue/content-handle limits, secret sentinel tests, and negative network-authority checks.

## 8. Failure, cancellation, restart, and contention semantics

Per-account remote mutation ownership is serialized.

Consumer cancellation returns typed cancellation while preserving protocol/store reconciliation invariants.

Stale request ids or handles fail closed.

Content handles expire/close deterministically and cannot escape the private store root.

Startup must not blindly retry DeliveryUnknown or assume DeleteMarkedSession committed.

Event queues are bounded with an explicit overflow/coalescing policy that never drops terminal operation outcomes silently.

## 9. Compatibility and migration

This is the first candidate backend consumer contract. Mark it pre-release until M005 closure. If a language-neutral serialized form is frozen, version it explicitly and preserve strict decoding/bounds.

No UI compatibility promise is created beyond the documented backend contract.

## 10. Required tests

- complete fake POP3/SMTP workflow;
- offline startup/read with no transport;
- wrong credentials without secret leakage;
- network unavailable;
- cancellation during sync and send;
- restart with DeletePending/DeleteMarkedSession;
- restart with FailedSafeToRetry/DeliveryUnknown;
- duplicate request id;
- content handle bounds/lifetime/path confinement;
- event queue pressure;
- graceful shutdown with active operation;
- consumer cannot request arbitrary host/port networking.

## 11. Required verification commands

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-runtime
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

If a backend binary exists, add a deterministic no-network CLI/service smoke command and record it exactly.

## 12. Documentation updates

- docs/architecture/backend-api.md;
- docs/architecture/runtime.md;
- docs/security/privacy.md;
- README developer usage;
- roadmap/registry;
- closure record 005-status.md.

## 13. Acceptance criteria

A clean checkout can run a deterministic backend scenario that:

- syncs synthetic I2P mail through injected POP3 transport;
- exposes headers and full content offline;
- creates and submits a message through injected SMTP transport;
- persists Sent/outbox state;
- restarts and reconciles all incomplete states correctly;
- exposes only bounded frontend-neutral operations;
- requires no GUI and no direct network privileges.

## 14. Stop conditions

Stop if:

- API design requires exposing database paths, protocol ordinals, or router internals;
- large content must be tunneled through unbounded JSON/control payloads;
- credential persistence is required to make tests pass;
- background scheduler scope appears necessary but lacks its own lifecycle decision;
- UI concerns begin driving backend ownership.

## 15. Closure evidence required

API contract table, full deterministic end-to-end transcript, restart/failure matrix, content-handle bound tests, secret sentinel evidence, queue-pressure behavior, exact verification results, security review, and unblock audit for M006.

## 16. Handoff notes

M005 is the backend functional qualification point. Do not wait for i2pr or a GUI to prove the mail product logic works.
