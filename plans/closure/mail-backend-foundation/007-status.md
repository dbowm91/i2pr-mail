# Mail Backend Foundation M007 Closure — Request, State, and Protocol Corrective

Status: closed

Closed: 2026-10-05

Baseline: `e1db79b55e3fdd128b9dafe701b50c5dbf5db9c5` on `codex/foundation-planning`

Source plan: `plans/implementation/mail-backend-foundation/007-request-state-protocol-corrective.md`

## Outcome

The three post-M005 correctness classes are closed. The request ledger is bounded over time instead of exhaustible, durable state is carried as project-owned typed values from the domain crate through the store and runtime APIs with schema-enforced constraints and fail-closed decoding, and POP3/SMTP parsing and command construction now reject malformed or over-bound input before any allocation or write.

## Closure evidence

### State and migration matrix

| Requirement | Evidence |
|---|---|
| Typed state at Rust API boundaries | `ReceiveState`, `SubmissionState`, `SubmissionProgress`, `StoredMessageState` in `crates/mail-domain/src/lib.rs` own the canonical SQL encoding (`as_storage_str`, `as_storage_i64`) and strict decoding (`from_storage_str`, `from_storage_i64`). `Store::{save_message,set_receive_state,set_outbox_stage}` and the runtime results take these values; no public signature accepts a state string or integer stage. |
| Bounded ambiguity axis | `SubmissionProgress::{NotStarted,DataMayHaveStarted,DeliveryAccepted}` is separate from submission state, and `SubmissionState::allowed_progress` is the single definition of representable pairs. Test `submission_state_progress_combinations_are_representable_only`. |
| Migration from v1, v2, v3 | `Store::open` creates a new store at version 4 and migrates older databases in place. Test `every_legacy_schema_version_migrates_to_the_current_schema` seeds a legacy v1, v2, and v3 database with rows, migrates, and asserts the decoded state, retained recipients/sender, current version on reopen, and that the migrated database now rejects an unknown receive state and an unrepresentable state/progress pair. |
| Constraints generated from the vocabulary | `receive_state_constraint()` and `submission_state_constraint()` in `crates/mail-store/src/lib.rs` build the `CHECK` clauses from `ReceiveState::ALL` / `SubmissionState::ALL` and `allowed_progress()`, so SQL cannot drift from Rust. |
| Invalid persisted state fails closed | Migration decodes every existing value before rebuilding and aborts with `StoreError::UnknownState`. Tests `unsupported_persisted_state_fails_closed_instead_of_being_repaired` (unknown token, wrong case, `Sent` in `messages`, out-of-range stage) and `a_newer_schema_is_rejected_without_modification`. The rebuild suspends foreign keys only for the copy and then verifies `pragma_foreign_key_check`. |
| Impossible combinations rejected | `set_outbox_stage` returns `StoreError::InvalidStateCombination` before any write. Test `impossible_submission_state_progress_pairs_are_rejected`. |
| All valid states round-trip | Test `every_valid_state_round_trips_through_the_typed_api` round-trips every `ReceiveState`, exercises the deletion transition chain, and confirms `Sent` appears only as a listing projection with no UIDL. |
| Raw blobs untouched | Migration rebuilds only `messages` and `outbox` metadata tables; `raw_entities` and blob files are not rewritten. Covered by the migration test plus the existing reopen/corruption test. |

### Request retention trajectory

| Requirement | Evidence |
|---|---|
| Bounded, not permanent, dedupe | `RequestLedger` in `crates/mail-runtime/src/lib.rs` holds active ownership plus a `MAX_RECENT_COMPLETED_REQUESTS` (1024) FIFO completion window with oldest-first eviction. `MAX_SESSION_REQUESTS` is removed. |
| Sustained use beyond capacity | Test `sustained_backend_use_recovers_request_capacity_without_restart` runs 2065 sequential successful requests through one `BackendService`, more than 2x the retention window, and every request succeeds without a restart. |
| Duplicate rejection inside the window | Same test asserts a recent request ID is still rejected with `InvalidRequest`; test `request_ledger_rejects_duplicates_and_bounds_only_live_capacity` covers duplicate-active, duplicate-recent, and deterministic eviction. |
| Evicted IDs reusable | Same tests assert `req-0` succeeds after eviction, because no durable cross-session idempotency contract is claimed. |
| Only live ceilings report Capacity | `RequestLedger::begin` returns `LedgerError::Capacity` only when `active.len() >= MAX_ACTIVE_REQUESTS`; completing beyond the retention window evicts instead. Covered by both ledger tests and by `BackendHealth.retained_request_ids` remaining at 1024 after 2065 requests. |

### Terminal path release proof

