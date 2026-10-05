# Mail Backend Foundation Milestone 002 — MIME and Durable Storage

Status: blocked

Repository baseline: planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; execute only after M001 closure

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: infrastructure

## 1. Objective

Implement a pure MIME facade plus restart-safe local persistence: SQLite metadata/state and immutable raw RFC message/blob storage. Establish the durable substrate required by both receive and send paths without opening any network connection.

## 2. Why this milestone is ready

Blocked until M001 closes. Once M001 closes, the domain model and dependency graph are stable enough to define schema/storage against them.

## 3. Current implementation evidence

At planning time no schema, migrations, blob layout, MIME dependency, or persistence API exists.

Research identified mail-parser and mail-builder as plausible Rust candidates, but this plan does not pre-authorize them. Implementation must review current versions, licenses, dependency graphs, unsafe usage, maintenance posture, and exact behavior before adoption.

## 4. Invariants that must not regress

- raw received/composed RFC entities can be retained exactly;
- metadata state is transactional;
- large content does not need to live as SQLite BLOBs;
- blob paths derive only from local opaque BlobId, never remote filename/UIDL/header content;
- credentials/auth payloads never enter SQLite/blob storage/logs;
- mail-mime remains pure and does not own sockets/filesystem/tasks;
- MIME parsing/building is bounded against hostile size/nesting/header input.

## 5. Scope

### In scope

- select and pin MIME parser/builder dependencies or implement a bounded facade if review rejects candidates;
- i2pr-mail-mime pure parse/build API;
- SQLite metadata schema and migrations;
- filesystem immutable raw-entity/blob layout;
- atomic temp-write/commit behavior;
- message summary, raw entity, UIDL mapping, local roles/status, draft, outbox, and transition persistence needed by M003/M004;
- corruption/restart/migration/security tests.

### Explicitly out of scope

- POP3/SMTP networking or protocol sessions;
- HTML rendering;
- remote-resource fetching;
- credential persistence;
- attachment metadata stripping;
- full-text search;
- frontend database access.

## 6. Required production changes

### MIME

Expose bounded pure functions/types for:

- parse raw message into header/body/part metadata plus references into raw data;
- obtain safe text/plain alternative when present;
- enumerate attachments with sanitized presentation filename separate from raw header;
- build an outbound RFC entity from domain draft/envelope inputs;
- explicitly supplied Message-ID and Date policy inputs rather than ambient hostname/timezone.

Do not permanently store sanitizer output as canonical message content.

### Storage

Use SQLite for transactional metadata and a private application data directory for immutable blobs/raw .eml entities.

Schema must include stable ids and enough state for:

- accounts/service profile;
- messages and summaries;
- (AccountId, Uidl) remote mapping;
- blob references and sizes/hashes where justified;
- local mailbox/read/deletion state;
- drafts;
- outbox state including DeliveryUnknown;
- schema version.

### Commit protocol

A raw entity write must not leave a database row claiming durable content that was never atomically committed. Define ordering, temp-file naming, fsync/rename expectations appropriate to supported platforms, and cleanup/recovery of orphan temp files.

## 7. Ordered work packages

### A — Dependency and MIME contract review

Evaluate current candidate crates and freeze the narrow facade before exposing third-party types.

Acceptance evidence: dependency review recorded in docs/architecture/mime.md and focused parser/builder fixtures.

### B — Schema and migrations

Implement initial migration and store interface.

Acceptance evidence: new database, reopen, migration idempotence/version checks, constraint tests.

### C — Blob/raw entity store

Implement local opaque paths, atomic write, integrity metadata, and cleanup.

Acceptance evidence: partial-write/crash simulation fixtures and path traversal negatives.

### D — Transactional domain storage

Persist message/draft/outbox/UIDL states without protocol knowledge.

Acceptance evidence: restart round trips and state-transition tests.

## 8. Failure, cancellation, restart, and contention semantics

Interrupted blob write leaves at most an orphan temp file, never a committed metadata reference to missing content.

Database transaction failure does not publish partially advanced receive/send state.

On restart, cleanup/recovery is deterministic and does not delete a committed blob referenced by the database.

Store access must have one documented SQLite concurrency model; do not mix ad hoc blocking database calls across async tasks.

## 9. Compatibility and migration

This is schema version 1. Migration machinery must exist from the first schema even though no older released schema exists. Once a release ships, historical migrations become immutable except factual bug correction with dedicated tests.

## 10. Required tests

### MIME

- representative plain text, multipart alternative, attachment, encoded-header fixtures;
- malformed MIME/header bounds;
- exact raw entity retention;
- build/parse round trip for supported compose subset;
- generated output contains no implicit hostname/User-Agent/X-Mailer.

### Store

- schema create/reopen;
- all domain state round trips;
- unique (AccountId, Uidl) mapping;
- atomic raw entity commit;
- missing/corrupt blob handling;
- rollback on write/database failure;
- orphan temp cleanup;
- path traversal/hostile filename rejection;
- credential sentinel never appears in database/blob files.

### Restart/contention

- restart during pending draft/outbox states;
- concurrent readers with serialized mutations according to chosen store owner.

## 11. Required verification commands

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-mime
cargo test --locked -p i2pr-mail-store
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

## 12. Documentation updates

- docs/architecture/mime.md;
- docs/architecture/storage.md;
- README storage/privacy notes where useful;
- roadmap/registry status;
- closure record 002-status.md.

## 13. Acceptance criteria

- supported MIME subset parses/builds behind a project-owned pure facade;
- raw entities survive restart byte-for-byte;
- metadata and blob commit semantics are crash/restart safe under tested faults;
- credentials are absent from durable mailbox state;
- M003 and M004 can rely on the store without knowing SQLite paths/schema details.

## 14. Stop conditions

Stop if:

- MIME dependency introduces unacceptable license/security/unsafe/dependency risk;
- exact raw retention conflicts with chosen parser API;
- platform atomicity requires a durable architectural decision not covered here;
- async SQLite ownership cannot be made explicit without changing roadmap architecture;
- scope expands into search, renderer, network protocol, or credential vault.

## 15. Closure evidence required

Dependency review, schema listing, migration/restart matrix, raw-entity integrity tests, fault-injection results, secret-sentinel evidence, exact verification commands/results, security review, and unblock audit for M003/M004.

## 16. Handoff notes

Prefer a narrow facade over exposing third-party MIME crate types. Preserve raw bytes even when parsed metadata is lossy or malformed.