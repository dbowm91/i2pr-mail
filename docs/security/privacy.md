# Privacy boundaries

- Authentication is supplied to one operation and is absent from domain/store models and error variants.
- Runtime errors discard server response text; diagnostics must use the typed result only.
- Generated MIME requires caller-supplied Message-ID and UTC time. The builder's hostname feature is disabled; no User-Agent/X-Mailer is emitted.
- Bcc addresses are SMTP envelope data only and are not serialized into message headers.
- POP3 UIDL is opaque metadata and never forms a filesystem path.
- Only runtime's `MailTransport` can open a logical service stream. The current production transport implementation is intentionally absent; tests use synthetic streams.
