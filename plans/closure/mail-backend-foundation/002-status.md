# M002 Closure — MIME and Durable Storage

Status: closed

## Implementation evidence

- MIME parser/builder choices are pinned through `Cargo.lock` and hidden behind the project facade. `mail-parser 0.11.9` and `mail-builder 1.0.0` are MIT OR Apache-2.0 and forbid unsafe code internally; mail-builder's hostname feature is disabled. `rusqlite 0.40.2` is MIT licensed with bundled SQLite; `sha2 0.10.9` provides content identity.
- Parser retains a separate exact `RawEntity`; bounded text parsing and text compose are exposed without third-party types.
- SQLite schema v1 covers raw metadata, messages/UIDL mapping, drafts, and outbox. Raw entities are SHA-256 named and atomically synced/renamed before metadata publication.
- Store APIs cover message lookups, drafts, outbox claim/stage/recovery, per-account UIDL uniqueness, and raw entity retrieval.
- Recovery deletes temporary files only; immutable content-addressed files are verified on read and are never silently overwritten.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| raw entity exact retention and restart | store reopen/read round trip and MIME raw retention tests |
| MIME parser and attachment metadata | plain text and multipart attachment fixtures |
| privacy-safe compose subset | explicit Message-ID/timestamp; no Bcc/User-Agent/X-Mailer/localhost; header injection rejected |
| schema create/reopen and version | store reopen test checks schema version 1; idempotent schema initialization |
| UIDL uniqueness | duplicate rejected within account; same UIDL allowed across accounts |
| atomic blob handling and corruption | synced temp + rename + directory sync; SHA-256/size verified on read; tamper test detects corruption |
| temporary recovery | startup fixture removes `.tmp` and preserves unrelated immutable file |
| transactional/outbox restart | unique UIDL conflict leaves one published message; outbox claim is compare-and-set; stale stage 1 recovers to DeliveryUnknown |
| secret persistence boundary | no credentials accepted by store APIs; synthetic persistence-file sentinel scan |

## Verification

Run on 2026-10-05 with Rust 1.98.1:

- `cargo fmt --all -- --check` — passed via `bash scripts/verify.sh quick`.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-mail-mime` — passed, 5 tests.
- `cargo test --locked -p i2pr-mail-store` — passed, 6 tests.
- `cargo test --locked --workspace --all-targets` — passed, 24 tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed.
- `bash scripts/verify.sh quick` — passed.

## Failure, restart, migration, and security review

Filesystem publication precedes metadata. A failure between these operations may leave an orphan content file; it cannot create metadata referring to missing content. Failed metadata insertion does not publish partial state. Content corruption is surfaced as `CorruptBlob`. Restart maps stale outbox stage 0 to retry-safe and stage 1+ to `DeliveryUnknown`. The first schema version is idempotently initialized; no released schema exists to migrate. The store is synchronous with one explicit mutable owner; async executor integration remains runtime-owned.

No medium/high unresolved findings at M002 scope. Compose supports text and parsing reports attachment count; rich outbound attachment composition is reserved to M004.

## Unblock audit

M003 is active and M004 is ready because both hard dependencies M001/M002 are closed. M005 remains blocked on M003 and M004 closure. M006 remains blocked on M005 and the stable authorized i2pr gateway.

## Implementation commit

Recorded in Git history with this closure update.
