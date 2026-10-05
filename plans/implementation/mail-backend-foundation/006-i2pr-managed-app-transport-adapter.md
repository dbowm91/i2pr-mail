# Mail Backend Foundation Milestone 006 — i2pr Managed-App Transport Adapter

Status: blocked

Repository baseline: planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; execute only after M005 closure and a stable i2pr managed-app transport contract

Source roadmap:

- plans/subsystems/mail-backend-foundation-roadmap.md#7-milestones

Long-term requirements:

- plans/000-long-term-specification.md
- plans/002-long-term-roadmap.md

Applicable ADRs:

- plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md

Primary class: capability + integration

## 1. Objective

Implement MailTransport using the stable i2pr managed-native-app capability channel so the qualified backend can reach the Postman POP3/SMTP I2P destinations without direct host networking or localhost tunnel dependencies.

## 2. Why this milestone is ready

It is not ready.

Hard dependency: M005 closed.

Interface dependencies in dbowm91/i2pr:

- Plan 349 must close the pre-runtime v1 direction/reply/broker-policy corrective;
- the router-side app-principal gateway must exist and expose a stable authorized path suitable for application I2P connectivity;
- any required SAM/I2CP adapter semantics must be stable enough for a downstream client.

Proposal 170 administrative completion is not a blanket prerequisite unless the final app gateway specifically and narrowly requires it for an app-owned resource. A general administrator credential is forbidden.

## 3. Current implementation evidence

The i2pr Plan-345 branch provides a runtime-neutral i2pr-app-proto contract but explicitly no live runtime, socket, process launcher, DNS owner, or router adapter. Plan 349 corrects message direction/request correlation and reserves the currently underspecified brokered_tcp open operation. Therefore the present contract is not sufficient authority for production mail transport.

## 4. Invariants that must not regress

- no direct std/tokio TCP socket path to POP3/SMTP in secured mode;
- no localhost 7659/7660 dependency;
- no brokered clearnet capability;
- no arbitrary public DNS requirement;
- app receives only its scoped I2P connectivity, not router internals or administrator credential;
- lower mail crates remain unaware of i2pr protocol details;
- failure to obtain authorized app transport fails closed;
- adapter cannot silently switch to UnsafeDirect/direct-network mode.

## 5. Scope

### In scope

- dependency/adaptation to the released/stable i2pr app SDK/protocol component;
- application manifest capability declarations required for mail;
- implement MailTransport for logical POP3 and SMTP I2P services;
- destination resolution/connect semantics through the authorized i2pr path;
- lifecycle/cancellation mapping;
- app health/diagnostic mapping as appropriate;
- deterministic adapter tests plus real/local integration smoke where the i2pr project supports it.

### Explicitly out of scope

- changing i2pr router protocol behavior inside this repo;
- implementing SAM/I2CP from scratch unless explicitly selected as the downstream adapter layer after interface review;
- Proposal 170 administration UI;
- clearnet broker;
- direct host sockets;
- GUI;
- packaging/store/signature policy owned by i2pr runtime.

## 6. Required production changes

### Interface review first

At execution start, inspect the exact closed i2pr contract and gateway. Freeze which operation produces the duplex I2P stream required by MailTransport.

If i2pr exposes SAM or I2CP as a capability stream rather than a direct destination stream, prefer a small project-owned adapter or separately maintained Rust SAM library over leaking that protocol into mail-domain/proto/store.

### Adapter

Implement only the two service intents needed by the backend:

- POP3 to pop.postman.i2p:110;
- SMTP to smtp.postman.i2p:25.

Do not expose a generic arbitrary-host connector through the backend API solely because the i2pr capability could support it.

### Manifest/capabilities

Request only the minimum capabilities required by the final i2pr contract plus lifecycle/health/UI-bridge when future consumer work requires them. Do not request brokered_tcp/clearnet for basic in-network mail.

## 7. Ordered work packages

