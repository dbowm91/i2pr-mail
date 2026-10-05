# Mail Backend Foundation Milestone 004 — SMTP Compose, Outbox, and Submission

Status: closed

Repository baseline: planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; execute after M001 and M002 closure

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/001-terminology-and-domain-model.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: capability

## 1. Objective

Deliver privacy-safe MIME compose and durable SMTP submission over an injected I2P byte stream, including attachments, capability/size handling, authenticated submission, and explicit DeliveryUnknown semantics that prevent blind duplicate retries.

## 2. Why this milestone is ready

Blocked on M001/M002. It is independent of POP3 correctness after the shared domain/storage substrate closes.

## 3. Current implementation evidence

No SMTP engine or outbox exists at planning time. Susimail interoperability guidance includes EHLO, PIPELINING, SIZE, 8BITMIME discovery, AUTH LOGIN, MAIL FROM, RCPT TO, DATA, final 250 acceptance, and QUIT.

## 4. Invariants that must not regress

- protocol engine is sans-I/O;
- authentication material is redacted;
- envelope recipients are distinct from message headers;
- Bcc recipients never appear in serialized message headers solely because they are envelope recipients;
- generated Message-ID does not contain local hostname/username/client version;
- generated Date uses a privacy-safe timezone policy (UTC by default);
- no User-Agent or X-Mailer header is added by default;
- exact serialized raw entity is durable before/with submission;
- loss of terminal reply after DATA yields DeliveryUnknown, not automatic retry;
- server SIZE capability is authoritative when advertised.

## 5. Scope

### In scope

- MIME compose for text body, common multipart/alternative where justified, and attachments supported by M002 facade;
- deterministic SMTP state machine;
- greeting, EHLO, capability parsing, AUTH LOGIN, MAIL FROM, RCPT TO, DATA, QUIT;
- PIPELINING where supported/safe;
- SIZE enforcement;
- dot-stuffing at transport DATA layer without modifying stored raw entity;
- durable outbox ownership and attempt state;
- local Sent state after confirmed acceptance;
- safe-retry vs ambiguous-delivery recovery.

### Explicitly out of scope

- opportunistic/direct TLS management;
- arbitrary SMTP servers/providers;
- OAuth;
- DSN ecosystem;
- SMTPUTF8 unless current target service evidence requires it;
- automatic resend of DeliveryUnknown;
- PGP/S/MIME product flows;
- UI compose editor.

## 6. Required production changes

### Compose

Create an explicit compose input that separates:

- From/Reply-To;
- To/Cc header recipients;
- Bcc envelope-only recipients;
- subject;
- text/html body representations supported by the MIME facade;
- attachments;
- In-Reply-To/References where replying;
- caller/runtime-provided current UTC time and cryptographically strong Message-ID source.

Generated raw entity must be committed before SMTP transaction ownership advances beyond Queued.

### SMTP machine

Parse multiline numeric responses and EHLO extensions. Bound line/count/diagnostic sizes. Support fragmented I/O.

Implement AUTH LOGIN without exposing encoded credentials in logs/traces.

DATA transmission applies CRLF normalization policy and dot-stuffing as required to the transmitted stream; the durable raw entity remains the canonical unstuffed RFC entity.

### Outbox state machine

Persist ownership/attempt state and classify failures:

- pre-DATA or explicit rejection before body acceptance -> FailedSafeToRetry where policy permits;
- final acceptance 250 observed -> Sent;
- terminator/body may have reached server but terminal acceptance is unavailable -> DeliveryUnknown.

A startup recovery pass must convert stale Submitting states according to durable stage markers rather than guessing success.

## 7. Ordered work packages

### A — Compose/privacy profile

Implement compose structures and header-generation policy.

Acceptance evidence: golden raw messages and privacy-negative assertions.

### B — SMTP transcript machine

Implement greeting/EHLO/AUTH/envelope/DATA/QUIT with capabilities and bounds.

Acceptance evidence: fragmented/multiline/pipelined/error transcript matrix.

### C — Durable outbox

Implement queue ownership, attempt stages, and restart recovery.

Acceptance evidence: failure injection before/within/after DATA.

### D — Sent reconciliation

On confirmed acceptance, preserve exact raw entity in local Sent state and finalize outbox atomically enough to prevent duplicate submission ownership.

Acceptance evidence: restart immediately around acceptance/finalization.

## 8. Failure, cancellation, restart, and contention semantics

One OutboxId has at most one active submission owner.

Cancellation before DATA can produce retry-safe queued/failed state.

Cancellation or transport loss after body terminator may produce DeliveryUnknown.

Restart of stale Submitting inspects persisted submission stage. It must never downgrade an ambiguous stage to retry-safe automatically.

Per-recipient rejection is recorded explicitly; partial recipient acceptance policy must be frozen before sending DATA. Do not silently resend already accepted recipients.

## 9. Compatibility and migration

Any new durable submission-stage fields require explicit M002-schema migration. SMTP/MIME serialized forms are pre-release but golden fixtures become compatibility evidence once first release ships.

## 10. Required tests

### Compose/privacy

- simple text;
- unicode/encoded headers within supported profile;
- To/Cc/Bcc separation;
- attachments;
- reply references;
- random Message-ID format;
- UTC Date;
- absence of hostname, OS username, local timezone, User-Agent, X-Mailer, router/client versions.

### SMTP

- EHLO multiline capabilities;
- AUTH LOGIN success/failure with redaction;
- PIPELINING on/off;
- SIZE below/equal/above limit;
- multiple recipients and rejection;
- DATA dot-stuffing and terminator;
- malformed/overlong response;
- disconnect at every stage.

### Outbox/restart

- queue survives restart;
- safe retry before DATA;
- ambiguous after DATA;
- confirmed acceptance then crash before cleanup;
- duplicate concurrent submit refused;
- DeliveryUnknown not auto-retried.

## 11. Required verification commands

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-mail-mime
cargo test --locked -p i2pr-mail-proto smtp
cargo test --locked -p i2pr-mail-runtime smtp
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

## 12. Documentation updates

- docs/architecture/smtp.md;
- docs/security/privacy.md;
- outbox recovery/operator diagnostics;
- roadmap/registry;
- closure record 004-status.md.

## 13. Acceptance criteria

- synthetic Postman-compatible SMTP transcript accepts a composed message through injected transport;
- stored raw entity is standards-compliant for the supported compose subset;
- privacy-header negative assertions pass;
- server size limit is enforced;
- ambiguous delivery is durable and not automatically retried;
- confirmed send produces local Sent state without duplicate ownership.

## 14. Stop conditions

Stop if:

- target service requires authentication/SMTP extension outside the frozen supported profile;
- MIME dependency cannot generate exact required output without local-identity leakage;
- partial recipient acceptance cannot be represented safely by current outbox model;
- transport seam requires direct host networking;
- a proposed retry rule can duplicate ambiguously accepted mail.

## 15. Closure evidence required

Golden message fixtures, privacy leak sentinel matrix, SMTP transcript/capability matrix, failure-stage/restart matrix, outbox contention evidence, exact verification results, dependency/security review, and unblock audit for M005 (partial; M003 also required).

## 16. Handoff notes

Reliability semantics are more important than aggressive retry. Preserve DeliveryUnknown even if it is inconvenient for the eventual UI.
