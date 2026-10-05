# M004 Closure — SMTP Compose, Outbox, and Submission

Status: closed

## Implementation evidence

- MIME composition takes explicit From/To/Cc/Bcc envelope inputs, text, attachments, caller-provided Message-ID, and UTC timestamp. Header injection is rejected; Bcc never enters raw MIME headers.
- SMTP response parsing is bounded and deterministic. Runtime supports EHLO, SIZE, AUTH LOGIN, MAIL FROM, RCPT TO, DATA, CRLF normalization/dot-stuffing, and QUIT over injected MailTransport.
- Schema v2 migration adds durable sender/recipient envelope metadata and Sent entity records.
- Outbox claim is compare-and-set; retries must retain exact canonical raw bytes, sender, and recipients. DeliveryUnknown cannot be claimed.
- Stage 1 is durable before DATA. Transport loss after DATA remains ambiguous; explicit server acceptance atomically records Sent and closes the outbox.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| privacy-safe compose | Unicode subject, attachment, UTC Date, explicit Message-ID, no Bcc/User-Agent/X-Mailer/local host/user/router sentinel test |
| SMTP multiline capabilities | `i2pr-mail-proto::smtp` multiline and inconsistent-code tests; runtime EHLO fixture |
| AUTH LOGIN and envelope separation | scripted acceptance transcript; Bcc is retained in private recipient metadata only |
| SIZE enforcement | over-limit transcript returns SizeLimit before DATA; normal send succeeds below limit |
| dot-stuffing and raw preservation | wire transcript doubles leading dot; Sent raw retrieval remains byte-exact |
| safe pre-DATA rejection | SIZE and recipient rejection produce FailedSafeToRetry and never send DATA |
| ambiguity and no blind retry | lost final reply produces DeliveryUnknown; claim_outbox rejects it |
| confirmed acceptance/Sent | final 250 atomically finalizes Sent; stored Sent raw entity matches exact input |
| outbox contention/retry integrity | compare-and-set claim; changed content/sender/recipients and DeliveryUnknown retry are rejected |
| schema migration | v1 database migrates to v2 tables and repeated reopen remains v2 |

## Verification

Run on 2026-10-05 with Rust 1.98.1:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked -p i2pr-mail-mime` — passed, 7 tests.
- `cargo test --locked -p i2pr-mail-proto smtp` — passed, 2 tests.
- `cargo test --locked -p i2pr-mail-runtime smtp` — passed, 4 tests.
- `cargo test --locked --workspace --all-targets` — passed, 36 tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed.
- `bash scripts/verify.sh quick` — passed.

## Failure, restart, and security review

Before DATA, protocol/recipient/size failures are retry-safe. After DATA may have been sent, transport loss preserves stage 1 and records DeliveryUnknown. Startup recovery preserves ambiguity. SMTP 250 and Sent row publication share a SQLite transaction; a local finalization failure cannot make the outbox claimable. The canonical entity is unchanged by wire normalization and dot-stuffing. Credentials are ephemeral runtime inputs and are not accepted by storage APIs.

No medium/high unresolved findings at M004 scope. Live SMTP interoperability testing is not claimed; deterministic synthetic transcripts qualify the protocol behavior.

## Unblock audit

M005 is ready because M003 and M004 are closed. M006 remains blocked on M005 closure and stable corrected i2pr managed-app transport/gateway.

## Implementation commit

Recorded in Git history with this closure update.
