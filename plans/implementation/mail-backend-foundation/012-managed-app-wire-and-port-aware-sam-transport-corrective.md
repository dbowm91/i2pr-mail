# Mail Backend Foundation Milestone 012 — Managed-App Wire Client and Port-Aware SAM Transport Corrective

Status: ready for handoff

Repository baseline:

- i2pr-mail `main` `e51d35eed3d9e92509138b1ad08a7d897275960f`;
- local foundation M001-M005 and M007-M011 closed;
- upstream i2pr audited at `main` `acd752b7ce35111ab129a1fb86e4a7315df37a7f`;
- upstream managed-app runtime Plans 368-371 and 382-383 closed;
- upstream SAM/368 remains separately ready and now explicitly owns SAM 3.2+ `FROM_PORT`/`TO_PORT` semantics required by mail;
- upstream managed-app Plan 385 is registered on `plans/385-linux-secured-app-sandbox` for the first Linux `Secured` backend.

Source roadmap:

- `plans/subsystems/mail-backend-foundation-roadmap.md`

Long-term requirements:

- `plans/000-long-term-specification.md`
- `plans/001-terminology-and-domain-model.md`
- `plans/002-long-term-roadmap.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`
- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`

Primary class: corrective invariant + integration infrastructure

## 1. Objective

Correct the now-stale pre-runtime assumptions in M006/M010 and implement every local
piece of the managed-app transport that can be qualified without claiming a live secured
router integration.

M012 owns four tightly related outcomes:

1. a bounded, language-neutral managed-app v1 client codec for the application side of
   the real i2pr `stdin/stdout` channel;
2. a bounded synchronous logical-stream multiplexer above that channel, preserving the
   existing `MailTransport` / `ByteStream` API;
3. a correction of the M010 SAM client model so one long-lived SAM session-control
   connection owns the Destination while each `STREAM CONNECT` uses a distinct SAM
   connection;
4. SAM 3.2+ port-aware client support so POP3 and SMTP can target the reference Postman
   I2P service ports 110 and 25 without a localhost I2PTunnel proxy.

M012 does **not** close M006. It leaves final live qualification gated on two upstream
capabilities:

- SAM/368 closure, because current i2pr advertises only SAM 3.1 and therefore cannot yet
  carry the required nonzero `TO_PORT`;
- Managed native app runtime/385 closure, because current `Secured` still refuses
  before exec and M006 forbids `UnsafeDirect` as a substitute.

## 2. Why this milestone is ready

Every local prerequisite is closed:

- M005 backend convergence;
- M007-M009 corrective sequence;
- M010 isolated SAM codec foundation;
- M011 mainline integration.

The old M006 reachability blocker is no longer true. Current upstream i2pr has a real
production chain:

```text
i2pr-daemon
  -> inherited anonymous manager transport
  -> i2pr-appd
  -> inherited apphost transport
  -> i2pr-apphost
  -> managed application over stdin/stdout
