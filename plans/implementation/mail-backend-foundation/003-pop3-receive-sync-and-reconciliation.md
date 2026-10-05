# Mail Backend Foundation Milestone 003 — POP3 Receive, Sync, and Reconciliation

Status: blocked

Repository baseline: planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; execute after M001 and M002 closure

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: capability

## 1. Objective

Deliver a robust receive path over an injected authorized byte stream: deterministic POP3 protocol machinery plus runtime synchronization, header-first fetch, durable offline mail, and restart-safe remote-deletion reconciliation.

## 2. Why this milestone is ready

Blocked on M001/M002. Once those close, protocol state can target stable domain/storage boundaries without coupling to i2pr.

## 3. Current implementation evidence

No POP3 implementation exists at planning time. Susimail behavior provides interoperability guidance: greeting/CAPA, USER/PASS, STAT, UIDL, LIST, TOP with RETR fallback, PIPELINING when supported, DELE, and QUIT transaction semantics.

## 4. Invariants that must not regress

- protocol engine is sans-I/O and deterministic;
- network access comes only from runtime MailTransport/injected duplex stream;
- USER/PASS values are redacted from diagnostics/transcripts;
- UIDL remains opaque and persistent; ordinal remains session-local;
- unknown/malformed server data is bounded and cannot cause unbounded allocation;
- initial sync is resumable/header-first;
- remote deletion is not considered committed solely because DELE returned OK;
- cancellation/disconnect leaves state reconcilable.

## 5. Scope

### In scope

Supported POP3 commands/state:

- greeting;
- CAPA;
- USER/PASS;
- STAT;
- UIDL;
- LIST;
- TOP n 0 where advertised;
- RETR fallback/full body;
- DELE;
- RSET;
- NOOP;
- QUIT;
- PIPELINING only where server capability and command ordering make it safe.

Runtime:

- one account POP3 session owner;
- manual/scheduled-call sync entrypoint, no background scheduler requirement;
- reconcile remote UIDL set against local store;
- fetch unknown headers first;
- on-demand or bounded requested body fetch;
- persist before optional remote delete;
- reconnect/retry with bounded policy for transport failures;
- typed sync progress/outcomes.

### Explicitly out of scope

- IMAP;
- APOP unless later interoperability evidence requires it;
- direct TLS/socket ownership;
- localhost tunnel fallback;
- background polling scheduler;
- UI notifications;
- generic provider discovery.

## 6. Required production changes

### Protocol machine

Implement a parser/state machine in i2pr-mail-proto that receives input bytes and emits command bytes/state events. It must support fragmented reads and multiple responses per input buffer.

Freeze explicit ceilings for response lines, multiline entries, capability count, UIDL length, aggregate metadata per sync batch, and diagnostic text. Standards/interoperability requirements must remain valid within those ceilings.

### Runtime transport seam

Define/implement MailTransport or equivalent in i2pr-mail-runtime with an operation to open the logical POP3 service stream. The trait/interface is authority-minimal and contains no generic arbitrary-host socket operation unless M006 later proves one is necessary and safe.

Tests use in-memory/scripted transport.

### Sync algorithm

On successful authentication:

1. CAPA and establish supported features.
2. STAT/UIDL/LIST to build current remote snapshot.
3. compare with durable (AccountId, Uidl) state.
4. fetch TOP headers for unknown mail where available; RETR fallback otherwise.
5. commit metadata/raw data before advancing cache state.
6. fetch full bodies according to explicit request/policy without blocking the whole mailbox from becoming usable.
7. process DeletePending with DELE in the same session.
8. only mark deletion committed after successful QUIT and/or subsequent UIDL reconciliation according to the frozen rule.

No code may infer UIDL ordering/meaning.

## 7. Ordered work packages

### A — POP3 transcript machine

Implement protocol grammar/state and synthetic transcript fixtures.

Acceptance evidence: fragmented/pipelined/multiline/malformed transcript tests.

### B — Runtime stream driver

Drive the sans-I/O machine over injected async byte streams with cancellation and timeouts.

Acceptance evidence: deterministic in-memory stream integration tests.

### C — Receive sync

Implement UIDL snapshot reconciliation and header-first persistence.

Acceptance evidence: empty/new/duplicate/reordered/server-removal scenarios plus restart.

### D — Deletion/recovery

Implement DeletePending, DELE, QUIT, disconnect ambiguity, and later reconciliation.

Acceptance evidence: failure at every transaction stage with correct durable post-restart state.

## 8. Failure, cancellation, restart, and contention semantics

Only one mutating POP3 transaction owns an account at a time. Concurrent read-only local-store access may continue.

Transport failure before authentication/snapshot leaves no remote-state advancement.

Failure during header/body retrieval retains any fully committed entities and retries only missing work.

Cancellation closes/drops the stream and records no false successful remote deletion.

DELE accepted + connection lost before successful QUIT remains pending/uncertain and is reconciled from UIDL next session.

Retries are bounded and cancellation-aware; no tight reconnect loop.

## 9. Compatibility and migration

No external protocol extension beyond standard POP3/Postman-compatible behavior is advertised. Persisted receive states must be representable by M002 schema; if a new state is required, add a migration explicitly rather than overloading an old value.

## 10. Required tests

### Protocol

- greeting success/error;
- CAPA with/without TOP/UIDL/PIPELINING;
- USER/PASS success/failure with secret redaction;
- fragmented response lines;
- multiline dot termination/dot unstuffing;
- STAT/UIDL/LIST parsing;
- TOP supported/fallback;
- RETR streaming;
- overlong/malformed response rejection;
- unexpected response/state transition.

### Sync/restart

- empty mailbox;
- many UIDLs with bounded memory behavior;
- new message header-first;
- body fetch after restart;
- reordered ordinals same UIDLs;
- remote disappeared UIDL;
- duplicate sync idempotence;
- disconnect at each delete stage;
- successful QUIT and later reconciliation.

### Security

- credential sentinel absent from logs/errors;
- no direct socket/name-resolution path in pure protocol;
- hostile UIDL cannot become a path.

## 11. Required verification commands

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-proto pop3
cargo test --locked -p i2pr-mail-runtime pop3
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

## 12. Documentation updates

- docs/architecture/pop3.md;
- docs/architecture/runtime.md;
- operator/developer notes for receive sync limits and delete policy;
- roadmap/registry;
- closure record 003-status.md.

## 13. Acceptance criteria

- a synthetic/in-memory Postman-compatible server can authenticate and synchronize mail through the runtime;
- unknown messages become visible from headers before all bodies are downloaded;
- full raw messages remain usable offline after restart;
- repeated sync is idempotent by Uidl;
- delete/disconnect cases never falsely report remote deletion;
- no direct host network path is required.

## 14. Stop conditions

Stop if:

- interoperability requires a nonstandard POP3 behavior that changes the service contract;
- the transport seam needs arbitrary host networking;
- receive sync requires parsing UIDL meaning;
- storage schema cannot represent uncertain delete/reconciliation safely;
- live Postman testing is the only way to prove core correctness—build deterministic fixtures first.

## 15. Closure evidence required

POP3 capability matrix, transcript fixtures, malformed/bounds matrix, sync/restart/delete state matrix, credential-redaction evidence, exact verification results, dependency review, and unblock audit for M005 (partial; M004 also required).

## 16. Handoff notes

Optimize for unreliable/disconnecting networks before optimizing raw throughput. Pipelining is an optional capability, not a correctness dependency.