### A — Downstream contract qualification

Record exact i2pr commit/tag/API and map it to MailTransport.

Acceptance evidence: interface matrix and explicit authority analysis.

### B — Adapter implementation

Implement open/cancel/close mapping with typed errors.

Acceptance evidence: deterministic fake app-channel/gateway tests.

### C — Integration smoke

Run against supported i2pr local/test environment and synthetic or actual reachable I2P service as appropriate.

Acceptance evidence: POP3 and SMTP stream establishment through app authority.

### D — Negative authority qualification

Prove secured operation has no direct/loopback fallback and denied/missing capability fails closed.

## 8. Failure, cancellation, restart, and contention semantics

App-channel loss aborts owned streams and leaves mail-runtime to its already-qualified POP3/SMTP recovery states.

Capability denial is a typed permanent/configuration failure, not a retry storm.

Router unavailable may be transient with bounded backoff owned by mail-runtime or adapter, but only one layer owns retry timing.

Application restart obtains fresh scoped capability state; no stale stream/resource handle is reused.

## 9. Compatibility and migration

Pin/declare the minimum compatible i2pr app protocol/SDK version. Do not vendor private router internals.

If i2pr changes the adapter contract before release, update this adapter only; lower mail APIs remain unchanged.

## 10. Required tests

- capability handshake success/failure;
- wrong/missing capability;
- POP3 logical service stream;
- SMTP logical service stream;
- fragmented app-channel data;
- cancellation/close/reset mapping;
- app-channel/router restart;
- no generic host connector exposed to consumer API;
- static/negative check for direct socket/loopback fallback in secured adapter.

## 11. Required verification commands

Use the repository routine floor plus adapter-focused tests. Exact i2pr interop commands must be taken from the stable upstream gateway at implementation time and recorded in closure rather than invented now.

Minimum local floor remains:

cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick

## 12. Documentation updates

- docs/architecture/i2pr-integration.md;
- README managed-app operation;
- exact supported i2pr contract/version;
- capability/security notes;
- roadmap/registry;
- closure record 006-status.md.

## 13. Acceptance criteria

- qualified backend opens POP3/SMTP I2P service streams through i2pr managed-app authority;
- receive and send scenarios work without direct host or localhost networking;
- capability denial fails closed;
- lower mail-domain/mime/proto/store contracts are unchanged by integration;
- exact upstream compatibility is documented.

## 14. Stop conditions

Stop if:

- Plan 349 is not closed;
- no stable router-side app transport exists;
- integration requires UnsafeDirect or localhost socket fallback;
- integration requires handing the app a general Proposal 170 administrator credential;
- the only available surface is the intentionally reserved/underspecified generic brokered_tcp operation;
- upstream API churn would require leaking i2pr internals into lower mail crates.

## 15. Closure evidence required

Exact upstream commit/tag, app capability matrix, adapter mapping, POP3/SMTP integration transcript, negative no-fallback evidence, restart/cancellation behavior, verification commands/results, security review, compatibility declaration, and roadmap completion audit.

## 16. Handoff notes

This plan is deliberately blocked. Do not implement against the known-pre-runtime Plan-345 shape merely to make progress; M001-M005 are the progress path until upstream stabilizes.

### Current external dependency evidence (checked 2026-10-05)

The upstream i2pr registry at commit `bf257b2b20ecf81d3772181bb566b03cf29ca795` registers Plan 345 as a managed native app runtime contract/architecture foundation. It does not provide a production application gateway, managed app process lifecycle, or destination stream API for this adapter. The upstream README continues to describe i2pr as experimental. Therefore the adapter has no stable interface to target and remains blocked. Recheck the upstream registry and the accepted gateway API before starting implementation.

- Registry: https://github.com/dbowm91/i2pr/blob/main/plans/registry.md (registry blob `b899c95159fad860e6970abec91d1f5253011fbc`)
- Project readiness: https://github.com/dbowm91/i2pr
