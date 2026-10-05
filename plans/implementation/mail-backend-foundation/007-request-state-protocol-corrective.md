# Mail Backend Foundation Milestone 007 — Request, State, and Protocol Corrective

Status: ready for handoff

Repository baseline: `e1db79b55e3fdd128b9dafe701b50c5dbf5db9c5` on `codex/foundation-planning`

Source roadmap:

- `plans/subsystems/mail-backend-post-m005-corrective-addendum.md`

Predecessor closures:

- M001-M005 are closed under `plans/closure/mail-backend-foundation/`.

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`

Primary class: invariant + corrective capability correctness

## 1. Objective

Close the concrete post-M005 correctness findings without reopening unrelated mail scope:

1. make request-ID deduplication bounded over time rather than permanently exhausting a long-lived backend;
2. carry receive/submission state as project-owned typed values across domain, store, and runtime boundaries;
3. make invalid persisted state fail closed and migrate the existing schema safely;
4. tighten POP3/SMTP status/auth/command bounds so malformed or oversized inputs are rejected before protocol allocation/writes.

## 2. Why this milestone is ready

All required production owners already exist and M001-M005 are closed. No i2pr interface or network access is required. The findings are local and reproducible.

## 3. Current implementation evidence

At the baseline:

- BackendService retains every accepted request ID until shutdown and permanently reaches MAX_SESSION_REQUESTS;
- ReceiveState and SubmissionState exist in mail-domain, but mail-store/mail-runtime expose state as arbitrary String/&str values;
- outbox state plus integer stage admits combinations not represented by the domain model;
- POP3 parse_status accepts the first three bytes of -ERR rather than validating the complete status atom;
- auth/envelope inputs lack one explicit pre-allocation protocol-line ceiling.

## 4. Invariants that must not regress

- duplicate request IDs inside the documented deduplication window fail deterministically;
- request retention is bounded and cannot permanently exhaust a healthy service;
- terminal completion/error releases active request ownership;
- store/runtime mutation APIs do not accept arbitrary strings for receive/submission states;
- unknown/corrupt persisted enum values fail closed;
- DeliveryUnknown remains non-retryable until explicit resolution;
- DeleteMarkedSession remains distinct from confirmed remote deletion;
- schema v1/v2/v3 fixtures continue to migrate;
- POP3/SMTP limits are enforced before allocation/write;
- secrets are absent from returned errors/debug output.

## 5. Scope

### In scope

- bounded active/recent request-ID lifecycle;
- typed receive/submission persistence mappings;
- typed submission progress/stage representation if still needed;
- schema migration/check constraints for state validity;
- strict persisted-state decoding;
- POP3 status-token correction;
- explicit POP3 USER/PASS and SMTP AUTH/envelope command-size ceilings;
- base64 expansion bounds;
- regression tests/documentation.

### Explicitly out of scope

Runtime decomposition, transport sync/async redesign, i2pr integration, database replacement, MIME expansion, background scheduling, and GUI work.

## 6. Required production changes

### A. Request-ID lifecycle

Replace the monotonically growing seen-request set with active ownership plus a bounded recent-completion window.

Required semantics:

- active duplicate is rejected;
- recent completed duplicate is rejected;
- terminal success/error removes active ownership;
- completed retention has deterministic bounded eviction;
- an evicted old ID may be reused because no durable cross-session idempotency contract is claimed;
- exceeding completion-history capacity evicts old entries instead of returning permanent Capacity;
- only genuinely live resource ceilings may produce Capacity.

Do not persist request IDs across restart.

### B. Typed state boundary

Use i2pr-mail-domain ReceiveState and SubmissionState at Rust API boundaries.

If SMTP ambiguity still requires a separate progress marker, define a bounded project-owned enum/type. Storage owns canonical SQL encoding/decoding. Ordinary callers must not pass state string literals after this corrective.

### C. Schema migration and constraints

Advance the schema version while preserving tested migration from versions 1, 2, and 3.

Use constraints or equivalent migration validation so unsupported persisted states/combinations fail closed. Do not rewrite raw message blobs.

### D. Protocol correctness and bounds

POP3 must recognize exactly +OK or -ERR status atoms with valid termination/text delimiters and reject truncated forms such as -ER.

Freeze auth/envelope ceilings consistent with protocol line limits. Reject over-bound values before formatting command buffers or base64 output. Generated MAIL FROM and RCPT TO commands must fit the same line budget.

## 7. Ordered work packages

### WP1 — Typed durable state

Introduce typed store/runtime APIs and schema migration first.

Acceptance evidence: migration fixtures, invalid stored-state negatives, and removal of ordinary string-state mutation call sites.

### WP2 — Request lifecycle

Implement bounded active/recent bookkeeping with terminal release on every path.

Acceptance evidence: process substantially more than the retention capacity without permanent failure; active/recent duplicate rejection; deterministic eviction/reuse.

### WP3 — Protocol bounds

Fix POP3 status parsing and auth/envelope limits.

Acceptance evidence: malformed-prefix and max/max+1 fixtures.

### WP4 — Documentation/guards

Update architecture docs and cheap static checks where useful.

## 8. Failure, cancellation, restart, and contention semantics

A request ID enters active ownership before operation execution and leaves it on every terminal path, including credential failure, transport failure, cancellation, timeout, store error, and validation error.

The recent dedupe window is process-local and may reset on restart.

Database migration is transactional. Unknown stored state is a corruption/unsupported-state error, never an automatic send/delete/retry decision.

## 9. Compatibility and migration

This is pre-release, but v1-v3 migration fixtures are retained as compatibility evidence. Rust APIs may intentionally change from strings to enums.

## 10. Required tests

Request lifecycle:

- duplicate active;
- duplicate recent;
- more than 2x completion-retention capacity of sequential successful requests;
- evicted ID reuse;
- failures/cancellations do not leak active slots.

State/storage:

- v1/v2/v3 -> current migration;
- current reopen;
- all valid states round-trip;
- unknown persisted state fails closed;
- impossible submission state/progress rejected;
- DeliveryUnknown cannot be claimed without explicit resolution;
- POP3 delete states retain prior behavior.

Protocol/security:

- +OK / -ERR and optional text;
- reject -ER and malformed delimiters;
- USER/PASS max and max+1;
- SMTP auth expansion max and max+1;
- MAIL FROM/RCPT max and max+1;
- secret sentinel absent from errors/diagnostics.

## 11. Required verification commands

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-domain
cargo test --locked -p i2pr-mail-proto
cargo test --locked -p i2pr-mail-store
cargo test --locked -p i2pr-mail-runtime
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

## 12. Documentation updates

Update domain, storage, backend API, POP3, SMTP, corrective addendum, registry, and closure documentation.

## 13. Acceptance criteria

- sustained operations beyond request-history capacity do not require restart;
- duplicates remain rejected within the bounded window;
- normal Rust APIs cannot write arbitrary mail state strings/stages;
- legacy schemas migrate and invalid state fails closed;
- malformed POP3 status is rejected;
- oversized auth/envelope inputs fail before unbounded formatting/base64/writes;
- M003-M005 end-to-end behavior remains green.

## 14. Stop conditions

Stop if typed state requires semantic protocol changes, migration cannot preserve v1-v3 data, deduplication requires durable idempotency, auth interoperability requires widening mechanisms, or scope expands into runtime decomposition/i2pr integration.

## 15. Closure evidence required

State/migration matrix, request retention trajectory, proof terminal paths release active ownership, protocol max+1 matrix, exact verification results, secret/state security review, and M008 unblock audit.

## 16. Handoff notes

Do not solve request exhaustion by merely increasing MAX_SESSION_REQUESTS. Do not solve typed-state defects with more scattered string validation.
