# Mail Backend Foundation Milestone 013 — Canonical SAM Adoption and MailTransport Composition

Status: blocked

Repository baseline:

- i2pr-mail planning branch `plans/m012-managed-app-adapter-prep`;
- M012 is registered ready but not yet closed;
- `dbowm91/i2pr-sam` M018 is registered on
  `plans/018-injected-connection-provider` and is blocked on its M017 foundation merge.

Source roadmap:

- `plans/subsystems/mail-backend-foundation-roadmap.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`
- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`

Primary class: integration infrastructure + corrective consolidation

## 1. Objective

Replace the temporary mail-local SAM implementation with the canonical
`dbowm91/i2pr-sam` client and compose it with M012's managed-app logical-stream client
to produce the real synchronous `MailTransport` implementation, still without claiming
live secured i2pr/Postman qualification.

M013 is the final local implementation milestone before M006.

The intended stack is:

```text
mail-runtime::MailTransport / ByteStream       synchronous product seam
                 |
                 v
        i2pr-mail adapter runtime              Tokio confined here
                 |
       +---------+------------------+
       |                            |
       v                            v
M012 ManagedAppClient      canonical i2pr-sam async client
       |                            |
       +---- SamConnectionProvider--+
                 |
                 v
managed-app logical service = sam
                 |
                 v
       inherited i2pr app channel
