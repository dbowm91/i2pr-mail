# Domain vocabulary

Local `MessageId` values, remote opaque POP3 `Uidl` values, and RFC `MessageIdHeader` values are distinct validated types. Identifier length limits are enforced at construction and deserialization.

## Durable state vocabulary

Receive and submission state are project-owned typed values, not strings. `ReceiveState`, `SubmissionState`, `SubmissionProgress`, and `StoredMessageState` carry the deletion uncertainty and SMTP ambiguity invariants explicitly.

`SubmissionProgress` is a separate bounded marker because SMTP ambiguity needs its own axis: `NotStarted`, `DataMayHaveStarted`, and `DeliveryAccepted`. A state may be persisted only with a progress marker the domain model can represent, and that pairing is expressed once in `SubmissionState::allowed_progress`. `DeliveryUnknown` requires `DataMayHaveStarted`, so ambiguity can never be recorded alongside a marker claiming nothing was written.

`StoredMessageState` is the message-list projection: either a `ReceiveState`, or `Sent` for a delivered submission. `Sent` is a projection of `sent_entities`, not a value the `messages` table accepts.

Each state type owns its SQL encoding (`as_storage_str`, `as_storage_i64`) and its strict decoding (`from_storage_str`, `from_storage_i64`), which returns `None` for any unknown token. Storage vocabulary, generated `CHECK` constraints, and the Rust types therefore cannot drift apart, and an unsupported persisted value fails closed instead of being coerced into a send, delete, or retry decision.