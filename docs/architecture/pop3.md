# POP3 receive

`i2pr-mail-proto` parses bounded status/multiline data without I/O. `i2pr-mail-runtime::sync_pop3` authenticates over an injected `MailTransport`, reads STAT/UIDL/LIST, reconciles solely by opaque UIDL, fetches TOP headers first, and falls back to RETR where TOP is unavailable. `fetch_pop3_body` explicitly retrieves a full entity later. Snapshot ordinals are used only within the active session.

## Status and command correctness

A status line is recognized only as a complete `+OK` or `-ERR` atom that is CRLF terminated and followed by either end-of-line or a single space delimiter. Truncated or glued forms such as `-ER`, `-ERRX`, `-ERRS`, `+OKX`, a bare `-\r\n`, or a line without CRLF are rejected as protocol errors rather than read as a negative status with stray text.

`USER` and `PASS` are bounded by the RFC 1939 ceiling of 40 octets. The protocol crate validates the credential and formats the complete command line, so an over-bound or non-atom value is rejected before a command buffer is built or written, and the rejection carries no part of the credential. `DELE`, `TOP`, and `RETR` lines are built the same way from the session ordinal.

## Reconciliation

Receive entities are committed to the store before their cache state advances. DELE becomes `DeleteMarkedSession`; only successful QUIT advances to `RemoteDeletionCommitted`. If QUIT fails, the next UIDL snapshot either confirms disappearance or returns the item to `DeletePending`. Cache and deletion transitions use `ReceiveState` values, so no caller can persist an unrepresented receive state. Authentication failures and malformed input produce typed errors without echoing server text or credential values.

Each response line is capped at 8 KiB; a multiline response is capped at 100,000 lines and 16 MiB aggregate. Operations accept a shared `OperationControl` with a deadline and cancellation flag; transport implementations must observe it and return typed timeout/cancel errors. Tests cover pre-open cancellation, timeout/cancel error propagation, exact status-atom recognition, and max/max+1 credential ceilings.