```

Plans 369 and 383 qualify that chain with production catalog authority and real private
SAM/I2CP logical streams. The application-facing managed-app v1 protocol is therefore
concrete enough to implement against without exposing or depending on private Rust
gateway types.

The two remaining external gates affect only final execution evidence, not the local
client/multiplexer/codecs:

- SAM 3.2/3.3 wire semantics are stable enough to implement a strict client, while
  upstream SAM/368 owns the server-side capability and interoperability qualification;
- the managed-app v1 framing/stream contract is already closed and exercised, while
  Plan 385 owns OS containment.

## 3. Current implementation evidence and corrective findings

### A — M006's upstream reachability finding is stale

M006 and `docs/architecture/i2pr-integration.md` still say no app runtime, process
channel, package/grant owner, or production gateway caller exists. That was true at the
2026-10-06 audit and is false on current upstream main.

Plans 368-371 and 382-383 now provide:

- the inherited daemon↔manager protocol and bridge;
- `i2pr-appd`;
- `i2pr-apphost`;
- the application-side managed-app v1 consumer;
- exact app-id/instance-id binding;
- persistent signed package selection, publisher trust and SAM/I2CP grants;
- restart-safe production autostart;
- black-box private SAM/I2CP streams with no loopback listener.

M012 must correct forward rather than rewrite M011 or M010 closure history.

### B — M010 modeled SAM STREAM lifetime incorrectly

M010 treats HELLO → SESSION CREATE → STREAM CONNECT as one client state machine over one
connection. For SAM STREAM sessions, the `SESSION CREATE` connection is the long-lived
session/control owner. Each `STREAM CONNECT` is performed on a **new SAM connection**
(after that connection's own HELLO) referencing the existing session id.

Therefore `MailTransport::open` cannot simply "open one managed-app SAM stream and run
the whole M010 transcript". The adapter needs:

- one retained SAM control logical stream for the lifetime of the mail transport/session;
- one new managed-app logical `sam` stream per POP3/SMTP connection;
- independent HELLO on each SAM connection;
- the same session id on each STREAM CONNECT;
- deterministic invalidation/teardown when the control connection dies.

This is a corrective to M010's model, not a reason to rewrite its closure record.

### C — SAM 3.1 cannot address Postman's nonzero I2P service ports

Reference Java I2P client-tunnel configuration targets:

- `pop.postman.i2p:110`;
- `smtp.postman.i2p:25`.

The familiar local 7660/7659 ports are I2PTunnel TCP proxy listeners, which this
repository intentionally refuses to use.

SAM 3.1 has no `FROM_PORT`/`TO_PORT`; those semantics begin in SAM 3.2 and are part
of the 3.3 profile upstream SAM/368 is already registered to implement. Resolving the
hostname to a Destination does **not** make the I2P service port disappear.

Consequently M006 cannot truthfully close against current i2pr SAM 3.1. M012 adds the
client-side port grammar/state now and requires a negotiated version supporting it;
M006 waits for upstream SAM/368.

### D — the adapter consumes the managed-app wire contract, not private gateway Rust APIs

M006 was written when the only concrete surface was the internal async
`AppGatewaySession`. The application now sees framed managed-app v1 bytes over
stdin/stdout. It should not import `i2pr-daemon`, `i2pr-app-proto`, or any unpublished
upstream Rust crate.

The adapter must implement the language-neutral v1 contract locally and treat i2pr as a
wire-compatible host.

### E — ADR-0002's conclusion survives, but its mechanism assumption needs review

ADR-0002 correctly keeps `MailTransport` synchronous and keeps runtime mechanics above
the lower mail crates. Its specific expectation of adapting an async private gateway was
based on the pre-Plan-369 interface.

If the real stdio application channel can be implemented with a bounded synchronous
multiplexer and no async runtime, record a new ADR that **supersedes only the concrete
bridge mechanism** while preserving ADR-0002's synchronous seam, fail-closed behavior,
single transport authority, boundedness, deadline/cancellation, and no-loopback
requirements. Do not silently contradict an accepted ADR.

## 4. Invariants that must not regress

- `MailTransport` and `ByteStream` remain synchronous.
- No network socket, resolver, localhost listener, or direct host connector is added.
- The adapter communicates only through inherited managed-app stdin/stdout.
- No dependency on unpublished i2pr Rust crates.
- Managed-app requested capabilities never become grants.
- The application accepts only effective host-issued capabilities.
- `sam` capability denial occurs before any SAM state is created.
- One SAM control connection owns one mail Destination/session; data connections refer
  to it but do not own it.
- A data-connection failure does not destroy a healthy control session unless SAM
  semantics require it.
- Control-session loss invalidates the session generation and every dependent connection.
- Nonzero Postman destination ports require negotiated SAM >= 3.2. Never silently send
  a portless 3.1 CONNECT.
- POP3/SMTP ambiguity/retry ownership remains in `mail-runtime`; transport reconnects
  cannot turn `DeliveryUnknown` into a retry-safe state.
- Every frame, queue, request, stream id, buffered chunk, and worker is bounded.
- Cancellation/deadline is observed while opening and while blocked on stream I/O.
- No raw mail body or credentials enter transport diagnostics.
- Lower `mail-domain`, `mail-mime`, `mail-proto`, and `mail-store` crates remain
  unaware of managed-app/SAM framing.

## 5. Scope

### In scope

- a new adapter/client crate at or above the transport boundary, expected package name
  `i2pr-mail-i2pr` unless implementation finds a clearer non-conflicting name;
- managed-app v1 application-role handshake and bounded frame codec;
- the exact application-side control subset required by mail;
- bounded request/reply correlation and logical stream multiplexing;
- synchronous `ByteStream` handles backed by the single channel owner;
- capability and hello identity validation;
- corrected SAM session/control/data connection ownership;
- SAM 3.2/3.3 version negotiation and `FROM_PORT`/`TO_PORT` client grammar;
- deterministic fake-host integration proving POP3/SMTP can run through the adapter;
- planning/docs correction of stale M006/M010-derived assumptions.

### Explicitly out of scope

- live i2pr process execution;
- `UnsafeDirect` qualification;
- implementing or qualifying upstream SAM/368;
- implementing upstream Linux sandbox Plan 385;
- package installation/signing/admin CLI;
- brokered clearnet;
- I2CP mail transport;
- Proposal 170;
- GUI/UI bridge;
- live Postman credentials or account tests;
- changing the lower mail protocol/storage APIs.

## 6. Required production changes

### A — managed-app v1 client codec

Implement only the application-role subset needed by mail.

Required wire support:

- fixed `I2PA` handshake, major/minor 1.0, Application role, zero reserved bytes;
- 12-byte frame envelope with the frozen payload/control ceilings;
- application → host:
  - `hello`;
  - `open`;
  - `close`;
  - `reset`;
- host → application:
  - correlated `reply`;
  - `capabilities`;
  - `stream_closed`;
  - `stream_reset`;
  - `health`.

`permission_request` is not required for initial mail operation. Mail must start with
the effective `sam` grant already present. If a permission response arrives without a
locally issued request, it is unexpected input.

The codec must reject:

- unknown frame version/kind/flags;
- control on nonzero stream ids and data on stream zero;
- payload/control max+1 before allocation;
- duplicate/unknown JSON fields;
- wrong-direction messages;
- noncanonical instance ids;
- unknown/duplicate/completed request ids;
- data for unopened/stale/foreign stream ids.

### B — identity and bootstrap input

The adapter requires the trusted expected `AppId` and `AppInstanceId` that the
host supplied for this launch; it sends those exact values in `hello`.

Do not guess an environment variable name in lower code. The adapter constructor should
take a typed bootstrap identity value. Final M006 composition maps Plan-385/managed-app
launch environment into it.

### C — one bounded channel owner

Exactly one owner reads managed-app stdin and one owner serializes writes. Logical
`ByteStream` objects must not concurrently parse the shared framing stream themselves.

Preferred implementation shape if no new ADR is needed:

- one bounded worker owns stdin/stdout;
- bounded command queue (for example `std::sync::mpsc::sync_channel` or a reviewed
  equivalent), never unbounded `mpsc::channel`;
- bounded per-stream receive queues;
- exact request-id and stream-id ledgers;
- condition-variable/timeout based wakeup rather than busy spinning;
- terminal channel failure wakes every waiter with one typed transport failure;
- dropping a logical stream sends close/reset at most once and releases local ownership.

If this implementation conflicts with ADR-0002's concrete bridge wording, file the
superseding ADR before code.

### D — corrected SAM connection model

Split `i2pr-mail-sam` state into connection-local and session-global ownership.

Required concepts:

- `SamConnection` state: Unestablished → HelloEstablished → command-specific/data;
- `SamStreamSession`: session id, generation, negotiated version, control-connection
  liveness, optional resolved-name cache;
- a control connection performs:
  - HELLO;
  - SESSION CREATE STYLE=STREAM;
  - NAMING LOOKUP as needed;
- each data connection performs:
  - HELLO;
  - STREAM CONNECT ID=<existing-session-id> DESTINATION=<destination>
    TO_PORT=<service-port>;
  - after successful STREAM STATUS it becomes opaque mail bytes.

A data connection must never issue SESSION CREATE.

### E — SAM version and port grammar

Extend the codec so the mail adapter may negotiate the minimum required version rather
than hard-coding 3.1.

For the Postman path:

- require negotiated version >= 3.2;
- accept the upstream-qualified 3.3 profile;
- encode `TO_PORT` from `ServiceEndpoint::port()`;
- support `FROM_PORT` only if needed by the frozen SAM grammar, defaulting to 0 when
  absence is normative;
- reject values outside 0–65535 before formatting;
- reject a 3.1-only peer for a nonzero destination port with a typed unsupported-version
  error;
- no hidden mapping to local 7659/7660.

The canonical STREAM CONNECT result is `STREAM STATUS RESULT=...`. M010's additional
legacy `SAM <session>:<stream> OK` acceptance must be re-reviewed against pinned
current Java I2P/i2pd evidence. Retain it only if an actual supported peer requires it;
otherwise narrow the accepted grammar in this corrective rather than carrying an
unevidenced alternate success form.

### F — MailTransport adapter

The adapter owns one lazily established SAM session generation.

`open(Pop3)`:

1. verify host hello/capabilities are established and `sam` is effective;
2. establish/reuse the control SAM session;
3. resolve `pop.postman.i2p` if required by the selected grammar;
4. open a new managed-app `sam` logical stream;
5. HELLO on that SAM connection;
6. STREAM CONNECT using the shared session id and `TO_PORT=110`;
7. return the post-connect data stream.

`open(Smtp)` is identical with `smtp.postman.i2p` and `TO_PORT=25`.

No implicit retry occurs after a returned stream has carried mail bytes. Re-establishing
a dead SAM session is allowed only for a **new** `MailTransport::open` request before
its protocol operation has acquired delivery ambiguity.

## 7. Ordered work packages

### WP1 — corrective contract freeze

Re-read current managed-app v1 and official/pinned SAM 3.2/3.3 contracts. Record the
M010 lifetime/port findings in architecture documentation and decide whether ADR-0002
needs a narrowly superseding ADR for the now-synchronous app-facing channel.

Acceptance: one state/connection diagram, one version/port matrix, and no unresolved
execution-model decision.

### WP2 — managed-app codec

Implement bounded handshake/frame/control encoding/decoding with exhaustive
direction/variant coverage.

Acceptance: golden bytes plus malformed/max+1/duplicate/unknown-field and exhaustive
control-direction tests.

### WP3 — managed-app multiplexer

Implement single-owner request/stream correlation, bounded queues, close/reset,
cancellation/deadline wakeups, and logical synchronous `ByteStream`.

Acceptance: adversarial fake-host tests with fragmentation, coalescing, reordering where
the protocol permits it, backpressure, stale ids, EOF and cancellation.

### WP4 — SAM corrective

Refactor `i2pr-mail-sam` into correct control-session/data-connection ownership and add
3.2+ port/version support.

Acceptance: deterministic transcripts proving one SESSION CREATE control connection and
multiple independent STREAM CONNECT connections with exact ports.

### WP5 — MailTransport composition

Compose managed-app + SAM pieces above the existing seam.

Acceptance: the existing fake POP3 and SMTP protocol fixtures complete through a
synthetic managed-app host that verifies the adapter emitted distinct SAM control/data
connections and the expected `TO_PORT` values.

### WP6 — guards/docs/closure

Extend `scripts/check-boundaries.sh`, update architecture/registry/roadmap, and write
M012 closure.

## 8. Failure, cancellation, restart, and contention semantics

### Managed-app channel failure

EOF, malformed framing, wrong identity reply, impossible correlation, or host reset makes
the channel generation terminal. Every active logical stream and pending request wakes
with a typed transport/protocol error. Do not attempt to re-open stdin/stdout inside the
mail process.

### SAM control-session failure

Mark the current SAM generation dead. No new data connection may attach to it. Existing
data streams are treated as failed when their host stream terminates; the adapter does
not pretend they remain valid.

A future `MailTransport::open` may establish one fresh session generation if the
managed-app channel itself is still healthy.

### Data-stream failure

Close/reset only that logical stream. A failed POP3/SMTP connection must not
automatically kill a healthy SAM control session.

### Cancellation/deadline

One `OperationControl` remains the caller's authority. The multiplexer must use bounded
waits/wakeup so cancellation/deadline can interrupt:

- request/reply wait;
- stream-open wait;
- inbound read wait;
- outbound capacity wait;
- SAM HELLO/SESSION/CONNECT waits.

Do not add an independent retry deadline.

### Contention

At most the existing backend live-request ceiling worth of active adapter operations,
and a separately named lower stream ceiling if upstream's v1 contract is lower.
Max+1 fails before allocating an upstream stream id.

## 9. Compatibility and migration

No mailbox schema/storage migration.

M012 may change the public API of `i2pr-mail-sam`, which is unreleased/internal and has
no consumer below the adapter seam. Preserve simple command/parser APIs where doing so
does not preserve the incorrect one-connection state model.

The new managed-app codec targets the language-neutral upstream contract, not Rust type
layout. Pin the exact i2pr commit used for fixtures in closure evidence, but do not add a
git dependency on i2pr.

M010's historical closure remains unchanged; M012 is the forward corrective authority
for SAM lifetime/version/port semantics.

## 10. Required tests

### Managed-app wire

- exact handshake bytes;
- all frame header fields and max/max+1;
- fragmentation/coalescing;
- duplicate/unknown JSON members;
- all required app→host and host→app controls;
- wrong-direction rejection;
- canonical instance-id matrix;
- request-id active/recent/stale behavior;
- stream-id open/data/close/reset lifecycle;
- data-before-open and data-after-close;
- foreign/duplicate stream ids;
- host EOF with active waiters;
- bounded queue saturation;
- cancellation and timeout during each blocking state.

### SAM corrective

- version parse/negotiate for 3.1, 3.2, 3.3 and no-overlap;
- `TO_PORT` exact 0, 25, 110, 65535 and max+1 rejection;
- one control connection SESSION CREATE;
- multiple data connections each HELLO + STREAM CONNECT;
- no SESSION CREATE on a data connection;
- data connection loss leaves control session live;
- control loss invalidates current generation;
- 3.1 peer + nonzero destination port fails before CONNECT;
- canonical STREAM STATUS success/failure;
- any retained alternate reply syntax backed by a pinned-peer fixture.

### Adapter/backend composition

- POP3 open emits `TO_PORT=110`;
- SMTP open emits `TO_PORT=25`;
- no local 7659/7660/7656 connection path exists;
- missing `sam` grant denies before SAM commands;
- two concurrent protocol connections share only the SAM session, not byte streams;
- synthetic backend sync/send scenarios still preserve POP3 deletion and SMTP
  `DeliveryUnknown` semantics;
- adapter/channel restart and cancellation do not leak workers, ids or buffers.

### Static negative evidence

Boundary guard must reject:

- `std::net` or DNS in the adapter;
- any i2pr git/workspace dependency;
- managed-app/SAM imports into lower mail crates;
- an unbounded channel constructor;
- direct access to stdin/stdout from more than the designated channel owner;
- local proxy literals 7656/7659/7660 in production transport code.

Every new guard gets a positive control proving it can fail.

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

Focused commands, adjusted to final crate names:

```text
cargo test --locked -p i2pr-mail-sam
cargo test --locked -p i2pr-mail-i2pr
```

Hosted CI must run the same deterministic floor on the exact closure head. M012 requires
no router, Postman account, or external network peer.

## 12. Documentation updates

- `docs/architecture/i2pr-integration.md` — current upstream runtime, managed-app wire
  ownership, correct SAM connection topology, 3.2+ port requirement and exact external
  blockers;
- `docs/architecture/transport-boundary.md` — application-facing stdio/multiplexer
  model and any ADR supersession;
- `README.md` only if crate inventory/verification commands change;
- `plans/registry.md`;
- `plans/subsystems/mail-backend-foundation-roadmap.md`;
- M006 implementation plan — replace stale "no runtime/channel" blocker with M012 +
  upstream SAM/368 + upstream Plan 385;
- `plans/closure/mail-backend-foundation/012-status.md`.

Do not rewrite M010 or M011 closure records.

## 13. Acceptance criteria

M012 closes only when:

- managed-app v1 application-role framing is locally implemented and bounded;
- one owner multiplexes logical streams over the application channel;
- `MailTransport` remains synchronous with no direct/loopback networking;
- SAM session ownership uses one long-lived control connection plus separate data
  connections;
- the client supports and tests `TO_PORT`/version semantics required by Postman;
- a 3.1-only host cannot accidentally produce a portless mail connection;
- synthetic managed-app integration carries the existing POP3 and SMTP scenarios;
- no i2pr Rust dependency leaks into this repository;
- boundary guards have demonstrated negative controls;
- exact-head hosted CI is green;
- M006's only remaining blockers are upstream SAM/368 and Managed native app runtime/385.

## 14. Stop conditions

Stop and register a corrective/ADR if:

- the real app-facing contract requires importing private i2pr Rust APIs;
- the only implementation needs a direct/loopback socket;
- managed-app framing cannot be multiplexed with bounded synchronous semantics while
  keeping `MailTransport` stable;
- the implementation needs an unbounded queue or one thread per byte stream without a
  hard global ceiling;
- SAM reference evidence contradicts the control/data connection model;
- current deployed peers require mutually incompatible STREAM reply/port grammar that
  cannot be handled narrowly;
- Postman is proven to accept port 0 directly, invalidating the port prerequisite — in
  that case record the executed evidence rather than preserving this assumption;
- work expands into live router qualification, package administration, sandboxing, GUI,
  clearnet or I2CP.

## 15. Closure evidence required

Closure must record:

- exact upstream i2pr commit and managed-app v1 reference read;
- exact SAM specification/reference revisions;
- corrective before/after connection-state diagram;
- Postman destination/port evidence;
- managed-app golden frame corpus;
- request/stream/queue ceiling matrix;
- SAM version/port transcript matrix;
- fake-host POP3/SMTP integration transcripts;
- cancellation/EOF/restart/backpressure evidence;
- boundary-guard mutation evidence;
- routine floor and hosted exact-head CI;
- security review and unresolved findings;
- unblock audit against upstream SAM/368 and Managed app runtime/385.

## 16. Handoff notes

M012 is deliberately the local work that upstream no longer needs to gate. It should
leave M006 small and evidence-oriented.

Do not preserve M010's one-connection SAM state machine for API compatibility. The
historical implementation was reasonable before a real adapter existed, but the final
transport must model actual SAM connection ownership.

Do not use the Java router's local POP3/SMTP proxies as a shortcut. Ports 7659 and 7660
are host TCP listeners; using them would violate the no-loopback authority boundary and
would make the managed-app sandbox meaningless.

When M012 closes, **do not automatically mark M006 ready**. Re-read upstream. M006 is
ready only when both the port-aware SAM server profile and the qualified `Secured`
runtime are closed and their exact contracts remain compatible with this adapter.
