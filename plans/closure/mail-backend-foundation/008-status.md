# Mail Backend Foundation M008 Closure — Runtime Decomposition and Transport Seam Stabilization

Status: closed

Closed: 2026-10-05

Predecessor closure: `plans/closure/mail-backend-foundation/007-status.md`

Source plan: `plans/implementation/mail-backend-foundation/008-runtime-decomposition-and-transport-seam.md`

## Outcome

The runtime is decomposed into explicit single-owner modules and `lib.rs` is now only a composition and re-export surface. The move is a behavioral no-op: the public API is item-for-item identical, all 57 workspace tests pass unchanged, and no network or router capability was added. The downstream adapter contract for M006 is frozen in `docs/architecture/transport-boundary.md`, including the execution-model stop condition that M008 is not allowed to pre-empt.

## Ownership map

Before: one `crates/mail-runtime/src/lib.rs` at 2379 lines owning transport, POP3, SMTP, backend lifecycle/API, and all tests.

After:

| Module | Lines | Owns |
|---|---|---|
| `lib.rs` | 34 | module composition and public re-exports only |
| `transport.rs` | 114 | `MailTransport`, `ByteStream`, `OperationControl`, `TransportError`, bounded line plumbing with neutral `LineError` |
| `pop3.rs` | 322 | POP3 synchronization, deletion reconciliation, body fetch, `SyncError`, `SyncReport` |
| `smtp.rs` | 296 | SMTP submission, dot-stuffing, `SubmitError`, `SmtpSubmission` |
| `backend.rs` | 630 | `BackendService`, request ledger, results, events, content handles, `BackendError` |
| `tests/fixtures.rs` | 150 | reusable synthetic stream/transport fixtures |
| `tests/pop3_tests.rs` | 253 | POP3 tests, next to their owner |
| `tests/smtp_tests.rs` | 266 | SMTP tests |
| `tests/backend_tests.rs` | 422 | backend workflow, ledger, and release tests |
| `tests/transport_tests.rs` | 54 | transport control, cancellation, timeout tests |

No authority was duplicated. Retry, deletion reconciliation, `DeliveryUnknown`, outbox ownership, content limits, deadlines, and request dedupe each still have exactly one owner in `backend.rs` plus `smtp.rs`, unchanged from M003-M007.

## Public API and re-export diff

An item-level comparison of every `pub fn`/`pub struct`/`pub enum`/`pub trait`/`pub const` and every public inherent method, before versus after:

- public items before: 53
- public items after: 53
- removed: none
- added: none

The re-export surface in `lib.rs` is explicit, so the public API is now reviewable in one place rather than being a side effect of file layout.

## Behavior-preserving adjustments

Decomposition required three internal signature changes. None alters observable behavior, and all are covered by existing tests:

- `OperationControl::check` now returns `Result<(), TransportError>` instead of a protocol error, removing a transport-to-pop3 dependency. `check` only ever produced `Cancelled` or `Timeout`, and both protocol drivers already mapped those two transport variants to their own `Cancelled`/`Timeout` values, so the resulting errors are identical.
- `transport::read_line` returns a neutral `LineError`; `pop3` and `smtp` each map it into their own typed error. `LineError::TooLong` maps to `SyncError::Limits` for POP3 and `SubmitError::Protocol` for SMTP, preserving the previous single-line-limit behavior on both paths.
- `smtp_reply` reads through the SMTP-typed line helper directly instead of re-mapping a POP3 error variant.

## Dependency and source census

No new dependency and no network capability was introduced.

| Crate | Direct dependencies | Network/async deps |
|---|---|---|
| mail-domain | serde, thiserror | none |
| mail-mime | i2pr-mail-domain, mail-builder, mail-parser | none |
| mail-proto | i2pr-mail-domain | none |
| mail-store | i2pr-mail-domain, rusqlite, sha2, thiserror | none |
| mail-runtime | base64, i2pr-mail-domain, i2pr-mail-mime, i2pr-mail-proto, i2pr-mail-store, sha2 | none |

- Transitive graph scan for tokio, hyper, reqwest, mio, socket2, async-std, rustls, openssl across `cargo tree --workspace -e normal`: none present.
- Source scan for `std::net`, `TcpStream`, `UdpSocket`, `to_socket_addrs`, DNS resolution, `localhost`, and `127.0.0.1` across all crates: the only two matches are negative test assertions — `ServiceEndpoint::new("localhost", 110).is_err()` in `mail-domain` and a header-leak assertion in `mail-mime`. No production code path references host networking.
- No i2pr, SAM, or I2CP dependency exists anywhere in the workspace.

`scripts/check-boundaries.sh` needed no change for the move itself: the crate dependency set, the lower-crate sans-I/O rule, and the typed durable-state guard all pass unchanged against the new module tree.

## Transport integration review

`docs/architecture/transport-boundary.md` records, for M006:

- the runtime asks only for an authorized logical mail transport and nothing more;
- lower layers never see SAM, I2CP, or app framing, and the boundary guard enforces it;
- Plan 355 maps one managed-app logical service stream to one protocol connection, so the adapter requests a stream per operation rather than assuming a pre-connected tunnel;
- no localhost fallback is permitted — an unavailable gateway fails the operation with a transport error rather than reintroducing a second authority;
- any SAM client dependency belongs at or above the adapter boundary.

The execution-model decision is explicitly deferred. If the closed Plan 355 SDK exposes only asynchronous channels, M006 must record an architecture decision before changing `MailTransport` or adding a blocking bridge. No ADR was added here, because documenting a guess before Plan 355 closes would present speculation as an accepted decision. M008 made no async conversion and added no bridge.

## M005 scenario proof

The M005 full fake-transport backend scenario, `backend_service_runs_fake_mail_workflow_restarts_offline_and_bounds_handles`, passes unchanged in `tests/backend_tests.rs`. It still covers POP3 sync, listing, open/read with bounds, draft create/delete, outbox queue and send, Sent visibility, request-ID rejection, handle release and staleness, local delete persistence, restart recovery of an interrupted submission as `DeliveryUnknown`, refusal to retry ambiguous delivery, explicit resolution, and offline read of both received and sent content after restart.

## Verification

Commands run from the repository root, all passing:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-mail-runtime` — passed, 19 tests.
- `cargo test --locked --workspace --all-targets` — passed, 57 tests total.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed, including both positive controls.
- `bash scripts/verify.sh quick` — passed.

## Unresolved findings

None at high or medium severity.

Low severity, accepted:

- `OperationControl::check` now returns the transport-level `TransportError` rather than a protocol error. This is a public signature change with identical observable behavior, made so the transport module no longer depends on a protocol error type. Recorded here rather than hidden.

## Planning updates

- M008 plan status: closed.
- Registry: M008 moves from dependency-ready to closed; M009 becomes dependency-ready.
- Corrective addendum: M008 status closed with this closure record; M009 status ready.
- Parent roadmap milestone row updated.

## Unblock audit

M009 hard-depends only on M008 closure and requires no external interface to add hosted verification. M008 is closed with no high/medium finding, so M009 is moved to ready.

M006 remains blocked on M007-M009 closure plus upstream i2pr Plan 354 and Plan 355 closure. This milestone produced no evidence that changes that upstream state, and it explicitly did not pre-decide the execution model M006 will need.