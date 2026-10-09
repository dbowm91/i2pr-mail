# Mail Backend Foundation Milestone 006 — i2pr Managed-App Transport Adapter

Status: blocked

Repository baseline: original planning baseline ec8056a75ccba628994aa7610ac5a07cd3d4a986; corrected after M011–M013 planning re-audit. Execute only after M013 closure plus upstream i2pr SAM/368 and Managed native app runtime/386 closure.

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
- post-M005 corrective M007-M009 closed;
- M010 SAM client foundation closed;
- M011 foundation integration closed;
- M012 managed-app v1 client/multiplexer must close;
- M013 canonical SAM adoption + MailTransport composition must close.

Interface dependencies in dbowm91/i2pr:

- managed-app Plans 345, 349, 352–355, 368–371 and 382–383 are closed on current upstream main `acd752b7`;
- Plans 368/369 provide the inherited daemon↔manager bridge, `i2pr-appd`, `i2pr-apphost`, the application-facing managed-app v1 stdin/stdout channel, and real private SAM/I2CP logical streams;
- Plans 382/383 provide signed immutable packages, persistent publisher trust/grants/exact selection, offline administration, and the production launch catalog;
- upstream SAM/368 must close SAM 3.3 including inherited 3.2+ `FROM_PORT`/`TO_PORT` STREAM behavior; reference Postman services use nonzero I2P ports 110 and 25, so current SAM 3.1 is insufficient;
- upstream Managed native app runtime/386 must close a qualified `Secured` launch backend; Plan 386 already names Plan 385 private app data/workspace as its hard predecessor. Current `UnsafeDirect` is ordinary host networking and cannot satisfy this plan's no-direct-network acceptance boundary.

Proposal 170 administrative completion is not a blanket prerequisite unless the final app gateway specifically and narrowly requires it for an app-owned resource. A general administrator credential is forbidden.

## 3. Current implementation evidence

The Plan-355-only description is now stale. Upstream has closed the missing application-side owners. Plan 369's production path is `i2pr-daemon -> inherited manager channel -> i2pr-appd -> i2pr-apphost -> application managed-app v1 stdin/stdout`; Plan 383 adds persistent trusted package/grant/catalog authority and restart-safe production launches. Black-box qualification already carries SAM and I2CP bytes through the private gateway without a loopback listener.

M006 therefore must not consume the internal `pub(crate)` `AppGatewaySession` API or add an i2pr Rust dependency. Its downstream contract is the language-neutral managed-app v1 application channel. M012 owns that client/multiplexer. M013 then consumes the canonical `dbowm91/i2pr-sam` injected-provider API, composes the synchronous MailTransport, and retires the temporary mail-local SAM implementation.

The remaining upstream limitations are capability limitations, not reachability limitations. Current `Secured` launches fail before exec because no qualified OS sandbox exists, and current i2pr advertises SAM 3.1 only. Reference Postman client tunnels target `pop.postman.i2p:110` and `smtp.postman.i2p:25`; SAM 3.1 cannot express those nonzero destination ports. Upstream Managed app/386 owns secured Linux containment, and SAM/368 owns SAM 3.3 including the inherited port semantics.

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

- consume the closed M013 production adapter stack (M012 managed-app client + canonical i2pr-sam);
- application package/manifest capability declarations required for mail;
- production composition of MailTransport for logical POP3 and SMTP I2P services;
- port-aware SAM session establishment through the authorized app channel;
- map Plan 383's trusted launch identity plus Plan 385's app data/cache/runtime paths into the adapter composition executed under Plan 386 `Secured` containment;
- lifecycle/cancellation and host health/diagnostic mapping;
- real/local integration smoke using the qualified Secured path and real private SAM stream support.

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

At execution start, inspect the exact closed upstream i2pr SAM/368 and Managed app/386 contracts plus the M013 closure and its pinned i2pr-sam revision. Freeze those exact commits and confirm the application-facing managed-app v1 framing and injected-SAM provider contract have not drifted.

Do not adapt private router types or reimplement SAM. M006 packages/launches the already-closed local adapter stack through the secured runtime and qualifies it against the real port-aware SAM server.

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

### B — Secured production composition

Bind the M013 adapter stack to the managed-app process bootstrap under Plan 386, consuming the trusted app/instance identity already supplied by the runtime and the Plan-385 private data/cache/runtime paths. Do not change lower mail APIs.

Acceptance evidence: the mail process starts under `Secured`, completes managed-app hello, observes its effective `sam` grant, and opens no host socket.

### C — Port-aware integration smoke

Run against supported i2pr local/test infrastructure with SAM/368 enabled. Establish the retained SAM session-control connection and separate POP3/SMTP STREAM connections using `TO_PORT=110` and `TO_PORT=25`.

Acceptance evidence: real POP3 and SMTP protocol bytes cross the private managed-app path; no 7656/7659/7660 listener is enabled or contacted.

### D — Negative authority qualification

Prove secured operation has no direct/public/LAN/loopback fallback, a missing/denied `sam` grant fails before SAM allocation, and a SAM peer without the required port-capable version fails rather than silently connecting to port 0.

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

- M007-M013 are not closed;
- upstream SAM/368 is not closed with port-aware managed-app STREAM evidence;
- upstream Managed native app runtime/386 is not closed with a qualified `Secured` profile;
- the application-facing managed-app v1 contract has drifted incompatibly from M012, or the canonical SAM/provider contract has drifted incompatibly from M013;
- integration requires the application to construct router-internal authority rather than receiving scoped capability from the trusted runtime;
- integration requires `UnsafeDirect` or localhost socket fallback;
- integration requires handing the app a general Proposal 170 administrator credential;
- the only available surface is the intentionally reserved/underspecified generic brokered_tcp operation;
- upstream API churn would require leaking i2pr internals into lower mail crates.

## 15. Closure evidence required

Exact upstream commit/tag, app capability matrix, adapter mapping, POP3/SMTP integration transcript, negative no-fallback evidence, restart/cancellation behavior, verification commands/results, security review, compatibility declaration, and roadmap completion audit.

## 16. Handoff notes

This plan remains blocked, but for materially different reasons than its original registration. Do not use the historical Plan-355-only reachability analysis as current implementation guidance. M012 is the immediate local progress path; M013 follows after canonical i2pr-sam M018. Once M013 closes, upstream i2pr SAM/368 and Managed app/386 are the only remaining capability gates.

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

The local corrective lane is complete: M007, M008, and M009 are closed with accepted records, and M010 has also closed the SAM client/contract decision work. M011 is the remaining local integration-hygiene prerequisite: it moves that qualified foundation to main without rewriting closure SHAs. After M011 closes, nothing in this repository can satisfy the remaining upstream runtime dependency.

The 2026-10-06 app-runtime wait is now superseded forward by M012/M013 planning. The app-side runtime exists and is exercised. M012 owns the independent managed-app application wire, while M013 adopts the canonical i2pr-sam provider seam and final local MailTransport composition. M006 must re-read the exact closed i2pr SAM/368 and Managed app/386 interfaces, then consume M013 rather than private router or mail-local SAM internals.

- Registry: https://github.com/dbowm91/i2pr/blob/main/plans/registry.md
- Managed app runtime roadmap: https://github.com/dbowm91/i2pr/blob/main/plans/subsystems/managed-native-app-runtime-roadmap.md
- v1 contract: https://github.com/dbowm91/i2pr/blob/main/specs/references/managed-native-app-runtime-v1.md
- Project readiness: https://github.com/dbowm91/i2pr