```

The SAM protocol/session owner remains `i2pr-sam`. The mail adapter owns only
managed-app framing, service authorization, product destination/port selection, and the
sync/async boundary.

## 2. Why this milestone is blocked

M013 has two hard dependencies:

1. M012 must close the managed-app v1 client/multiplexer.
2. `dbowm91/i2pr-sam` M018 must close and be merged to its mainline, exposing a stable
   injected reliable-connection provider.

M018 itself is currently blocked on i2pr-sam M017, which must first reconcile and merge
the completed SAM foundation to `main`.

M013 does **not** require upstream i2pr SAM/368 or managed-app/386 to implement and close
its deterministic local composition. Those are M006 live-qualification gates.

When both local/cross-repo library dependencies close, re-audit their exact public APIs
and move M013 to ready in the same planning update.

## 3. Current implementation evidence

### Canonical SAM library is already the stronger protocol owner

The current i2pr-sam foundation branch implements:

- SAM 3.1-3.3 negotiation;
- retained SESSION CREATE control-connection lifetime;
- a fresh HELLO + STREAM CONNECT connection per stream;
- typed `Port` values and `FROM_PORT`/`TO_PORT`;
- naming and Destination identity;
- shared sessions/datagram families;
- bounded lifecycle/resource accounting;
- Java I2P SAM 3.3 live qualification;
- a blocking facade over the same async implementation.

The missing capability for managed apps is not protocol logic. It is the ability to
obtain each reliable SAM connection from an injected provider rather than
`TcpStream::connect`. i2pr-sam M018 owns that correction.

### M010 is now transitional

`crates/mail-sam` was useful to freeze the initial downstream requirements before the
dedicated library existed. It should not become a second long-term SAM implementation.

M013 may remove `crates/mail-sam` from the workspace only after transcript/API
comparison proves every mail-required behavior is covered by the selected i2pr-sam
revision. Historical M010 closure remains untouched.

### Mail's product ports are explicit

`ServiceProfile` already carries:

- POP3 `pop.postman.i2p:110`;
- SMTP `smtp.postman.i2p:25`.

M013 must map those port values directly to i2pr-sam's typed destination port argument.
It must not map them to the host-side I2PTunnel proxy ports 7660/7659.

### ADR-0002 remains the right architecture

The mail backend remains synchronous. Async execution is confined to the adapter. The
adapter may own one bounded Tokio runtime and call the canonical async SAM client inside
it while presenting synchronous `MailTransport` / `ByteStream` above.

No Tokio dependency may move into mail-domain/mime/proto/store/runtime.

## 4. Invariants that must not regress

- Exactly one SAM protocol/session implementation remains after migration.
- No direct `std::net`, `tokio::net`, DNS, localhost or host proxy path exists in the
  mail adapter.
- Every SAM connection is supplied by M012's managed-app `sam` service opener through
  i2pr-sam M018's provider abstraction.
- Provider failure cannot fall back to i2pr-sam's default TCP connector.
- `MailTransport` stays synchronous.
- Tokio/runtime ownership is confined to the adapter crate.
- One adapter runtime is bounded and owned; no runtime per read/write and no unbounded
  worker creation.
- The retained SAM stream session is shared across POP3/SMTP opens; returned data streams
  remain independent.
- POP3 uses destination port 110; SMTP uses destination port 25.
- Minimum negotiated SAM version for the production mail path is 3.2 or later because
  nonzero destination-port semantics are required.
- A 3.1-only peer fails before a portless STREAM CONNECT can be treated as mail success.
- Operation cancellation/deadline remains authoritative over connect/read/write.
- SMTP `DeliveryUnknown` and POP3 deletion semantics remain owned by mail-runtime.
- No adapter retry may turn ambiguous SMTP delivery into a safe retry.
- No raw body, credential, private Destination key or managed-app authority value appears
  in errors/logs.
- Lower mail crates remain unaware of i2pr-sam or managed-app framing.

## 5. Scope

### In scope

- exact-revision dependency on `i2pr-sam` / `i2pr-sam-proto` after M018 closure;
- implementation of `SamConnectionProvider` over M012 `ManagedAppClient`;
- adapter-owned Tokio runtime/lifecycle;
- SAM client/session initialization with min 3.2, max 3.3;
- stable mail session id derived from the trusted launch instance identity;
- `MailService::Pop3` / `MailService::Smtp` mapping to Postman destination + typed
  `TO_PORT`;
- synchronous `ByteStream` wrapper over async `SamStream`;
- cancellation/deadline bridge;
- retirement/removal of `crates/mail-sam` after parity proof;
- boundary guards and deterministic integrated fake-host tests.

### Explicitly out of scope

- live upstream i2pr process qualification;
- implementing SAM server features;
- implementing i2pr-sam M018 locally;
- Linux sandbox;
- package/signature/admin UX;
- `UnsafeDirect` qualification;
- I2CP mail transport;
- clearnet broker;
- Proposal 170;
- GUI/UI bridge;
- live Postman credentials/account.

## 6. Required production changes

### A — pin canonical SAM dependency

After M018 closes on i2pr-sam main:

- record exact repository revision;
- consume the async client/provider API;
- use `i2pr-sam-proto::Port` (or the final equivalent typed port);
- do not depend on i2pr-sam test/private modules;
- do not depend on the blocking facade unless implementation evidence shows it provides
  a cleaner cancellation mapping than the adapter runtime described below.

Because i2pr-sam is pre-1.0 and unpublished, pin an exact git revision for development.
Before any distributable i2pr-mail release, the SAM repository must have an explicit
compatible license and a stable distribution mechanism. That is a release gate, not a
pre-1.0 implementation blocker.

### B — managed-app SAM connection provider

Implement i2pr-sam M018's provider trait using an `Arc<ManagedAppClient>`.

Each provider `open()`:

1. checks the managed-app channel is established;
2. verifies effective `sam` capability;
3. calls `open_service(Sam)`;
4. returns that fresh `ManagedLogicalStream` as the provider duplex.

No SAM command or session id appears in the provider. The canonical SAM client decides
how many connections are needed and what bytes they carry.

Map M012 failures into generic provider categories:

- missing/denied capability → permission denied;
- closed app channel → unavailable/closed;
- app stream/resource ceiling → resource limit;
- malformed/fatal host channel → I/O/protocol unavailable;
- cancellation → cancelled where the final M018 API supports it.

### C — adapter runtime owner

Create one adapter runtime owner per backend/application instance.

Preferred shape:

- one small multithread or current-thread Tokio runtime chosen by evidence;
- bounded number of runtime worker threads;
- M012 channel tasks and i2pr-sam operations live on that runtime;
- runtime shutdown cancels/joins the managed-app client and SAM session before dropping.

Do not create a new Tokio runtime for each `MailTransport::open`, read or write.

If a current-thread runtime cannot service M012's background reader/writer while the
caller is outside `block_on`, use a bounded multithread runtime and record the reason.

### D — SAM client/session initialization

Construct the injected-provider SAM client with:

- minimum version 3.2;
- maximum version 3.3;
- existing bounded control/connect limits;
- no UDP-forwarding mode;
- no TCP endpoint fallback.

Create one ordinary STREAM session with transient Destination.

The session id must be deterministic per launch but unique across application instances,
for example a bounded encoding derived from the trusted managed-app instance id. It is
not a secret and must not include hostname, username, PID or wall-clock time.

Retain the session for the adapter lifetime. Let the canonical library own the session
control connection.

### E — MailService mapping

For each `MailTransport::open`:

`Pop3`:
- destination: `pop.postman.i2p`;
- `from_port = None` / protocol default;
- `to_port = Port(110)`.

`Smtp`:
- destination: `smtp.postman.i2p`;
- `from_port = None`;
- `to_port = Port(25)`.

The canonical SAM client opens a fresh provider connection, performs its own HELLO and
STREAM CONNECT referencing the retained session.

No explicit NAMING LOOKUP belongs in the mail adapter unless the selected canonical
library API requires it. Hostname-vs-Destination resolution is a SAM-library concern.

### F — synchronous ByteStream bridge

Return a wrapper containing:

- `Option<SamStream>`;
- shared adapter runtime handle/owner;
- the `OperationControl` passed to `MailTransport::open`;
- terminal state.

Each synchronous read/write executes the async SAM operation under the adapter runtime
and races it against an adapter-local cancellation/deadline future that periodically
calls `OperationControl::check()`.

Requirements:

- bounded polling quantum, documented and tested;
- cancellation/deadline drops the in-flight async operation and closes/drops the
  underlying stream so it cannot consume future bytes;
- a quantum timeout is not reported as user Timeout until `OperationControl` actually
  expires;
- no detached operation survives a cancelled synchronous call;
- write_all preserves exact bytes or returns failure; partial completion must not be
  reported as full success.

If a cleaner cancellation primitive can be added to `OperationControl` without
changing lower protocol semantics, record and test that change rather than hiding a
busy loop.

### G — retire mail-local SAM code

Before removal, construct a mail-required parity matrix:

- HELLO version behavior;
- stream session creation;
- distinct connection per STREAM CONNECT;
- naming/destination handling used by mail;
- typed TO_PORT 25/110;
- malformed/failure mapping needed by adapter.

Once canonical i2pr-sam covers the matrix, remove `crates/mail-sam` from workspace and
delete its production code/tests. Keep M010 plans/closure as historical evidence.

Do not retain the old crate "just in case"; two active SAM implementations create
divergent security/compatibility ownership.

## 7. Ordered work packages

### WP1 — dependency/API freeze

Re-read closed i2pr-sam M018 and M012 closure. Pin the exact revision and record the
provider/runtime API.

Acceptance: no private/test API dependency and no unresolved execution-model question.

### WP2 — provider composition

Implement ManagedAppClient → SamConnectionProvider and deterministic provider error
mapping.

Acceptance: exact provider-open count tracks the canonical client's utility/session/data
connections and no TCP/UDP fallback occurs.

### WP3 — SAM session and service mapping

Create/reuse one SAM STREAM session and map POP3/SMTP to typed destination ports.

Acceptance: deterministic transcripts show `TO_PORT=110` and `TO_PORT=25`, distinct
data connections, and refusal of negotiated SAM 3.1.

### WP4 — synchronous transport bridge

Implement `MailTransport` and cancellation-aware `ByteStream`.

Acceptance: sync callers can open/read/write while all async work remains adapter-owned;
deadline/cancel interrupts blocked operations within the documented bound.

### WP5 — backend convergence through adapter

Run existing POP3/SMTP fake scenarios through:

```text
BackendService -> MailTransport -> i2pr-sam -> M018 provider
              -> M012 managed-app logical stream -> synthetic host/SAM server
