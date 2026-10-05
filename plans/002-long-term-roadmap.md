# i2pr-mail Long-Term Roadmap

Status: canonical

## Phase 0 — planning and architecture foundation

Establish the planning system, product invariants, backend ownership boundaries, and durable architecture decisions.

## Phase 1 — backend foundation

M001: Rust workspace, domain vocabulary, dependency direction, bounds, and verification skeleton.

M002: MIME boundary, durable SQLite metadata, immutable raw-entity/blob storage, migrations, and restart-safe persistence.

## Phase 2 — receive capability

M003: deterministic POP3 protocol machine plus receive/sync orchestration, header-first fetch, UIDL reconciliation, and transactional remote deletion.

## Phase 3 — send capability

M004: deterministic SMTP protocol machine plus MIME compose/outbox orchestration, privacy-safe generated headers, attachments, and explicit ambiguous-delivery handling.

## Phase 4 — backend convergence

M005: frontend-neutral backend service surface combining offline state, sync, fetch, draft, send, delete, events, and bounded content handles. This phase is the backend functional qualification point.

## Phase 5 — i2pr managed-app integration

M006: implement MailTransport against the stable i2pr managed-native-app gateway. This is interface-blocked until the corrected app contract and an authorized I2P application transport exist.

The secured adapter must not fall back to direct host sockets or localhost POP3/SMTP listeners.

## Phase 6 — frontend work

Frontend selection, graphical rendering, HTML sanitization presentation policy, accessibility, and UX are a separate future workstream. The backend contract should make those choices replaceable.

## Deferred extensions

Generic Internet mail accounts, IMAP, provider OAuth, OpenPGP/S/MIME product flows, contacts, calendar integration, multiple service providers, attachment metadata sanitization, and clearnet relay support require separate product decisions and roadmaps.