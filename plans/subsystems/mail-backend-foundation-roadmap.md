# Mail Backend Foundation Roadmap

Status: active

Long-term references:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md

Related ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md
- plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md

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

M001-M005 are implemented and closed. The post-M005 corrective sequence M007-M009 is also closed, including hosted verification, and M010 closed the isolated SAM 3.1 client codec plus downstream contract qualification.

The qualified foundation is on `main`. M011 integrated `codex/foundation-planning` by a history-preserving exact-head fast-forward from `9f2cb7c7` to `64cd8121`, and every M001-M005 and M007-M010 commit remains reachable from `main` at its original SHA, so the evidence cited by the existing closure records is still valid. Integration-head hosted CI is green on run `37491896332`. The work branch `codex/foundation-planning` has been retired; its name survives only as history and as lineage documented in `plans/closure/mail-backend-foundation/011-status.md`.

The former M006 reachability blocker is now cleared upstream. Re-audited against i2pr `main` `acd752b7`: Plans 368–371 and 382–383 supply the inherited daemon↔manager channel, `i2pr-appd`, `i2pr-apphost`, the managed-app v1 application consumer, persistent signed-package trust/grants/catalog, and black-box private SAM/I2CP streams with no loopback listener.

That progress exposed a cleaner cross-repository integration sequence rather than making M006 immediately executable. M012 now owns only the independent managed-app v1 application client/multiplexer. The dedicated `dbowm91/i2pr-sam` repository already owns the correct SAM STREAM control/data connection lifecycle and typed FROM_PORT/TO_PORT behavior; its M018 is registered to remove concrete TCP coupling through an injected reliable-connection provider after M017 merges the SAM foundation. M013 is therefore registered behind M012 + i2pr-sam M018 to adopt the canonical SAM client, retire the temporary mail-local SAM implementation, and compose the synchronous MailTransport. Final M006 then remains gated only on M013 plus upstream i2pr SAM/368 port-aware server support and Managed native app runtime/386's qualified Linux `Secured` backend; `UnsafeDirect` and localhost proxies remain forbidden.

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
                             +--> M007 corrective correctness
                                   |
                                   v
                                  M008 runtime decomposition
                                   |
                                   v
                                  M009 hosted qualification
                                   |
                                   v
                                  M010 SAM 3.1 client codec
                                   + contract qualification
                                   |
                                   v
                                  M011 foundation branch integration
                                   |
                                   v
                                  M012 managed-app v1 client/multiplexer
                                   |
                                   v
                                  M013 canonical i2pr-sam + MailTransport
                                   ^                         |
                                   |                         v
                    i2pr-sam M017 -> M018            M006 live qualification
                                                     also requires upstream i2pr:
                                                     - SAM/368 port-aware SAM 3.3
                                                     - managed-app/386 Linux Secured

Dependency classification:

- M002 hard-depends on M001.
- M003 hard-depends on M001/M002.
- M004 hard-depends on M001/M002.
- M005 hard-depends on M003/M004.
- M007, M008, and M009 are closed. Hosted verification is green on run 37376165523; runs 37372987942 and 37373787767 were cancelled with no runner during a GitHub Actions outage and executed zero steps.
- M010 hard-depends on M009 closure and on ADR-0002. It depends on no upstream milestone: the SAM 3.1 client codec is the client half of the adapter, and upstream `specs/references/portable-service-tunnel-sam-adapter-handoff.md` assigns SAM client implementation to a separate repository that i2pr will never ship. M010 is closed with no remaining local work.
- M011 hard-depends on all completed local foundation work through M010 and on an exact-head green integration candidate. It owned only history-preserving mainline integration, qualification, planning reconciliation, and work-branch cleanup, and is closed. It required no production change.
- M012 hard-depends locally on M010/M011 closure and on the now-concrete managed-app v1 application wire contract. Those dependencies are satisfied. It implements the independent application-role managed-app client/multiplexer only; it requires neither a live router, SAM implementation, nor sandbox and is ready.
- M013 hard-depends on M012 closure and on `dbowm91/i2pr-sam` M018 closure/mainline integration. M018 is registered but blocked on i2pr-sam M017's foundation reconciliation/merge. M013 then adopts the canonical SAM client, supplies each SAM connection through M012, composes synchronous MailTransport, and retires `i2pr-mail-sam`. It requires no live i2pr or Secured backend and remains blocked only on those two implementation dependencies.
- M006 hard-depends locally on M013 closure. Its former upstream process/channel dependency is satisfied by i2pr Plans 368–371 and 382–383. Two upstream capability dependencies remain: i2pr SAM/368 must close port-aware SAM 3.3 support, and Managed native app runtime/386 must close the first qualified `Secured` backend.
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

Blocker: M013 closure plus two upstream capabilities. The old app-runtime reachability blocker is closed by i2pr Plans 368–371 and 382–383. M013 owns the completed local production adapter stack using canonical i2pr-sam. Final mail qualification still requires upstream i2pr SAM/368 to provide nonzero I2P `TO_PORT` semantics and upstream Managed native app runtime/386 to provide a qualified `Secured` launch profile. Current `UnsafeDirect` operation is not an acceptable substitute.

Exit: real adapter smoke/qualification plus negative evidence that no direct/loopback fallback is required.

### M010 — SAM 3.1 client codec and downstream contract qualification

Class: capability/infrastructure.

Objective: land the client half of the i2pr adapter as a deterministic sans-I/O codec, and record the M006 §7A interface and authority matrix.