```

Acceptance: sync/read/send/restart/ambiguity scenarios remain semantically unchanged.

### WP6 — remove local SAM duplicate + guards/docs/closure

Prove parity, remove `crates/mail-sam`, strengthen boundary guards, update docs/planning,
and close M013.

## 8. Failure, cancellation, restart, and contention semantics

### Managed-app channel loss

M012 terminates all logical streams. The provider returns unavailable for future opens.
The retained i2pr-sam session becomes unusable and is discarded.

No attempt is made to reconstruct the application channel; only a new i2pr-managed app
launch can do that.

### SAM session loss

Invalidate the cached session. A future **new** `MailTransport::open` may establish a
fresh SAM session if the managed-app channel is still healthy and the caller's
OperationControl permits it.

Never reconnect behind an already-returned POP3/SMTP ByteStream.

### Data-stream failure

A failed data stream affects that operation only. The canonical SAM session remains
usable when its control connection is healthy.

### Cancellation/deadline

OperationControl governs:

- provider open;
- SAM HELLO/client startup;
- SESSION CREATE;
- STREAM CONNECT;
- synchronous stream reads/writes.

Cancellation closes the affected data stream. If cancellation occurs during initial
session construction, discard that partially established session generation.

### Retry ownership

The adapter may use the canonical client's bounded **transport-open** retry only before
a mail protocol stream exists. It must not retry SMTP protocol delivery.

Any retry policy must be named and bounded; no nested mail-runtime + SAM-library retry
loops with independent exponential clocks.

## 9. Compatibility and migration

No mailbox/storage schema migration.

`MailTransport` and `ByteStream` public APIs remain stable.

The internal SAM dependency changes from `i2pr-mail-sam` to a pinned canonical
`i2pr-sam` revision. This is intentionally a pre-release internal migration.

Update Cargo.lock and dependency documentation. Keep the exact revision visible so an
upstream API change cannot silently alter the adapter.

## 10. Required tests

At minimum:

- provider maps one logical app stream to each canonical SAM connection;
- no default TCP provider call under injected mode;
- no UDP bind under the mail path;
- negotiated 3.2 and 3.3 accepted, 3.1 rejected before port-aware connect;
- one retained stream session across sequential POP3/SMTP opens;
- POP3 exact `TO_PORT=110`;
- SMTP exact `TO_PORT=25`;
- two concurrent mail opens use distinct SAM data connections;
- data-stream failure preserves healthy session;
- session-control failure invalidates generation;
- managed-app channel failure invalidates all;
- missing SAM grant maps to `TransportError::Denied`;
- provider unavailable/resource limit map correctly;
- OperationControl cancel during provider open/session create/connect/read/write;
- deadline during same stages;
- quantum timeout does not become premature user timeout;
- no async task/stream/id/resource leak after repeated opens and cancellations;
- existing POP3 deletion reconciliation passes through adapter;
- successful SMTP and every `DeliveryUnknown` case remain unchanged;
- privacy/header tests remain unchanged;
- SAM parity matrix passes before mail-local crate removal;
- post-removal workspace has exactly one SAM protocol implementation dependency;
- boundary guards reject direct socket/DNS/loopback and lower-crate SAM imports.

## 11. Required verification commands

Routine floor:

```text
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

