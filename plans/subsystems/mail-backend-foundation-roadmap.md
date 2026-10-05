# Mail Backend Foundation Roadmap

Status: active

Long-term references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md

Related ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

## 1. Purpose and ownership boundary

Build a robust frontend-neutral Rust backend for I2P in-network mail, initially targeting the Postman-compatible POP3 and SMTP services used by Susimail.

This repository owns mail domain semantics, MIME handling, POP3/SMTP behavior, local mailbox/outbox persistence, retry/reconciliation state, and the frontend-neutral mail service API.

i2pr owns router networking, managed-app process isolation, capability authorization, and any SAM/I2CP/Proposal-170 adapter on the router side. i2pr-mail consumes only the final authorized application transport contract.

## 2. Work classification

### Invariants

- no direct clearnet/LAN/loopback dependency in the secured product path;
- credentials and authentication payloads never enter ordinary logs or mailbox persistence;
- UIDL is opaque; POP3 ordinal is transient;
- POP3 deletion and SMTP ambiguity are restart-safe states;
- outbound generated headers do not leak host/client/router identity;
- lower pure crates do not own sockets/tasks/filesystem state;
- frontend code cannot become a second network authority.

### Capabilities

- receive/synchronize I2P mail;
- read mail offline from durable local state;
- compose and submit I2P mail with attachments;
- manage drafts, sent state, local deletion, and remote POP3 deletion policy;
- expose a backend API suitable for a later GUI;
- run through i2pr managed-app capabilities when that interface is stable.

### Infrastructure

- domain vocabulary and bounded identifiers;
- MIME parser/builder facade;
- SQLite metadata + immutable blob store;
- deterministic POP3/SMTP state machines;
- abstract authorized byte-stream transport;
- verification and dependency-direction guards.

### Polish

Search, richer mailbox organization, HTML rendering, diagnostics, attachment sanitization, and UX follow functional backend closure.

## 3. Non-goals

This workstream does not build a GUI, generic IMAP client, Gmail/Exchange integration, OAuth ecosystem, conventional clearnet mail client, localhost webmail server, calendar/contact sync, automatic E2E crypto product flow, or unrestricted network broker.

Proposal 170 administration is not required for basic mail operation.

## 4. Current state

At the planning baseline the repository contains only the project README and this planning bootstrap; no production mail code, schema, compatibility contract, or released artifact exists.

The target i2pr native-app branch has a runtime-neutral application contract but no production app manager/gateway. i2pr Plan 349 is correcting direction/reply semantics and reserving the underspecified generic brokered-TCP open path. The router-side application transport remains a future interface dependency.

Therefore M001-M005 are intentionally implementable without i2pr. M006 is blocked until the downstream interface is stable.

## 5. Target architecture

Frontend, later
    |
backend service API
    |
mail-runtime
    |------ mail-store ---- SQLite metadata + immutable blobs
    |------ mail-mime  ---- pure parse/build/sanitize boundary
    |------ mail-proto ---- sans-I/O POP3 and SMTP machines
    |------ mail-domain --- stable identities and states
    |
MailTransport
    |
i2pr managed-app adapter, later
    |
authorized I2P application stream
    |
pop.postman.i2p:110 / smtp.postman.i2p:25

Recommended crate boundaries:

- i2pr-mail-domain
- i2pr-mail-mime
- i2pr-mail-proto
- i2pr-mail-store
- i2pr-mail-runtime
- final backend executable/service only when M005 needs it

The precise package names may be adjusted during M001 if Cargo naming ergonomics justify it, but ownership direction must remain intact.

## 6. Dependency graph

M001 workspace/domain/boundaries
 |
 +--> M002 MIME + durable store
         |
         +--> M003 POP3 receive/sync
         |
         +--> M004 SMTP compose/outbox
                 \       /
                  \     /
                   M005 backend service convergence
                             |
                             +--> M006 i2pr adapter
                                  also requires stable i2pr app gateway

Dependency classification:

- M002 hard-depends on M001.
- M003 hard-depends on M001/M002.
- M004 hard-depends on M001/M002.
- M005 hard-depends on M003/M004.
- M006 hard-depends on M005 and interface-depends on corrected/stable i2pr managed-app transport.
- Frontend work is soft/deferred and begins only after M005 establishes a stable backend surface.

## 7. Milestones

### M001 — Workspace, domain, and dependency boundary

Class: invariant + infrastructure.

Objective: establish the Rust workspace, domain vocabulary, crate ownership, bounded types, and static dependency rules without claiming mail capability.

Exit: domain states and dependency guards are tested; no socket/database/frontend owner appears in pure crates.

### M002 — MIME and durable storage

Class: infrastructure.