`BackendService::terminal` is the single terminal path and calls `requests.end` for every operation, including the multi-event sync and body-fetch paths. `BackendHealth` exposes `active_requests` and `retained_request_ids` so the property is observable rather than assumed.

Test `failing_and_cancelled_requests_release_active_ownership` drives more than 2x `MAX_ACTIVE_REQUESTS` failing operations (unknown message), plus credential failure, pre-cancelled sync, and an unknown outbox row, then asserts `active_requests == 1` (the probe itself) and bounded retained history. Without release on every terminal path this test would report `Capacity` partway through.

### Protocol max+1 matrix

| Input | Frozen ceiling | Behavior |
|---|---|---|
| POP3 status atom | exact `+OK`/`-ERR` + CRLF + optional SP text | `-ER`, `-ERRX`, `-ERRS`, `+OKX`, `+Ok`, `-\r\n`, bare LF rejected. Test `status_atoms_are_recognized_exactly`, `truncated_or_glued_status_forms_are_rejected`. |
| POP3 `USER` | `MAX_USER_LEN` 40 | max accepted, max+1 rejected before formatting. Tests `credential_ceilings_are_enforced_before_formatting`, `pop3_credential_ceilings_reject_before_any_command_is_written` (asserts no `USER`/`PASS` bytes reach the stream). |
| POP3 `PASS` | `MAX_PASS_LEN` 40 | as above; rejection carries no credential bytes. |
| SMTP credential | `MAX_CREDENTIAL_LEN` 768, expanded line `<= MAX_AUTH_LINE` 1024 | max accepted, max+1 rejected before base64 encoding runs. Tests `credential_ceilings_are_enforced_before_base64_expansion`, `smtp_credential_and_envelope_ceilings_reject_before_queue_or_io`. |
| SMTP `MAIL FROM` | `MAX_ENVELOPE_LINE` 512 incl. CRLF | exact-fit address accepted, one byte more rejected; self-bracketed address rejected. Test `envelope_lines_fit_the_frozen_budget`. |
| SMTP `RCPT TO` | same | same. |

SMTP bounds are checked before the outbox row is queued and before a stream opens, so an over-bound envelope has neither durable nor network effect; the test asserts `outbox_state` is `None` for each rejected submission and that nothing reached the stream.

### Security review

- Credentials remain ephemeral. `pop3.rs` and `smtp.rs` error types carry no payload, so a rejection cannot echo a secret; the runtime bounds tests assert no credential substring reaches the written command buffer.
- The typed-state guard in `scripts/check-boundaries.sh` prevents reintroducing string state, and the existing store test continues to assert no credential bytes reach any store file or blob.
- No new dependency was added. The lower crates remain sans-I/O and Tokio-free; the boundary guard passes unchanged for that rule.
- No local hostname, username, timezone, or version is emitted; no header generation changed.

### Verification

Commands run from the repository root, all passing:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-mail-domain` — passed, 6 tests.
- `cargo test --locked -p i2pr-mail-proto` — passed, 11 tests.
- `cargo test --locked -p i2pr-mail-store` — passed, 13 tests.
- `cargo test --locked -p i2pr-mail-runtime` — passed, 19 tests.
- `cargo test --locked --workspace --all-targets` — passed, 57 tests total.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed, including the dependency positive control and the typed durable-state positive control.
- `bash scripts/verify.sh quick` — passed.

M003-M005 end-to-end behavior remains green: the full fake-transport backend scenario, POP3 reconciliation and delete-commit tests, SMTP dot-stuffing, size limit, recipient rejection, and DeliveryUnknown recovery all pass unchanged.

### Unresolved findings

None at high or medium severity.

Low severity, accepted:

- `messages.receive_state` containing the historical `Sent` token (accepted by the pre-v4 `save_message` string check) now fails closed at migration. No code path in M001-M005 ever wrote that value; `Sent` was only ever a listing projection from `sent_entities`. Treating it as unsupported is the fail-closed choice and is recorded rather than silently coerced.
- `BackendHealth.active_requests` includes the in-flight health probe itself. Documented in `docs/architecture/backend-api.md`; a caller subtracting one gets the count of other in-flight requests.

## Planning updates

- M007 plan status: closed.
- Registry: M007 moves from dependency-ready to closed; M008 becomes dependency-ready.
- Corrective addendum: M007 status closed with this closure record; M008 status ready.
- Parent roadmap milestone row updated.

## Unblock audit

M008 hard-depends only on M007 closure and requires no external i2pr interface for decomposition. M007 is closed with no high/medium finding, so M008 is moved to ready.

M009 remains blocked on M008 closure. M006 remains blocked on M007-M009 closure plus upstream i2pr Plan 354 and Plan 355 closure; no local corrective work can satisfy that interface dependency, and this milestone produced no evidence that changes the upstream state.