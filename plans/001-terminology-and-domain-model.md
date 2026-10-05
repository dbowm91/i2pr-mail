# i2pr-mail Terminology and Domain Model

Status: canonical

## Identity terms

AccountId — stable local identity for one configured mail account.

MessageId — stable local identity for one durable message entity.

DraftId — stable local identity for one editable draft.

OutboxId — stable local identity for one submission attempt/work item.

BlobId — stable local identity for raw message or attachment content.

Uidl — opaque POP3 UIDL associated with an AccountId. It is not parsed for meaning.

Pop3Ordinal — transient session-local message number. Never persisted as message identity.

MessageIdHeader — RFC Message-ID value from or generated for a message. It is mail metadata and is distinct from local MessageId.

## Service profile

The initial service profile has two logical endpoints:

- POP3: pop.postman.i2p, service port 110.
- SMTP: smtp.postman.i2p, service port 25.

These values describe I2P service destinations. They do not imply localhost listeners or direct host-network permission.

## Message state

Remote receive state distinguishes at least:

- RemoteKnown — UIDL exists remotely.
- HeaderCached — durable header metadata exists locally.
- BodyCached — complete raw entity exists locally.
- DeletePending — local policy requests remote deletion.
- DeleteMarkedSession — DELE was accepted in the current POP3 transaction.
- RemoteDeletionCommitted — later reconciliation confirms the message is no longer remote.

Local mailbox roles may include Inbox, Drafts, Sent, Trash, and Bulk/Spam as local presentation/state categories. POP3 itself does not supply a folder model.

## Submission state

Submission state distinguishes:

- Draft — not queued.
- Queued — durable and eligible for submission.
- Submitting — an active SMTP transaction owns the attempt.
- FailedSafeToRetry — failure occurred before ambiguous server acceptance.
- DeliveryUnknown — DATA may have been accepted but terminal confirmation is unavailable.
- Sent — terminal server acceptance was observed and the local sent entity is durable.

DeliveryUnknown is not equivalent to FailedSafeToRetry.

## Transport terms

MailTransport — runtime-owned interface capable of opening an authorized logical byte stream for the POP3 or SMTP I2P service.

Protocol machine — deterministic sans-I/O POP3 or SMTP state machine that consumes bytes/events and emits bytes/actions without opening sockets.

i2pr adapter — later implementation of MailTransport using the stable managed-native-app capability API. It must not expose router internals to domain/protocol crates.

## Persistence terms

Raw entity — exact RFC 5322/MIME bytes received from POP3 or serialized for SMTP DATA prior to transport dot-stuffing.

Metadata store — transactional database containing identities, message summaries, UIDL mappings, mailbox state, outbox state, and blob references.

Blob store — filesystem-backed immutable content store for raw entities and large attachment payloads.

## Privacy terms

Local-identifying metadata — hostname, local username, host/network addresses, machine architecture, local timezone, router/client software versions, or other implementation data that is unnecessary for interoperable mail.

Remote content — MIME/HTML/attachment data originating outside the process trust boundary. Parsing must be bounded and rendering must not imply network permission.