Focused commands, using final crate/package names:

```text
cargo test --locked -p i2pr-mail-managed-app
cargo test --locked -p i2pr-mail-runtime
cargo tree --workspace -e normal
```

Also record the exact pinned i2pr-sam revision and its M018 closure/CI evidence rather
than rerunning its entire cross-platform suite inside this repository.

Hosted i2pr-mail CI must be green on the exact M013 closure head.

## 12. Documentation updates

- `docs/architecture/i2pr-integration.md` — canonical SAM dependency and final local
  adapter stack;
- `docs/architecture/transport-boundary.md` — runtime/cancellation bridge;
- README crate/dependency inventory;
- Cargo/dependency policy notes;
- `plans/registry.md`;
- `plans/subsystems/mail-backend-foundation-roadmap.md`;
- M006 plan — local gate becomes M013 closed;
- `plans/closure/mail-backend-foundation/013-status.md`.

M010 closure remains immutable and is described as superseded forward for production SAM
ownership.

## 13. Acceptance criteria

M013 closes only when:

- M012's managed-app client is the sole application-channel owner;
- canonical i2pr-sam M018 supplies every SAM connection;
- no mail-local production SAM parser/state machine remains;
- one retained SAM STREAM session serves POP3/SMTP opens;
- exact typed destination ports 110/25 are used;
- SAM 3.1 cannot silently service the mail path;
- synchronous MailTransport observes cancellation/deadline over async operations;
- existing backend POP3/SMTP semantics pass through the complete synthetic stack;
- no direct/loopback socket path exists;
- dependency/boundary guards have negative evidence;
- exact-head hosted CI is green;
- M006's only remaining blockers are upstream i2pr SAM/368 and Managed app/386.

## 14. Stop conditions

Stop and register a corrective if:

- i2pr-sam M018 closes without an injected provider capable of supplying every STREAM
  control/data connection the mail path needs;
- consuming i2pr-sam requires private/unpublished implementation APIs beyond a stable
  exact-revision public crate surface;
- the provider mode can reach TCP/UDP fallback;
- the sync bridge cannot cancel a blocked async operation within a bounded interval;
- retiring `i2pr-mail-sam` loses a mail-required behavior not owned by canonical
  i2pr-sam;
- dependency licensing makes even pre-release cross-repo consumption unauthorized;
- implementation expands into live i2pr/sandbox qualification.

## 15. Closure evidence required

Record:

- exact M012 closure and i2pr-sam M018 revision/closure;
- SAM parity/removal matrix;
- dependency graph before/after;
- provider connection-count/transcript evidence;
- version/port matrix;
- OperationControl cancellation/deadline matrix;
- end-to-end synthetic POP3/SMTP scenarios;
- resource/task soak;
- guard mutation evidence;
- local/hosted verification;
- security/license review;
- unresolved findings;
- M006 unblock audit against current upstream SAM/368 and Managed app/386.

## 16. Handoff notes

M013 should make M006 boring.

After M013, the mail repository should already contain the production adapter stack and
all deterministic correctness evidence. M006 should then only package/launch the app
through a closed `Secured` i2pr runtime, exercise the closed port-aware SAM server, run
real POP3/SMTP qualification, and prove the negative no-fallback boundary.

Do not keep `i2pr-mail-sam` as a second fallback implementation after parity is proven.
The dedicated SAM repository exists specifically to prevent that ownership split.
