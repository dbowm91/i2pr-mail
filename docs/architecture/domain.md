# Domain vocabulary

Local `MessageId` values, remote opaque POP3 `Uidl` values, and RFC `MessageIdHeader` values are distinct validated types. Receive and submission state enums preserve deletion uncertainty and SMTP `DeliveryUnknown` explicitly. Identifier length limits are enforced at construction and deserialization.
