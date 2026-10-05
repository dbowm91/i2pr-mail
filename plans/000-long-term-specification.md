# i2pr-mail Long-Term Specification

Status: canonical

## 1. Product purpose

i2pr-mail is a purpose-built Rust mail client for mail services reachable inside I2P. Its first supported service profile is the Postman-compatible POP3/SMTP model used by Susimail. It is not a generic desktop Internet-mail client.

The project is backend-first. The backend must be independently usable and testable before any graphical frontend is selected.

## 2. Stable ownership boundaries

The product is divided into:

- pure mail domain vocabulary;
- deterministic MIME and POP3/SMTP protocol machinery;
- durable mailbox/blob persistence;
- asynchronous orchestration over an abstract authorized byte-stream transport;
- a frontend-neutral backend service API;
- a later i2pr managed-native-app transport adapter;
- later frontend work outside the foundation.

Router protocol state, I2P tunnel construction, app sandboxing, package lifecycle, and Proposal 170 administrator authority belong to i2pr, not i2pr-mail.

## 3. Network and anonymity invariants

The secured product path must not require direct clearnet, LAN, loopback, public DNS, or unrestricted host networking.

Mail transport is authorized I2P connectivity supplied through a transport abstraction. The backend must not depend on localhost POP3/SMTP tunnels as its architectural contract.

Basic mail operation does not require a general Proposal 170 administrator credential.

The application must not claim that ordinary email provides end-to-end anonymity or end-to-end encryption. Messages forwarded into conventional Internet mail inherit conventional email metadata and provider visibility.

## 4. Mail-protocol invariants

POP3 UIDL is an opaque persistent remote identifier. POP3 message numbers are session-local.

Remote POP3 deletion is a transaction whose final state is reconciled after successful QUIT or a later UIDL refresh; a dropped connection must not be treated as proof of deletion.

SMTP submission has an explicit ambiguous-delivery state. If DATA may have reached the server but the terminal acceptance reply is unknown, automatic resend is forbidden until policy or user action resolves ambiguity.

Server-advertised limits and capabilities are authoritative within the supported protocol profile.

## 5. Privacy invariants

Generated outbound mail must not expose local hostname, OS username, machine architecture, local timezone, router type/version, i2pr-mail version, or implementation-specific User-Agent/X-Mailer headers.

Message-IDs are generated from cryptographically strong randomness under a non-host-identifying mail domain convention.

HTML content must not cause background network fetches. A future renderer must sanitize untrusted HTML and block external resources by default.

Attachment payloads are treated as opaque user data unless an explicit future sanitization feature is invoked.

## 6. Persistence invariants

The exact raw RFC 5322/MIME entity received or composed is durably retainable.

Large raw entities and attachment payloads should not require duplication as SQLite BLOBs. Metadata and state should remain transactional while message/blob storage uses bounded, atomic filesystem operations.

Credentials and authentication payloads are not ordinary mailbox state and must not be written to the mailbox database, raw-message store, logs, or diagnostic traces.

Persistent schemas should permit multiple AccountId values even if the first product surface exposes one account.

## 7. Backend API invariants

The backend API exposes mail-domain operations, not POP3 sequence numbers, SMTP command details, SAM/I2CP internals, or filesystem paths.

Bulk bodies and attachments use opaque handles or streams rather than being forced through small control/event JSON payloads.

Commands and events must be bounded. Restart must reconcile incomplete POP3 deletion and SMTP submission state.

## 8. Initial supported capability

A backend foundation is functionally complete when it can:

- authenticate to the supported POP3 service through an injected I2P stream;
- discover and reconcile UIDLs and message sizes;
- fetch headers before bodies and retain mail durably for offline access;
- compose standards-compliant MIME mail with attachments;
- submit mail through the supported SMTP service with correct ambiguity handling;
- maintain drafts, sent state, deletion state, and a durable outbox;
- expose those operations through a frontend-neutral service surface;
- pass deterministic transcript, restart, corruption, privacy, and negative-network tests.

The later i2pr adapter makes this backend a managed native application without changing the mail-domain contract.

## 9. Explicit long-term non-goals unless separately approved

The initial roadmap does not include generic IMAP, Gmail/Exchange provider integration, OAuth provider ecosystems, clearnet SMTP/POP3, calendars, contacts synchronization, a webmail server, an embedded localhost API server, remote HTML-resource loading, or automatic OpenPGP/S/MIME key management.