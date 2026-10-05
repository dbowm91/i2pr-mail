# ADR-0001: Backend-first I2P mail and transport boundary

Status: accepted

Date: 2026-10-05

Decision owners: i2pr-mail maintainers

Related canonical sections:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md
- plans/002-long-term-roadmap.md

Affected roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md

## Context

The project is intended to become a simple Susimail-like client for I2P in-network mail. The i2pr managed-native-app runtime is still being defined: its Plan-345 contract foundation exists, Plan 349 corrects pre-runtime direction/reply and broker semantics, and a router-side application transport gateway is not yet a stable downstream API. (Upstream status corrected 2026-10-05: Plans 345, 349, 352, and 353 have since closed, Plan 354 is ready, and the gateway this ADR lacks remains Plan 355, which is blocked on 354. The decision below is unchanged; only the dated observation of upstream state was stale.)

Tying mail protocol/domain code directly to the current i2pr draft API would force churn into unrelated mail logic. Recreating conventional localhost POP3/SMTP tunnels would also conflict with the secured managed-app design, which denies direct loopback/network access.

## Decision drivers

- make the mail backend useful before the native-app runtime is finished;
- preserve a narrow, auditable network authority;
- permit deterministic transcript testing without network access;
- avoid router/SAM/I2CP details in mail-domain logic;
- keep the future frontend replaceable;
- provide restart-safe receive/send semantics suitable for unreliable anonymity-network conditions.

## Considered options

### Directly depend on current i2pr app protocol

Rejected for the foundation. The downstream interface is intentionally still changing.

### Use localhost POP3/SMTP client tunnels

Rejected as the architectural contract. It introduces direct host networking/loopback assumptions that the secured app profile is designed to remove.

### Make mail protocol code own SAM/I2CP

Rejected. It duplicates router-facing concerns and couples mail semantics to an I2P control protocol.

### Abstract authorized byte streams

Accepted. Mail protocol and orchestration depend on a narrow MailTransport interface. Tests can supply in-memory streams; the eventual i2pr adapter supplies authorized I2P streams.

## Decision

The repository is backend-first and layered as:

mail-domain <- mail-mime
mail-domain <- mail-proto
mail-domain <- mail-store
mail-domain + mail-mime + mail-proto + mail-store <- mail-runtime
mail-runtime <- future backend binary/service
MailTransport is runtime-owned; the later i2pr integration implements it without changing lower layers.

POP3 and SMTP protocol engines are sans-I/O. They do not resolve names, open sockets, read ambient clocks, or spawn tasks.

Raw RFC message entities are durable; metadata/outbox/UIDL state is transactional; credentials are not part of ordinary mailbox persistence.

The initial service profile is I2P-only Postman-compatible POP3/SMTP. Generic clearnet mail is not a fallback.

## Consequences

Positive: development can proceed independently of i2pr API churn; protocol behavior is replayable and fuzzable; security authority stays narrow.

Negative: an adapter layer is required before real i2pr managed-app operation; some conventional mail libraries may not fit because they insist on owning sockets/TLS.

Neutral: the first executable may use test/fake transport until the managed-app gateway exists.

## Compatibility and migration

There is no released compatibility obligation at ADR adoption. Public backend DTOs and persistent schemas still require explicit versioning before release.

## Security and reliability implications

The secured product must fail closed if no authorized I2P transport is available. It must not silently fall back to direct network or loopback sockets.

SMTP ambiguous delivery and POP3 transactional deletion are first-class durable states.

## Verification

Each foundation milestone must prove dependency direction and deterministic protocol behavior. The i2pr adapter milestone must include a negative test showing direct network/loopback fallback is absent.

## Supersession

A future ADR may supersede this decision only if it preserves an equally explicit network authority and migration path.