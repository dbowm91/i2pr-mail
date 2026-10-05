# POP3 receive

`i2pr-mail-proto` parses bounded status/multiline data without I/O. `i2pr-mail-runtime::sync_pop3` authenticates over an injected `MailTransport`, reads STAT/UIDL/LIST, reconciles solely by opaque UIDL, fetches TOP headers first, and falls back to RETR where TOP is unavailable. `fetch_pop3_body` explicitly retrieves a full entity later. Snapshot ordinals are used only within the active session.

Receive entities are committed to the store before their cache state advances. DELE becomes `DeleteMarkedSession`; only successful QUIT advances to `RemoteDeletionCommitted`. If QUIT fails, the next UIDL snapshot either confirms disappearance or returns the item to `DeletePending`. Authentication failures and malformed input produce typed errors without echoing server text or credential values.

Each response line is capped at 8 KiB; a multiline response is capped at 100,000 lines and 16 MiB aggregate. Operations accept a shared `OperationControl` with a deadline and cancellation flag; transport implementations must observe it and return typed timeout/cancel errors. Tests cover pre-open cancellation and timeout/cancel error propagation.
