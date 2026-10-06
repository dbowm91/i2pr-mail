# Mail Backend Foundation Milestone 010 — SAM 3.1 Client Codec and Downstream Contract Qualification

Status: closed — see `plans/closure/mail-backend-foundation/010-status.md`

Repository baseline: post-M009 planning head `7bfb127`

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/002-long-term-roadmap.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md
- plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md

Primary class: capability + infrastructure

## 1. Objective

Remove the two remaining *decision* dependencies of the i2pr integration line while leaving the transport itself blocked, by landing the part of M006 work package A that upstream status no longer gates:

1. A deterministic, sans-I/O SAM 3.1 client codec covering exactly the outbound mail path — `HELLO VERSION`, `NAMING LOOKUP`, `SESSION CREATE`, `STREAM CONNECT`.
2. The recorded M006 §7A downstream contract and authority matrix.

The codec is the client half of the adapter. Upstream i2pr explicitly assigns SAM client implementation to a separate repository and will never supply it, so this work is ours regardless of how the managed-app runtime resolves.

M010 deliberately does **not** implement `MailTransport`, does not open a socket, and does not unblock M006.

## 2. Why this milestone is ready

It is ready.

Hard dependencies:

- M007, M008, and M009 are closed with accepted closure records;
- hosted deterministic verification is green on run `37376165523`;
- ADR-0002 is accepted, so the execution-model question that M008 deferred is now decided by evidence rather than speculation.

Interface dependencies in dbowm91/i2pr — all satisfied or explicitly out of scope:

- Plans 345, 349, 352, 353, 354, and 355 are closed on `main` `2f82c799`;
- Plan 355's frozen stream mapping is stable and quotable: one managed-app logical stream binds to exactly one raw SAM or I2CP protocol connection, carrying exact protocol octets in order, with no encoding added or rewritten;
- the SAM 3.1 wire contract is externally stable and versioned, so the codec can be qualified against the specification and against upstream's implementation without waiting for a router to exist;
- the app-side runtime that actually blocks M006 has no registered upstream plan, so no part of this milestone depends on it.

## 3. Current implementation evidence

Upstream i2pr SAM implementation, read at `origin/main` `2f82c799`:

- `crates/i2pr-api/src/sam/version.rs` advertises exactly one version, `3.1`, and rejects malformed, whitespace-contaminated, multi-component, and overflowing version strings.
- `crates/i2pr-api/src/sam/reply.rs` defines the result vocabulary and the reply kinds `HELLO REPLY`, `DEST REPLY`, `SESSION STATUS`, `STREAM STATUS`, `NAMING REPLY`, `PONG`.
- `crates/i2pr-daemon/tests/sam_loopback.rs` asserts the observable wire forms `HELLO REPLY RESULT=OK VERSION=3.1`, `SESSION STATUS RESULT=OK DESTINATION=…`, and `RESULT=OK` on stream connect.
- `specs/references/portable-service-tunnel-sam-adapter-handoff.md` assigns "SAM HELLO/version/capability negotiation; command/reply codecs and state machines; STREAM connect, accept, and forwarding; … async runtime and blocking facade" to a *future separate repository*. i2pr will not ship a SAM client.

