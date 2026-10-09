# Mail Backend Foundation Milestone 012 — Managed-App V1 Application Client and Multiplexer

Status: ready for handoff

Repository baseline:

- i2pr-mail `main` `e51d35eed3d9e92509138b1ad08a7d897275960f`;
- M001-M005 and M007-M011 closed;
- upstream i2pr audited at `main` `acd752b7ce35111ab129a1fb86e4a7315df37a7f`;
- upstream managed-app Plans 368-371 and 382-383 closed.

Source roadmap:

- `plans/subsystems/mail-backend-foundation-roadmap.md`

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`
- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`

Primary class: corrective integration infrastructure + invariant

## 1. Objective

Implement the application side of i2pr managed-app v1 as a bounded, router-independent
Rust client/multiplexer above an injected duplex byte stream.

M012 corrects the stale M006 assumption that no application runtime/channel exists.
Current upstream now launches a real application through:

```text
i2pr-daemon
  -> inherited anonymous manager channel
  -> i2pr-appd
  -> i2pr-apphost
  -> application stdin/stdout (managed-app v1)
```

The mail repository therefore needs a client for the **language-neutral application
wire contract**, not access to private `i2pr-daemon` Rust types.

M012 deliberately stops below SAM. It produces an async logical `sam` service stream
that a later M013 can hand to the canonical `dbowm91/i2pr-sam` client once that
library's injected-connection provider (M018) is closed. This avoids deepening the
temporary `i2pr-mail-sam` implementation from M010.

## 2. Why this milestone is ready

All local prerequisites are closed through M011.

The application-facing contract is concrete and exercised upstream:

- Plan 368 defines and qualifies the private manager↔daemon bridge;
- Plan 369 ships `i2pr-appd`, `i2pr-apphost`, and the managed-app v1 application
  consumer;
- Plan 370 corrects the canonical decimal-string instance-id representation;
- Plan 371 closes optional/nonblocking runtime startup;
- Plans 382-383 supply signed packages, persistent trust/grants/exact selection,
  production autostart, and restart qualification.

Upstream's production catalog injects the launch identity as two reserved application
arguments:

- `--i2pr-app-id=<AppId>`;
- `--i2pr-app-instance=<canonical-u128-decimal>`.

The application's first wire traffic is the fixed application-role handshake followed by
exactly one `hello` carrying those values. On success the host sends a correlated
successful reply and then one `capabilities` event containing the effective
administrator-granted capability set.

No live router, SAM feature beyond the app-channel framing, or OS sandbox is necessary
to implement and deterministically verify this client.

## 3. Current implementation evidence

### The old M006 blocker is stale

M006 and `docs/architecture/i2pr-integration.md` were written when Plan 355's
`AppGatewaySession` existed only as `pub(crate)` infrastructure with no production
caller. That is historical evidence, not current architecture.

Current upstream has the trusted process/channel owner and production launch authority.
M012 must correct planning/documentation forward rather than rewriting historical
closure records.

### The current black-box fixture demonstrates the exact app-side flow

Upstream's independent application fixture already proves the sequence an external app
must implement:

1. application-role handshake;
2. hello with exact AppId/instance id;
3. successful reply;
4. one-shot effective capabilities;
5. `open { service = sam, stream_id }`;
6. correlated success;
7. opaque data frames on that nonzero stream id;
8. close/reset and host stream-ended events.

The fixture uses ordinary `std::io` and no router crate in the application process,
which is evidence that the wire contract is sufficient for an independent client.

### SAM ownership belongs elsewhere

The dedicated `dbowm91/i2pr-sam` implementation branch already has:

- correct retained SESSION CREATE control-socket ownership;
- a fresh connection for each STREAM CONNECT;
- typed `FROM_PORT`/`TO_PORT`;
- async and blocking client surfaces;
- SAM 3.3 Java-I2P live qualification.

Its remaining downstream gap is concrete `TcpStream` coupling. SAM-library M018 is now
registered to add an injected reliable-connection provider after M017 merges the
foundation. M012 must not reimplement that protocol/session machinery.

## 4. Invariants that must not regress

- No dependency on any unpublished i2pr Rust crate.
- No direct network, DNS, loopback, filesystem, process-launch, or router-control
  authority in the managed-app client crate.