Objective: persist raw entities and transactional mail/outbox metadata safely, with a pure MIME facade and restart/migration tests.

Exit: raw message round-trip, schema migration, atomic blob commit, corruption/rollback, and credential non-persistence evidence pass.

### M003 — POP3 receive and reconciliation

Class: capability.

Objective: support deterministic POP3 receive/sync over an injected stream, including CAPA, auth, UIDL/LIST, header-first fetch, body fetch, pipelining where safe, and restart-safe deletion.

Exit: deterministic transcripts and runtime integration demonstrate offline durable mail without direct networking.

### M004 — SMTP compose and submission

Class: capability.

Objective: construct privacy-safe MIME mail and submit it through deterministic SMTP with auth, SIZE/capability handling, attachments, outbox durability, and DeliveryUnknown semantics.

Exit: successful, safe-retry, and ambiguous-delivery transcript/restart scenarios pass.

### M005 — Backend service convergence

Class: capability.

Objective: expose the complete backend through bounded frontend-neutral commands/events and content handles, with one coherent lifecycle owner.

Exit: an end-to-end fake-transport scenario can sync, list, read, draft, send, restart, and reconcile without UI or network privileges.

### M006 — i2pr managed-native-app adapter

Class: capability/integration.

Objective: replace fake transport with the stable authorized i2pr application transport without changing mail-domain/protocol/store contracts.

Blocker: corrected i2pr app contract plus production router-side app-principal transport gateway.

Exit: real adapter smoke/qualification plus negative evidence that no direct/loopback fallback is required.

## 8. Cross-cutting requirements

### Storage and migration

Migrations are monotonic and tested from every released schema once releases exist. Filesystem blobs use validated opaque paths and atomic commit. Database rows never embed secrets.

### Protocol and compatibility

All line/frame/message sizes are bounded before allocation. Protocol parsers reject malformed multiline replies, overlong lines, invalid response codes, and unexpected state transitions without panic.

### Security and authorization

Only MailTransport may obtain network authority. HTML rendering and attachments do not imply network permission. Authentication material is redacted from traces.

### Concurrency, cancellation, and recovery

One account must not have conflicting concurrent POP3 mutation transactions. Cancellation closes the owned stream and leaves durable state reconcilable. SMTP ownership prevents duplicate simultaneous submission of one OutboxId.

### Observability and audit

Diagnostics expose typed state/outcomes, not credentials or raw message bodies. Transcript fixtures use synthetic content.

### Performance and resource use

Large mail must stream to/from bounded buffers/files where practical. Initial sync is header-first and resumable; no design may require loading an entire mailbox into memory.

### Documentation and operations

Architecture docs must explain transport authority, storage recovery, privacy-header policy, and known limits.

## 9. Verification strategy

Use deterministic POP3/SMTP transcripts, malformed-input/property tests, restart fixtures, SQLite migration tests, partial-file/transaction failure tests, privacy-header assertions, dependency-direction guards, and a final fake-transport end-to-end backend scenario.

Live I2P/network tests are not routine CI prerequisites. M006 may add an explicitly classified local/manual or supported integration lane once the i2pr gateway exists.

## 10. Risks and decision points

- MIME dependency choice must pass dependency/security review before M002 implementation.
- SQLite crate choice and async ownership must avoid blocking-runtime misuse.
- POP3 servers may vary in CAPA/TOP/PIPELINING behavior; M003 must degrade within the supported POP3 contract rather than assuming Postman extensions.
- SMTP failure after DATA is inherently ambiguous and must remain visible.
- The i2pr application gateway may expose SAM/I2CP rather than a destination-stream API; M006 must adapt above MailTransport rather than push that concern downward.
- If the managed-app gateway requires direct network/loopback to reach Postman, M006 stops and reports instead of weakening ADR-0001.

## 11. Completion definition

This roadmap closes only when M001-M006 have evidence-based closure, the backend is independently functional, and i2pr integration preserves the no-direct-network authority boundary. GUI completion is not required.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| M001 | ready | plans/implementation/mail-backend-foundation/001-workspace-domain-and-boundary-foundation.md | — | none |
| M002 | blocked | plans/implementation/mail-backend-foundation/002-mime-and-durable-storage.md | — | M001 |
| M003 | blocked | plans/implementation/mail-backend-foundation/003-pop3-receive-sync-and-reconciliation.md | — | M001, M002 |
| M004 | blocked | plans/implementation/mail-backend-foundation/004-smtp-compose-outbox-and-submission.md | — | M001, M002 |
| M005 | blocked | plans/implementation/mail-backend-foundation/005-backend-service-convergence.md | — | M003, M004 |
| M006 | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | — | M005 + stable i2pr app transport |
