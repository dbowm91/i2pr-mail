# MIME boundary

`i2pr-mail-mime` keeps exact raw RFC entity bytes in `RawEntity` and returns a separate parsed view. Inputs are capped at 25 MiB; header extraction is capped at 256 KiB. `mail-parser` 0.11.9 (MIT OR Apache-2.0, `#![forbid(unsafe_code)]`) is used behind the facade. Parser output is never canonical storage.

Outbound text composition uses `mail-builder` 1.0.0 (MIT OR Apache-2.0, no required dependencies); its `gethostname` feature is disabled. Callers must supply Message-ID and UTC Unix timestamp so the builder does not generate them from ambient state. Bcc is envelope-only and excluded from entity headers. No User-Agent or X-Mailer headers are added. Attachments and HTML are not yet exposed by this initial compose subset.
