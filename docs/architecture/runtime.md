# Runtime ownership

`MailTransport` is runtime-owned and opens only a logical `MailService` stream; protocol machines cannot name hosts or ports. Tests inject an in-memory stream. Every open receives a shared `OperationControl` with a monotonic deadline and cancellation flag. Production stream implementations must enforce both while connecting and doing I/O, and return typed timeout/cancel errors. Protocol driving is synchronous in this foundation; executor integration is a later runtime composition concern and may not call blocking I/O on an async executor thread.
