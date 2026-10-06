# Mail Backend Foundation Milestone 006 — i2pr Managed-App Transport Adapter

Status: blocked

Repository baseline: original planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; corrected on post-M005 planning head; execute only after M007-M009 closure and an upstream app-side managed-app runtime milestone

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

Hard dependencies:

- M005 closed;
- post-M005 corrective M007-M009 must close.

Interface dependencies in dbowm91/i2pr:

- managed-native-app Plans 345, 349, 352, 353, 354, and 355 are closed (verified 2026-10-06 on `main` `2f82c799`);
- Plan 354 closed the listener-independent SAM/I2CP private connection seams;
- Plan 355 closed the router app-principal gateway and froze the authorized SAM/I2CP service-stream contract, but its gateway API is `pub(crate)` to `i2pr-daemon` and no production caller exists;
- an app-side trusted runtime milestone (package/lifecycle supervision, AppManager, and the app-channel transport that can authenticate a process and supply `AppGatewayAuthorization`) must be planned and closed upstream. It is currently "eligible for a future plan" and unregistered;
- any required downstream SAM client adapter semantics must be stable enough for i2pr-mail.

Proposal 170 administrative completion is not a blanket prerequisite unless the final app gateway specifically and narrowly requires it for an app-owned resource. A general administrator credential is forbidden.

## 3. Current implementation evidence

The original Plan-345-only description is stale. Upstream i2pr has now closed Plans 345, 349, 352, 353, 354, and 355. Plan 354 owns listener-independent private SAM/I2CP connection drivers, and Plan 355 owns the trusted AppPrincipal/effective-capability router gateway built on them.

Plan 355's frozen mapping is one managed-app logical stream to one raw SAM or I2CP protocol connection. It does not promise a pre-connected arbitrary I2P destination stream. M006 must therefore begin by reviewing the exact closed Plan-355 interface and deciding the smallest adapter above MailTransport. If a SAM client layer or sync/async execution-model change is required, record that decision before implementation rather than leaking it into lower mail crates.

Plan 355 closed the router half only. Its closure record states it does not implement "a usable third-party application capability", and `AppGatewaySession`, `AppGatewayAuthorization`, `AppGatewayLimits`, and `AppGatewayComposition` are all `pub(crate)` in `crates/i2pr-daemon/src/app_gateway.rs` under a module-level `#![allow(dead_code)]` reading "No production app-runtime caller exists yet". `AppGatewayAuthorization::from_trusted_composition` is deliberately non-serializable and unreachable off-wire, so an application cannot mint its own authority. i2pr-mail therefore cannot integrate against Plan 355 as shipped: there is no app-side channel, no process authentication, and no public surface to adapt.

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

- M007-M009 are not closed;
- upstream i2pr Plan 354 or Plan 355 is not closed (both closed as of `2f82c799`);
- no app-side trusted runtime and reachable app-channel exist to obtain authorized app transport, or the router-side gateway API is not publicly consumable by a downstream process;
- integration requires the application to construct or be granted router-internal authority rather than receiving scoped capability from a trusted host runtime;
- no stable router-side app-principal SAM/I2CP service-stream contract exists;
- integration requires UnsafeDirect or localhost socket fallback;
- integration requires handing the app a general Proposal 170 administrator credential;
- the only available surface is the intentionally reserved/underspecified generic brokered_tcp operation;
- upstream API churn would require leaking i2pr internals into lower mail crates.

## 15. Closure evidence required

Exact upstream commit/tag, app capability matrix, adapter mapping, POP3/SMTP integration transcript, negative no-fallback evidence, restart/cancellation behavior, verification commands/results, security review, compatibility declaration, and roadmap completion audit.

## 16. Handoff notes

This plan is deliberately blocked. Do not implement against the known-pre-runtime Plan-345 shape merely to make progress; M001-M005 are the progress path until upstream stabilizes.

### Current external dependency evidence (re-audited 2026-10-05 during M009, re-audited again 2026-10-06)

The 2026-10-05 audit is superseded. It read upstream branch `work/router-console-plans-356-358` at `f6036a9e` and concluded that no Plan 354 or Plan 355 closure record existed and that `82080dbd` was not an ancestor of `main`.

The 2026-10-06 re-audit reads upstream `main` at `2f82c799` and finds the named blocker cleared:

| Plan | Closure commit | Closure record | Status |
|---|---|---|---|
| 354 | `6cd35bfe` | `plans/closure/managed-native-app-runtime/354-status.md` | `passed-managed-app-private-client-transport-seams` |
| 355 | `2b96f1bc` | `plans/closure/managed-native-app-runtime/355-status.md` | `passed-managed-app-principal-gateway-private-client-seams` |

Both closure commits are ancestors of upstream `main`, as is the `d2f17f38` contract freeze. Upstream's registry and roadmap now read "Plans 345, 349, 352–355 passed" and "private SAM/I2CP seams and router app-principal gateway closed".

M006 nonetheless remains blocked, on a different and better-characterized dependency. Plan 355 closed the router side only:

- `crates/i2pr-daemon/src/app_gateway.rs` declares `AppGatewayAuthorization`, `AppGatewayLimits`, `AppGatewayComposition`, and `AppGatewaySession` as `pub(crate)`, and the module carries `#![allow(dead_code)]` with the comment "No production app-runtime caller exists yet; this infrastructure API is exercised by its module tests until a separately planned consumer lands." No downstream crate and no external process can reach the gateway.
- Plan 355's closure record states it "does not implement an application runtime, process authentication, package management, sandboxing, brokered clearnet, or a usable third-party application capability", and that "the future trusted runtime retains framing and stream-id ownership" because the module does not decode managed-app frames.
- `i2pr-app-proto` is `publish = false` and is described upstream as "vocabulary and pure validation only".
- The v1 contract still records that `hello` "is not authentication proof" and that "a future trusted transport owner must bind that claim to its authenticated process/IPC principal before authorizing access."
- Upstream's dependency graph terminates at `package/lifecycle + AppManager administrative owner (eligible for a future plan)`; the registry confirms "A future AppManager/package/process plan is eligible to be registered against the gateway; none is registered yet."

M006 therefore has no authorized app transport to obtain. This is the correct security posture — an app must not be able to construct its own gateway authority — but it means the adapter cannot be written until upstream supplies the app-side runtime and a reachable channel.

The local corrective lane is complete: M007, M008, and M009 are closed with accepted records and hosted verification green on run `37376165523`. Nothing in this repository can satisfy the remaining dependency.

When an app-side runtime milestone closes upstream, M006 must perform a fresh interface review against the closed contract rather than assuming either the current draft shape or this 2026-10-06 reading. The seam it must implement above is frozen in `docs/architecture/transport-boundary.md`, including the rule that Plan 355 grants one logical service stream per operation rather than a pre-connected arbitrary destination byte stream, and the execution-model stop condition if the closed SDK is async-only.

- Registry: https://github.com/dbowm91/i2pr/blob/main/plans/registry.md
- Managed app runtime roadmap: https://github.com/dbowm91/i2pr/blob/main/plans/subsystems/managed-native-app-runtime-roadmap.md
- v1 contract: https://github.com/dbowm91/i2pr/blob/main/specs/references/managed-native-app-runtime-v1.md
- Project readiness: https://github.com/dbowm91/i2pr
