# M003 Closure — POP3 Receive, Sync, and Reconciliation

Status: closed

## Implementation evidence

- POP3 status, bounded multiline, dot-unstuff, UIDL, and LIST parsing are deterministic sans-I/O functions.
- Runtime sync authenticates over injected `MailTransport`, reads CAPA/STAT/UIDL/LIST, keys all durable receive identity by opaque UIDL, uses only current-session ordinals, and fetches TOP headers before optional full body. Unsupported TOP falls back to RETR.
- Raw message data is committed before cache state advances. Body fetch is explicit.
- DeletePending emits DELE; accepted DELE remains DeleteMarkedSession until successful QUIT. A later UIDL snapshot reconciles missing or still-present entries after uncertain QUIT.
- Runtime commands have bounded line/list/aggregate sizes. Shared operation control communicates deadlines/cancellation to the transport, and runtime maps transport cancellation/timeouts to typed errors.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| bounded status/multiline/dot behavior | `i2pr-mail-proto` POP3 unit tests |
| opaque UIDL, transient ordinal | UIDL parser accepts path-like opaque test value; runtime persists only UIDL and addresses messages through local ids |
| CAPA and auth sequence, secret handling | synthetic runtime POP3 transcript; error values contain only typed variants |
| header-first plus offline body | synthetic TOP transcript persists header; explicit RETR test upgrades same message to BodyCached and exact raw survives store |
| TOP fallback | synthetic transcript rejects TOP and verifies the RETR result is cached as BodyCached |
| deletion transaction | tests for successful QUIT, failed QUIT, and subsequent missing-UIDL reconciliation |
| cancellation and timeout | cancellation before transport open and typed transport error propagation tests |
| no direct socket/name path | runtime accepts only logical MailService through MailTransport; boundary script passed |

## Verification

Run on 2026-10-05 with Rust 1.98.1:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-mail-proto pop3` — passed, 3 tests.
- `cargo test --locked -p i2pr-mail-runtime pop3` — passed, 6 tests.
- `cargo test --locked --workspace --all-targets` — passed, 34 tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed.
- `bash scripts/verify.sh quick` — passed.

## Failure, restart, and security review

Before authenticated snapshot completion no remote state is advanced. Successfully committed messages remain durable if a later fetch fails. DELE ambiguity survives as DeleteMarkedSession and is reconciled on next UIDL listing. Authentication input and server response text are not included in errors. The sync seam requires one runtime owner per account; M005 will provide the single lifecycle owner. No direct transport implementation exists in this milestone.

Unresolved findings: none at M003 scope. No live Postman service test is claimed or required for deterministic protocol qualification.

## Unblock audit

M004 is active and unblocked because M001/M002/M003 are closed. M005 remains blocked on M004 closure. M006 remains blocked on M005 and stable upstream i2pr managed-app transport.

## Implementation commit

Recorded in Git history with this closure update.