- The client speaks only the documented application-role wire contract.
- Application-requested capabilities are never treated as grants.
- Only the host-issued one-shot `capabilities` set is effective authority.
- `sam` open is refused locally if effective `sam` capability is absent.
- Hello AppId/instance id must exactly match the trusted reserved launch arguments.
- Reserved launch arguments are consumed by the adapter and are not forwarded as mail
  product arguments.
- Exactly one owner reads the shared host channel and exactly one owner serializes writes.
- Logical streams never parse shared framing independently.
- Every frame, control payload, request id, stream id, queue and pending operation is
  bounded by the upstream v1 contract or a stricter named local ceiling.
- Unknown, duplicate, stale, wrong-direction and malformed inputs fail closed.
- Channel EOF/fatal framing error terminates the whole channel generation and wakes every
  waiter.
- One logical stream close/reset does not destroy healthy sibling streams.
- No unbounded channel constructor is used.
- Tokio/async machinery stays in the adapter layer; lower mail crates remain unchanged.
- No raw message body or credentials appear in diagnostics.

## 5. Scope

### In scope

- a new adapter-layer crate, expected package name `i2pr-mail-managed-app` unless a
  better non-conflicting name is justified;
- strict launch-context parser for the two reserved i2pr arguments;
- application-role handshake;
- bounded managed-app frame envelope codec;
- exact app→host and host→app control vocabulary needed for mail;
- hello/capability establishment;
- request/reply correlation;
- logical stream open/close/reset;
- async logical stream I/O over bounded queues;
- channel lifecycle/cancellation/backpressure;
- deterministic fake-host/adversarial tests;
- static dependency/authority guards;
- planning/docs correction of the stale M006 reachability narrative.

### Explicitly out of scope

- SAM command/session implementation;
- consuming `i2pr-sam` before its M018 closes;
- implementing `MailTransport`;
- live i2pr process execution;
- package installation/signing/admin CLI;
- Linux sandbox implementation;
- `UnsafeDirect` qualification;
- brokered clearnet;
- I2CP mail transport;
- Proposal 170;
- GUI/UI bridge.

## 6. Required production changes

### A — trusted launch-context parser

Add a small typed value:

```text
ManagedLaunchIdentity {
    app_id,
    instance_id,
}
```

with a parser for exactly the reserved forms currently frozen by upstream:

```text
--i2pr-app-id=<bounded AppId>
--i2pr-app-instance=<canonical decimal nonzero u128>
```

Requirements:

- each appears exactly once;
- malformed/duplicate/missing values fail before handshake;
- instance id rejects sign, whitespace, leading zero, exponent/fraction and >u128;
- the two reserved arguments are removed from the remaining application argument view;
- no environment variable or executable name is used as identity proof.

If upstream changes how trusted launch context reaches the application before M006,
record that as a contract change rather than accepting multiple implicit aliases.

### B — wire codec

Implement the application-role contract independently from upstream Rust representation.

Handshake:

- magic `I2PA`;
- major 1;
- minor 0;
- role Application;
- zero reserved bytes.

Frame envelope:

- version 1;
- kind 1 control / 2 data;
- zero flags;
- stream id 0 for control, nonzero for data;
- 32-bit declared payload length;
- payload <= 65,536 bytes;
- control JSON <= 16,384 bytes.

Control decoding must reject duplicate keys and unknown fields before a semantic message
is produced.

Required app→host messages:

- `hello`;
- `open`;
- `close`;
- `reset`.

Required host→app messages:

- correlated `reply`;
- one-shot `capabilities`;
- `stream_closed`;
- `stream_reset`;
- `health`.

`permission_request` is deliberately not required for initial mail operation. Mail
starts only with pre-existing administrator grants.

### C — session establishment

The client performs:

1. write application handshake;
2. send one hello request using trusted launch identity;
3. require the reply to correlate exactly and succeed;
4. require the next authority-bearing event to be one `capabilities` value;
5. freeze that set as read-only effective capability state.

A second hello, duplicate capabilities event, permission mutation, or data before an
opened stream is a fatal contract error.

### D — bounded multiplexer

Use a single channel owner.

A suitable architecture is:

- one reader task parsing all inbound frames;
- one serialized bounded writer queue;
- at most 64 active request ids;
- at most 128 live logical streams;
- bounded per-stream inbound byte/chunk queue;
- bounded aggregate queued bytes, named explicitly;
- monotonic nonzero request/stream ids with deterministic exhaustion behavior rather than
  wraparound aliasing;
