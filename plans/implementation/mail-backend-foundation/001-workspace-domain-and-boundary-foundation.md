# Mail Backend Foundation Milestone 001 — Workspace, Domain, and Boundary Foundation

Status: ready for handoff

Repository baseline: ec8056a75ccba628994aa7610ac5a07cd3d4a986 on codex/foundation-planning

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: invariant + infrastructure

## 1. Objective

Create the initial Rust workspace and freeze the backend ownership boundaries, domain/state vocabulary, bounded identifiers, and verification guards required by all later mail work. This milestone must establish structure and invariants without claiming working mail receipt or submission.

## 2. Why this milestone is ready

The repository is new and has no production compatibility or migration burden. Canonical product/security direction and ADR-0001 are accepted. No external runtime dependency is required.

## 3. Current implementation evidence

Only repository/planning documentation exists at the baseline. There is no Cargo workspace, schema, protocol implementation, runtime, executable, CI contract, or released API.

The active i2pr native-app API remains an external future interface dependency and is intentionally irrelevant to this milestone.

## 4. Invariants that must not regress

- mail-domain owns identities and mail state, not network/storage implementations;
- lower pure crates do not open sockets, resolve names, access filesystem state, spawn tasks, or depend on GUI frameworks;
- UIDL and local MessageId remain distinct;
- POP3 ordinal is not durable identity;
- DeliveryUnknown is distinct from retry-safe SMTP failure;
- credentials/auth payloads are not general domain/config values suitable for serialization;
- no direct clearnet/loopback fallback is introduced;
- workspace unsafe code is denied by default.

## 5. Scope

### In scope

- root Cargo workspace, rust-toolchain/MSRV documentation if needed, formatting/lint policy;
- initial crates: i2pr-mail-domain, i2pr-mail-mime, i2pr-mail-proto, i2pr-mail-store, i2pr-mail-runtime;
- dependency direction between those crates;
- domain identity/state types needed by M002-M005;
- bounded string/newtype validation for account/mail identifiers and service configuration;
- initial Postman service profile values as data, not socket behavior;
- static dependency/boundary checks;
- repository verification script;
- architecture overview and domain docs.

### Explicitly out of scope

- SQLite schema;
- MIME dependency adoption;
- POP3 or SMTP command engines;
- async transport runtime;
- real network access;
- i2pr dependency;
- GUI/frontend;
- persisted credentials;
- released public API stability.

## 6. Required production changes

### Core/domain

Create i2pr-mail-domain with at least:

- AccountId, MessageId, DraftId, OutboxId, BlobId;
- Uidl as an opaque bounded validated value;
- MailService = Pop3 | Smtp;
- local mailbox role/state vocabulary;
- receive-cache/deletion state sufficient to represent RemoteKnown, HeaderCached, BodyCached, DeletePending, DeleteMarkedSession, RemoteDeletionCommitted;
- submission state sufficient to represent Draft, Queued, Submitting, FailedSafeToRetry, DeliveryUnknown, Sent;
- MessageIdHeader wrapper distinct from local MessageId;
- service profile containing logical I2P destination name and port without host-socket semantics.

Use enums/newtypes instead of state booleans when the boolean would permit illegal combinations.

### Workspace ownership

The expected dependency direction is:

- mail-domain has no project crate dependency;
- mail-mime may depend on mail-domain;
- mail-proto may depend on mail-domain;
- mail-store may depend on mail-domain;
- mail-runtime may depend on domain/mime/proto/store;
- no inverse dependency from pure/storage crates to runtime.

Create empty/minimal crate surfaces only where needed to enforce the graph; do not invent future APIs merely to fill files.

### Security and bounds

Freeze conservative identifier/diagnostic/config ceilings and max+1 tests. No untrusted string is allowed to drive a path directly.

### Documentation and static guards

Add docs/architecture/overview.md and docs/architecture/domain.md.

Add scripts/check-boundaries.sh that fails if pure crates gain obvious network/process/task/GUI dependencies or dependency direction reverses. Keep the guard narrow and maintainable; dependency inspection plus targeted source checks are sufficient.

Add scripts/verify.sh with a quick path covering format, check, focused guards, and unit tests.

## 7. Ordered work packages

### A — Workspace and toolchain

Create manifests, shared package metadata, lints, minimal crate roots, and locked dependency policy.

Acceptance evidence: cargo metadata/check resolves the full workspace and no unintended external dependency is present.

### B — Domain freeze

Implement identity and state vocabulary with validation and serde only where persistence/API value semantics justify it.

Acceptance evidence: round-trip, invalid/max+1, identity-separation, and state-construction tests.

### C — Dependency boundaries

Wire only the allowed crate edges and implement the static guard.

Acceptance evidence: positive-control guard test or documented proof that an intentionally forbidden dependency/pattern is detected.

### D — Verification/docs

Create architecture docs and repository verification commands.

Acceptance evidence: quick verification passes from a clean checkout.

## 8. Failure, cancellation, restart, and contention semantics

This milestone owns no runtime tasks or persistence. It must nonetheless represent later restart-critical states without collapsing them. Invalid domain input returns typed validation errors and never panics on hostile bytes.

## 9. Compatibility and migration

No released compatibility exists. Serde forms are pre-release and may still change, but the closure must record any persisted/public forms introduced so M002 can treat them intentionally.

MSRV should initially match compatible i2pr expectations: Rust 1.88+ / edition 2024 at this baseline unless repository evidence requires a higher floor.

## 10. Required tests

### Focused unit tests

- each identifier valid/min/max/max+1;
- Uidl opacity and invalid input;
- service profile validation;
- receive and submission state serde/equality semantics where applicable;
- local MessageId cannot be confused with MessageIdHeader/Uidl at type level.

### Security and negative tests

- no path-like interpretation of remote identifiers;
- no credential-bearing serializable domain type;
- boundary guard positive control.

### Integration tests

- cargo workspace dependency graph matches the documented direction.

## 11. Required verification commands

Run at minimum:

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

Record exact outcomes in closure; do not claim unrun commands.

## 12. Documentation updates

- README.md if build/development commands become available;
- docs/architecture/overview.md;
- docs/architecture/domain.md;
- plans/subsystems/mail-backend-foundation-roadmap.md milestone status;
- plans/registry.md status transitions;
- plans/closure/mail-backend-foundation/001-status.md at closure.

## 13. Acceptance criteria

- workspace builds on the documented MSRV/toolchain policy;
- stable crate ownership graph exists and is statically checked;
- required domain identities/states are implemented and bounded;
- no socket/database/runtime/frontend capability is falsely claimed;
- boundary and unit tests pass;
- M002 has the stable domain and ownership contract it needs.

## 14. Stop conditions

Stop and report rather than improvise if:

- a required domain decision changes ADR-0001 ownership;
- a dependency would force network/filesystem/task ownership into mail-domain;
- a credential design requires persistence semantics;
- repository tooling cannot enforce the claimed dependency boundary;
- scope expands into POP3, SMTP, storage schema, or GUI implementation.

## 15. Closure evidence required

- implementation commit list;
- final crate dependency graph;
- identifier/state requirement-to-test matrix;
- boundary guard positive-control evidence;
- exact verification results;
- dependency review;
- security review confirming no secret/network/runtime owner landed;
- unblock audit for M002.

## 16. Handoff notes

Prefer small types and explicit state transitions over an abstract generic mail-provider model. Do not add IMAP/provider/OAuth vocabulary to make the model look extensible.