# SMTP submission

Mail composition uses explicit From/To/Cc/Bcc envelope inputs, text, attachments, UTC Unix time, and caller-supplied Message-ID. Bcc is excluded from raw MIME headers and retained in the private outbox envelope table. `mail-builder` hostname generation is disabled.

`submit_smtp` uses injected transport, bounded EHLO response parsing, AUTH LOGIN, server SIZE limits, MAIL/RCPT, and DATA. The canonical raw entity remains unchanged, line endings are normalized only for wire DATA, and leading dots are doubled.

## Bounded credentials and envelope lines

Credentials are validated against a frozen pre-expansion ceiling and their base64 expansion is computed and bounded before any encoding happens, so an over-long AUTH LOGIN response never allocates or writes. A credential that would expand past the 1 KiB auth-line budget is rejected before the base64 engine runs.

`MAIL FROM` and `RCPT TO` lines are built by the protocol crate within a 512-byte envelope budget including CRLF. An address that cannot fit is rejected, and an address carrying its own angle brackets is rejected because the builder supplies them. Both checks run before the outbox row is queued and before a stream is opened, so an over-bound or malformed envelope has no durable and no network effect.

## Delivery ambiguity

Submission progress is typed. The store records `SubmissionProgress::DataMayHaveStarted` with `SubmissionState::Submitting` before any DATA byte is written. A lost terminal reply or a write failure persists `DeliveryUnknown`, which is non-retryable until an explicit resolution decision; only explicit 250 acceptance atomically stores Sent and closes the outbox. Restart recovery maps a submission that never reached DATA to `FailedSafeToRetry` and one that may have started to `DeliveryUnknown`. No automatic retry of ambiguous state occurs, and an impossible state/progress pair is rejected before it can be written.

SMTP recipient rejection occurs before DATA and leaves a retry-safe failure. The current stream API is synchronous; production cancellation/deadline qualification remains open as documented in the runtime architecture.