This milestone exists because upstream will not supply it: `specs/references/portable-service-tunnel-sam-adapter-handoff.md` assigns SAM client codecs and state machines to a separate repository. It depends on no upstream milestone and does not unblock M006.

Exit: `i2pr-mail-sam` compiles with zero dependencies, encodes and parses exactly the outbound mail path, rejects injection and malformed input fail-closed, and the M006 contract matrix cites the closed upstream contract.

### M011 — Foundation branch integration, merge, and cleanup

Class: integration + planning/branch hygiene.

Objective: move the completed M001-M005/M007-M010 foundation from `codex/foundation-planning` to `main` by a history-preserving exact-head fast-forward, prove mainline CI, reconcile planning, and retire the work branch only after durable closure evidence exists on main.

Exit: all evidence-bearing commits remain reachable at their original SHAs from `main`; exact integration and final closure heads have green hosted CI; M006 is the only remaining active capability plan and remains blocked on the upstream app-side runtime; no unique work remains solely on the feature branch.

Result: closed. `main` moved `9f2cb7c7` → `64cd8121` by a single non-forced fast-forward with no rewrite of any cited commit; candidate CI `37491157600` and integration-main CI `37491896332` both green; the work branch was deleted after the closure record landed. See `plans/closure/mail-backend-foundation/011-status.md`.

### M012 — Managed-app v1 application client and multiplexer

Class: corrective integration infrastructure + invariant.

Objective: implement the independent application side of i2pr managed-app v1 over injected async byte I/O: trusted launch-context parsing, handshake/hello/effective-capability establishment, bounded frame/control codecs, request/stream correlation, and isolated logical service streams.

Why ready: the local foundation through M011 is closed and upstream Plans 368–371/382–383 provide a concrete application-facing wire contract. M012 uses deterministic fake-host evidence and requires neither a live router, SAM implementation, nor qualified OS sandbox.

Exit: a bounded `ManagedAppClient` can open authorized logical `sam` streams with no direct network authority; M013 becomes the local integration successor.

### M013 — Canonical SAM adoption and MailTransport composition

Class: integration infrastructure + corrective consolidation.

Objective: consume the closed `dbowm91/i2pr-sam` M018 injected-connection provider through M012 logical `sam` streams, compose the existing synchronous MailTransport/ByteStream seam, map POP3/SMTP to typed I2P destination ports 110/25, and retire the temporary `i2pr-mail-sam` implementation after parity evidence.

Blocker: M012 closure + i2pr-sam M018 closure/mainline integration. M018 is itself registered behind i2pr-sam M017.

Exit: all deterministic production adapter behavior is local and closed, one canonical SAM implementation remains, and M006's only blockers are upstream i2pr SAM/368 and Managed native app runtime/386.

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

This roadmap closes only when M001-M006, the post-M005 M007-M009 corrective sequence, and integration milestones M012-M013 have evidence-based closure, the backend is independently functional, and i2pr integration preserves the no-direct-network authority boundary. M010/M011 remain historical prerequisites already closed. GUI completion is not required.

## 12. Milestone status

| Milestone | Status | Implementation plan | Closure record | Blockers |
|---|---|---|---|---|
| M001 | closed | plans/implementation/mail-backend-foundation/001-workspace-domain-and-boundary-foundation.md | plans/closure/mail-backend-foundation/001-status.md | none |
| M002 | closed | plans/implementation/mail-backend-foundation/002-mime-and-durable-storage.md | plans/closure/mail-backend-foundation/002-status.md | none |
| M003 | closed | plans/implementation/mail-backend-foundation/003-pop3-receive-sync-and-reconciliation.md | plans/closure/mail-backend-foundation/003-status.md | none |
| M004 | closed | plans/implementation/mail-backend-foundation/004-smtp-compose-outbox-and-submission.md | plans/closure/mail-backend-foundation/004-status.md | none |
| M005 | closed | plans/implementation/mail-backend-foundation/005-backend-service-convergence.md | plans/closure/mail-backend-foundation/005-status.md | none |
| M006 | blocked | plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md | — | M013 closure + upstream i2pr SAM/368 port-aware SAM 3.3 + upstream managed-app/386 qualified Linux Secured backend |
| M007 | closed | plans/implementation/mail-backend-foundation/007-request-state-protocol-corrective.md | plans/closure/mail-backend-foundation/007-status.md | none |
| M008 | closed | plans/implementation/mail-backend-foundation/008-runtime-decomposition-and-transport-seam.md | plans/closure/mail-backend-foundation/008-status.md | none |
| M009 | closed | plans/implementation/mail-backend-foundation/009-hosted-verification-and-corrective-closure.md | plans/closure/mail-backend-foundation/009-status.md | none |
| M010 | closed | plans/implementation/mail-backend-foundation/010-sam31-client-codec-and-contract-qualification.md | plans/closure/mail-backend-foundation/010-status.md | none |
| M011 | closed | plans/implementation/mail-backend-foundation/011-foundation-branch-integration-merge-and-cleanup.md | plans/closure/mail-backend-foundation/011-status.md | none |
| M012 | ready | plans/implementation/mail-backend-foundation/012-managed-app-v1-client-and-multiplexer.md | — | none |
| M013 | blocked | plans/implementation/mail-backend-foundation/013-canonical-sam-adoption-and-mailtransport-composition.md | — | M012 closure + dbowm91/i2pr-sam M018 closure/mainline integration |
