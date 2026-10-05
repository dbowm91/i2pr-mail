# Runtime ownership

`i2pr-mail-runtime` is decomposed into single-owner modules and `lib.rs` is only
the composition and re-export surface:

| Module | Owns |
|---|---|
| `transport` | `MailTransport`, `ByteStream`, `OperationControl`, `TransportError`, and bounded line plumbing (`read_line`, `write_command`, `write_line`) with neutral `LineError`. |
| `pop3` | POP3 stream, header-first synchronization, deletion reconciliation, on-demand body fetch, and `SyncError`. |
| `smtp` | SMTP stream, AUTH LOGIN submission, dot-stuffing, and `SubmitError`. |
| `backend` | `BackendService`, the request ledger, results, events, content handles, and `BackendError`. |
| `tests` | Shared synthetic transport fixtures plus per-owner test modules. |

`MailTransport` opens only a logical `MailService` stream; protocol machines cannot name hosts or ports. Tests inject an in-memory stream. Every open receives a shared `OperationControl` with a monotonic deadline and cancellation flag. Production stream implementations must enforce both while connecting and doing I/O, and return typed timeout/cancel errors. `OperationControl::check` reports a transport-level `TransportError`, which each protocol driver maps into its own error vocabulary; the transport module does not depend on protocol errors.

Protocol driving is synchronous in this foundation; executor integration is a later runtime composition concern and may not call blocking I/O on an async executor thread. The adapter contract for the future i2pr integration, including its execution-model stop condition, is documented in `transport-boundary.md`.