- exact terminal ownership for pending requests and streams.

Use bounded Tokio channels or an equivalent with explicit capacity. Do not use
`tokio::sync::mpsc::unbounded_channel`, `std::sync::mpsc::channel`, or a hidden
unbounded VecDeque.

### E — async logical stream

Expose a `ManagedLogicalStream` implementing `AsyncRead + AsyncWrite + Unpin + Send`
(or a reviewed equivalent) over one nonzero managed-app stream id.

Writes frame opaque bytes as data for that id. Reads consume only data routed to that id.

Drop/close:

- sends close at most once when the channel is healthy;
- releases local queue/id ownership regardless of host state;
- never blocks indefinitely in Drop.

Host `stream_closed` produces EOF; `stream_reset` produces a typed/reset I/O failure.

### F — service opener

Expose an async method such as:

```text
open_service(AppService::Sam) -> ManagedLogicalStream
```

Before allocating an id or issuing `open`, require the corresponding effective
capability.

For M012, `sam` is the only required service. Keep the service enum capable of
representing the frozen v1 names without accidentally making unsupported services
usable.

### G — no ambient stdio in the protocol engine

The protocol/multiplexer constructor takes injected async I/O. This is what keeps tests
deterministic and prevents a second hidden owner.

Production stdin/stdout composition belongs to M013/M006 and must be tiny and obvious.

## 7. Ordered work packages

### WP1 — contract and launch-context freeze

Pin current upstream managed-app v1 and Plan-383 launch-context behavior. Update the
architecture docs to mark the 2026-10-06 "no app runtime" statement superseded.

Acceptance: exact wire/launch-context matrix and no unresolved identity input.

### WP2 — codec

Implement handshake, frame and control codecs with bounds and duplicate/unknown-field
rejection.

Acceptance: golden bytes plus exhaustive malformed/max+1/directional fixtures.

### WP3 — client establishment and authority

Implement hello correlation and one-shot capability establishment.

Acceptance: wrong identity, wrong reply id, failed reply, duplicate capabilities and
missing `sam` grant all fail closed.

### WP4 — multiplexer and logical streams

Implement bounded reader/writer ownership, request/stream ledgers, data routing,
close/reset and backpressure.

Acceptance: fragmented/coalesced fake-host traffic, sibling isolation, max+1 ceilings,
EOF and reset tests.

### WP5 — lifecycle/adversarial review

Drive cancellation, task failure, queue saturation, stale ids, duplicate events and
channel teardown.

Acceptance: no leaked task/id/queue ownership and deterministic error outcomes.

### WP6 — guards/docs/closure

Extend the repository boundary checker, update planning/docs, run hosted verification,
and write M012 closure.

## 8. Failure, cancellation, restart, and contention semantics

### Fatal channel errors

Wrong handshake behavior, malformed frame, impossible control direction/correlation,
oversized input or underlying EOF makes the channel generation terminal.

Terminal transition:

- stop new opens;
- wake every request waiter;
- terminate every logical stream;
- drain/drop bounded queues;
- join/cancel owned tasks;
- never reconnect stdin/stdout internally.

A new application launch is a new channel generation owned by i2pr, not by this client.

### Logical stream failure

Host close affects only the named stream. Host reset affects only the named stream unless
the host itself then closes the whole channel.

A local stream write after terminal close/reset returns typed failure and emits no new
frame.

### Cancellation

Client-level cancellation shuts down the writer/reader owners and all streams.

Per-open cancellation before success releases its request/stream ids. If an open request
may already have succeeded remotely when cancellation wins a race, send a best-effort
close/reset for the reserved stream id while still releasing local ownership
deterministically.

### Backpressure

Writer and inbound queues block/wake under explicit capacity and cancellation. They never
grow to absorb a stalled peer.

One slow logical stream must not consume the entire aggregate budget indefinitely; define
a per-stream ceiling and a channel aggregate ceiling.

## 9. Compatibility and migration

No mailbox/schema migration.

New adapter-layer crate only. Existing `mail-domain`, `mail-mime`, `mail-proto`,
`mail-store`, `mail-runtime`, and `i2pr-mail-sam` APIs remain unchanged in M012.

