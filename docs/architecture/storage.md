# Storage and recovery

`i2pr-mail-store` stores metadata in SQLite and immutable raw entities as SHA-256-addressed files below the private store root. A temp file is fully written and synced before rename; the directory is synced before metadata insertion. A crash between rename and database commit can leave only an unreferenced blob. Startup removes only `*.tmp` files and never prunes committed blobs.

Schema version 3 evolves monotonically from v1: v2 adds durable outbox sender/recipient metadata and Sent entity records; v3 adds local deletion tombstones. The base schema creates message/UIDL, draft, outbox, and raw-entity tables with a per-account unique UIDL index. Message/raw writes commit the blob before publishing metadata. Outbox claim uses a compare-and-set update; restart maps stage 0 to `FailedSafeToRetry`, and stage 1 or later to `DeliveryUnknown`. No credential input exists in the ordinary store API.

SQLite access is synchronous and one `Store` owner serializes mutable operations. Runtime must not call it from an unbounded async worker; executor integration is a runtime responsibility.
