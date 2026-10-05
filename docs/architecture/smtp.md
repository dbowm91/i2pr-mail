# SMTP submission

Mail composition uses explicit From/To/Cc/Bcc envelope inputs, text, attachments, UTC Unix time, and caller-supplied Message-ID. Bcc is excluded from raw MIME headers and retained in the private outbox envelope table. `mail-builder` hostname generation is disabled.

`submit_smtp` uses injected transport, bounded EHLO response parsing, AUTH LOGIN, server SIZE limits, MAIL/RCPT, and DATA. It persists stage 1 before body transmission, canonical raw entity remains unchanged, line endings are normalized only for wire DATA, and leading dots are doubled. A lost terminal reply persists `DeliveryUnknown`; only explicit 250 acceptance atomically stores Sent and closes the outbox. No automatic retry of ambiguous state occurs.

SMTP recipient rejection occurs before DATA and leaves a retry-safe failure. The current stream API is synchronous; production cancellation/deadline qualification remains open as documented in the runtime architecture.