Verified against the SAM v3 specification (<https://i2p.net/en/docs/api/samv3/>):

- `HELLO VERSION MIN=3.1 MAX=3.1` is answered by `HELLO REPLY RESULT=OK VERSION=3.1`;
- `NAMING LOOKUP NAME=$name` is answered by `NAMING REPLY RESULT=$result NAME=$name [VALUE=$destination]`;
- base64 uses the I2P alphabet `A–Z a–z 0–9 - ~` with `=` padding.

One compatibility divergence is recorded rather than papered over: the SAM v3 specification documents the `STREAM CONNECT` success reply as `SAM $session:$stream OK`, while upstream i2pr emits `STREAM STATUS RESULT=OK`. The codec therefore accepts **both** shapes and normalizes them to one typed outcome. This is deliberate: the mail client must interoperate with whichever router answers it, and must not silently accept a malformed line to do so.

The name resolution requirement is not optional convenience. SAM 3.1 has no `HOSTNAME=` option on `STREAM CONNECT` — that arrived in 3.2 — so a client must resolve `pop.postman.i2p` to a base64 destination with `NAMING LOOKUP` first. This is exactly why the milestone scope is the three-command outbound path.

## 4. Invariants that must not regress

- no direct std/tokio TCP socket path, and no socket of any kind, in the codec;
- no localhost 7656/7659 dependency and no loopback fallback;
- no async runtime, executor, or `async fn` in the codec or below the transport seam;
- no filesystem access and no ambient clock in the codec;
- lower mail crates remain unaware of SAM; SAM must not leak into `mail-proto`, `mail-store`, or `mail-domain`;
- exactly one transport authority; the codec opens nothing and is handed bytes;
- credentials never enter the codec, and no codec error type may carry a secret;
- hostile or malformed peer input fails closed with a typed error and is never coerced into a valid value.

## 5. Scope

### In scope

- a new standalone `i2pr-mail-sam` crate with no project and no external dependencies;
- SAM 3.1 version negotiation, bounded command builders, bounded reply parsing;
- a deterministic client state machine covering hello, session establishment, and stream connection;
- the M006 §7A downstream contract and authority matrix in `docs/architecture/i2pr-integration.md`;
- a boundary guard extension covering the new crate.

### Explicitly out of scope

- implementing `MailTransport` or any adapter (that is M006, still blocked);
- `SESSION ADD`/`REMOVE`, `STREAM ACCEPT`, `STREAM FORWARD`, `DATAGRAM`, `RAW`, `PING`/`PONG`, `QUIT`, `DEST GENERATE`, or `AUTH`;
- private-destination generation or persistent destination storage;
- any socket, tunnel, lease, or router-internal type;
- any change to `MailTransport`, `ByteStream`, `OperationControl`, `TransportError`, or `BackendService`;
- M006 itself, and any claim that the transport line is unblocked.

## 6. Required production changes

### New crate

`crates/mail-sam` publishes package `i2pr-mail-sam`, depends on nothing, and carries `[lints] workspace = true`. It is deliberately **not** a dependency of `mail-runtime`: it sits at or above the transport seam so that the future adapter can use it without any existing crate gaining a router-protocol dependency.

Modules:

- `version.rs` — `SamVersion`, strict parsing, `negotiate`.
- `reply.rs` — `SamResult`, `SamReplyKind`, `SamReply`, bounded `parse_reply`, typed extraction helpers.
- `client.rs` — `SamClient`, `SamEvent`, `SamClientState`, the four command builders, `handle_reply`.

### Bounds enforced before formatting

Every bound is checked before a command line is produced, so an over-budget value is rejected instead of truncated. A rejected value never becomes a partial command.

- `MAX_LINE = 8192`, `MAX_TOKEN_COUNT = 64`, `MAX_OPTION_COUNT = 32`;
- `MAX_NAME_LEN = 256`, `MAX_SESSION_ID_LEN = 256`, `MAX_DESTINATION_LEN = 1024`;
- names and session ids must be bounded, non-empty, whitespace-free, quote-free, backslash-free, control-free ASCII — this is the injection guard, since a caller-supplied name is otherwise able to inject a second SAM token or split one command into two;
- destinations must use the I2P base64 alphabet `A–Z a–z 0–9 - ~` plus `=` padding; `+` and `/` are rejected because a router will not accept them.

### Two accepted reply shapes

`parse_reply` accepts both `STREAM STATUS RESULT=OK` and `SAM <session>:<stream> OK` for a stream-connect success, normalizing to one `SamReply`. It rejects everything else. This is the only place the codec tolerates a documented divergence, and it does so narrowly and with a comment naming the divergence.

### Boundary guard

`scripts/check-boundaries.sh` gains `i2pr-mail-sam` to the expected dependency map with an empty allowed set, and to the sans-I/O source scan, so a future `std::net`, `tokio::`, `std::fs`, or `std::process` reference is rejected structurally rather than by review.

## 7. Ordered work packages

### A — Version negotiation

Acceptance evidence: strict parse matrix and fail-closed negotiation tests.

### B — Bounded command builders

Acceptance evidence: exact-byte transcript assertions plus an injection matrix proving a crafted name cannot add or split a token.

### C — Bounded reply parsing

Acceptance evidence: per-kind parse tests, quoted and escaped value tests, both stream-reply shapes normalizing identically, and unknown-result fail-closed tests.

### D — Client state machine

Acceptance evidence: ordered-transition tests, out-of-order rejection tests, and a full hello → session → connect transcript.

### E — Downstream contract qualification (M006 §7A)

Acceptance evidence: the interface and authority matrix in `docs/architecture/i2pr-integration.md`, citing exact upstream commits and quoting the closed contract.

## 8. Failure, cancellation, restart, and contention semantics

The codec holds no I/O and therefore has no contention of its own; it is a pure function of client state and input bytes.

- Every parse failure is a typed `SamReplyError`. No failure is silently repaired, and an unrecognised result token is never mapped to `Ok`.
- `negotiate` fails closed when the client's range and the router's support do not overlap. It never returns "the nearest version".
- The codec keeps no durable state. A restart begins again at `Unestablished`, and a stale session id is never reused because none is persisted.
- The codec owns no clock and no retry timing. Deadline and cancellation remain `OperationControl`'s responsibility at the seam, and exactly one layer owns retry timing; a codec cannot introduce a second clock because it has no clock.
- Capability denial remains a typed permanent failure when it eventually reaches M006. Nothing here retries anything.

## 9. Compatibility and migration

- New crate, no existing public API changes. `MailTransport`, `ByteStream`, `OperationControl`, `TransportError`, and `BackendService` are untouched.
- The codec targets SAM 3.1, which upstream i2pr advertises as its only supported version and which the SAM v3 documentation describes as "the recommended minimum SAM implementation". No 3.2 or 3.3 feature is used, so the codec is also usable against Java I2P and i2pd, which matters because i2pd "does not currently support most 3.2 and 3.3 features".
- No persistence and no schema version.
- If upstream's stream-reply divergence is corrected later, the accepted-shape logic narrows; the normalized outcome does not change, so no caller is affected.

## 10. Required tests

- version parse accept/reject matrix, including whitespace, control bytes, missing/extra separators, non-decimal and overflowing components;
- negotiation overlap and each no-overlap case, including major-version mismatch;
- every command builder's exact bytes including the trailing newline;
- rejection of empty, over-length, whitespace, quote, backslash, control-byte, and non-ASCII names and session ids;
- injection attempts that try to append a token or split a command, proving rejection and an unchanged single-command line;
- destination alphabet acceptance and rejection, including a realistic 516-character I2P-alphabet destination and rejection of `+` and `/`;
- the `MAX_LINE` boundary accepted at 8192 and rejected at 8193, with no truncation anywhere;
- per-reply-kind parsing including quoted values, escaped quotes, and `=` inside a value;
- both stream-connect reply shapes parsing to the same normalized outcome;
- unknown result token failing closed;
- out-of-order state transitions and each rejection;
- a full hello → session create → stream connect transcript asserting exact bytes.

## 11. Required verification commands

Minimum local floor, unchanged:

```
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

Focused:

```
cargo test --locked -p i2pr-mail-sam
```

## 12. Documentation updates

- `docs/architecture/i2pr-integration.md` — new: the M006 §7A downstream contract and authority matrix;
- `docs/architecture/transport-boundary.md` — record that the execution-model question is answered by ADR-0002 rather than deferred;
- `README.md` — verification section only, if the crate list is enumerated there;
- `plans/registry.md` — register M010 as dependency-ready;
- `plans/subsystems/mail-backend-foundation-roadmap.md` — register M010 and its place in the dependency graph;
- `plans/closure/mail-backend-foundation/010-status.md` — the closure record.

## 13. Acceptance criteria

- `i2pr-mail-sam` compiles and tests with zero dependencies and no sans-I/O violation;
- every outbound command M006 needs is encoded correctly and bounds are enforced before formatting;
- every documented SAM 3.1 reply M006 will encounter is parsed into a typed value, with the documented stream-reply divergence handled explicitly;
- the client state machine rejects out-of-order use;
- the M006 §7A interface and authority matrix exists, cites exact upstream commits, and states plainly that M006 remains blocked;
- no existing public API changed and no existing closure evidence is invalidated.

## 14. Stop conditions

Stop and re-plan if:

- the codec would need to open a socket, spawn a task, read a clock, or touch the filesystem;
- `mail-proto`, `mail-store`, `mail-domain`, or `mail-runtime` would need to depend on the codec or on any router-protocol type;
- implementing M006 §7A would require inventing an upstream contract rather than quoting a closed one;
- the accepted stream-reply shapes would have to grow into general "parse anything" tolerance;
- the work would require claiming M006 is unblocked, or presenting this codec as a transport implementation;
- upstream's SAM surface changes in a way that invalidates the recorded contract matrix.

## 15. Closure evidence required

Requirement-to-evidence mapping; exact commands and results for the focused and full floors; an injection-review showing a caller-supplied value cannot alter command structure; a fail-closed review of every parse path; confirmation that the boundary guard rejects an injected violation; the contract matrix with upstream commit citations; an unblock audit against `plans/registry.md` recording that M006 remains blocked and why.

## 16. Handoff notes

M010 is intentionally the part of the i2pr line that upstream cannot gate. It leaves M006 blocked on an upstream app-side runtime that is, as of 2026-10-06, "eligible for a future plan" and unregistered.

The loopback SAM listener in i2pr (`127.0.0.1:7656`, disabled by default, with no password or credential field in `SamConfig`) would permit outbound mail today and is deliberately **not** used. `ADR-0001`, `docs/architecture/transport-boundary.md`, and the repository architectural constraints all forbid a loopback fallback, because it reintroduces a second, unmanaged network authority. M010 does not revisit that decision; if it should be revisited, that is an ADR supersession question, not a milestone.

The codec written here is the client half only. The server half, the process channel, and the trusted runtime all remain upstream's responsibility.