The M010 SAM crate is intentionally retained during this milestone so M012 does not mix
wire-client work with protocol-owner migration. M013 decides/removes that duplication
after canonical `i2pr-sam` M018 is consumable.

The managed-app implementation targets the language-neutral contract, not serde enum
layout or any upstream Rust API.

## 10. Required tests

At minimum:

- launch identity exact/duplicate/missing/malformed matrix;
- handshake exact bytes and wrong magic/version/role/reserved values;
- frame exact/max/max+1/truncated/coalesced/fragmented cases;
- duplicate and unknown control JSON fields;
- every required directional control variant;
- wrong-direction variant rejection;
- hello success and every correlation/identity failure;
- capabilities exactly once;
- open without grant rejected before id allocation;
- request id exact ceiling/max+1/stale/duplicate;
- stream id exact ceiling/max+1/stale/duplicate;
- data-before-open/data-after-close/foreign-stream rejection;
- stream close versus reset semantics;
- sibling isolation;
- inbound/outbound queue exact capacity/max+1;
- aggregate byte budget;
- reader failure/writer failure/channel EOF;
- cancellation at hello, open, read, write and capacity wait;
- repeated open/close soak returns counters to baseline;
- error/debug output contains no data payload or mail credential sentinel;
- guard mutation tests proving prohibited network/unbounded-channel edges fail.

## 11. Required verification commands

Routine floor remains:

```text
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

Focused command, adjusted to the final crate name:

```text
cargo test --locked -p i2pr-mail-managed-app
```

Hosted CI must run the deterministic floor at the exact closure head. No router or
external network fixture is required.

## 12. Documentation updates

- `docs/architecture/i2pr-integration.md` — current upstream runtime/channel truth and
  M012/M013 dependency split;
- `docs/architecture/transport-boundary.md` — managed-app logical-stream seam;
- M006 plan — replace obsolete reachability blocker with M013 + upstream capability
  gates;
- `plans/registry.md`;
- `plans/subsystems/mail-backend-foundation-roadmap.md`;
- `plans/closure/mail-backend-foundation/012-status.md`.

Do not rewrite M010/M011 closure records.

## 13. Acceptance criteria

M012 closes only when:

- launch context is parsed from the exact trusted reserved arguments;
- managed-app v1 application framing is locally implemented without upstream Rust deps;
- hello/capability establishment is exact and fail-closed;
- one bounded owner multiplexes logical streams over injected async I/O;
- `sam` service opens yield isolated `AsyncRead + AsyncWrite` logical streams;
- missing grants/malformed host behavior fail before authority is used;
- no direct/loopback/network capability exists in the crate;
- queues/tasks/ids remain bounded and cancellation-safe;
- boundary guards demonstrate their negative controls;
- exact-head hosted CI is green;
- M013 becomes the sole local integration successor.

## 14. Stop conditions

Stop and register a corrective if:

- the implementation requires importing `i2pr-app-proto` or another private upstream
  crate;
- upstream launch identity no longer arrives through a single documented trusted form;
- the wire contract requires an unbounded queue or ambiguous ownership;
- a logical stream cannot implement async byte semantics without leaking shared frames;
- direct stdin/stdout access would need to exist in more than one owner;
- implementation scope expands into SAM, sandbox, package administration, GUI, clearnet
  or I2CP.

## 15. Closure evidence required

Record:

- exact upstream i2pr commit/reference files;
- launch-context grammar;
- wire golden fixtures;
- control-direction matrix;
- request/stream/queue ceilings;
- hostile framing and duplicate-key evidence;
- cancellation/EOF/backpressure/soak results;
- guard mutation results;
- local and hosted deterministic floor;
- security review and unresolved findings;
- successor audit for M013.

## 16. Handoff notes

Do not build another SAM client in this milestone.

The M010 `i2pr-mail-sam` code remains historical/local scaffolding until M013. The
canonical protocol/session owner is now `dbowm91/i2pr-sam`; its M018 provider seam is
the missing abstraction that lets a managed-app logical `sam` stream replace a host
TCP SAM connection.

M012 can execute while upstream i2pr Plan 385 establishes private app data/workspace and Plan 386 subsequently establishes Linux `Secured` containment. M012 does not claim secured operation merely because the wire client is complete.
