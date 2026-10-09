# i2pr integration contract

This document records milestone M006's §7A deliverable: the downstream contract and
authority analysis for the i2pr managed-app adapter. It describes the contract only.
It does not implement `MailTransport`, and it does not claim the adapter is unblocked.

The adapter itself remains blocked. See §4.

> M012 supersession (2026-10-09): the §1 rows stating "no app-side runtime or
> channel" and "no reachable gateway" describe the 2026-10-06 upstream state
> (Plans 354/355 router-side only) and are superseded forward by upstream
> managed-app Plans 368–371 (daemon↔manager bridge, `i2pr-appd`,
> `i2pr-apphost`, v1 application consumer, startup) and Plans 382–383
> (signed packages, trust/grants, catalog, autostart). The application-facing
> managed-app v1 wire contract is now concrete and is implemented locally by
> M012 (`i2pr-mail-managed-app`: trusted launch identity, handshake, bounded
> frames, hello/capabilities, multiplexed logical `sam` streams over injected
> async I/O). M013 composes that client with canonical `i2pr-sam` M018 into
> synchronous `MailTransport`; M006 remains gated on M013 plus upstream SAM/368
> (port-aware 3.3) and managed-app/407 (qualified Linux `Secured`). The table
> below is preserved as historical evidence and must not be read as current
> reachability guidance. See `plans/closure/mail-backend-foundation/012-status.md`.

## 1. Status summary

| Question | Answer | Evidence |
|---|---|---|
| Is the router-side gateway closed? | Yes | i2pr Plans 354 and 355 both closed on `main` |
| Is it reachable from outside `i2pr-daemon`? | No | every gateway type is `pub(crate)` |
| Is there an app-side runtime or channel? | No | no `i2pr-app-runtime` crate; IPC deliberately undecided |
| Is a SAM client available to consume? | No | upstream assigns it to a separate repository |
| Can mail reach the network another way? | Yes, but it is forbidden | loopback SAM on `127.0.0.1:7656` |

## 2. Pinned upstream contract

All citations are read from `dbowm91/i2pr` at `main` = `2f82c799`.

| Item | Value |
|---|---|
| Repository | `dbowm91/i2pr` |
| Ref read | `origin/main` `2f82c799` |
| Plan 354 closure commit | `6cd35bfe` — `passed-managed-app-private-client-transport-seams` |
| Plan 354 closure record | `plans/closure/managed-native-app-runtime/354-status.md` |
| Plan 355 closure commit | `2b96f1bc` — `passed-managed-app-principal-gateway-private-client-seams` |
| Plan 355 closure record | `plans/closure/managed-native-app-runtime/355-status.md` |
| Service-stream contract freeze | `d2f17f38` |
| Managed-app wire contract | `specs/references/managed-native-app-runtime-v1.md` |
| SAM adapter handoff | `specs/references/portable-service-tunnel-sam-adapter-handoff.md` |

Both closure commits are ancestors of `origin/main`. No tag or published crate version
exists: **every crate under `crates/` is `publish = false`**, so any future consumption
of an i2pr type is a git-commit pin, not a semver dependency. That satisfies M006 §9's
"pin and declare the minimum compatible version" requirement in form, but it means an
upstream refactor is a breaking change with no deprecation window.

## 3. Interface matrix

The frozen mapping, quoted from the v1 contract §3 "Router service stream mapping":

> For a successful `open` of `sam` or `i2cp`, the trusted host runtime binds that nonzero
> logical stream id to exactly one router-owned protocol connection. Each data-frame
> payload is passed to that SAM or I2CP connection as the exact protocol octets, in order;
> the gateway does not add an encoding, rewrite protocol fields, or inspect application
> protocol content.

Mapping `MailService` onto that surface:

| `MailService` | SAM intent | Destination | Port handling |
|---|---|---|---|
| `Pop3` | `open` with `service = sam`, then SAM `STREAM CONNECT` | `pop.postman.i2p` resolved via `NAMING LOOKUP` | SAM 3.1 has no `HOSTNAME=`/`PORT=` on `STREAM CONNECT` (a 3.2 feature), so the destination is resolved first and the port rides inside the SAM session |
| `Smtp` | `open` with `service = sam`, then SAM `STREAM CONNECT` | `smtp.postman.i2p` resolved via `NAMING LOOKUP` | as above |

Consequences that constrain the adapter:

- **One stream per operation.** The gateway grants a logical stream, not a pre-connected
  destination socket. `MailTransport::open` must request a stream per operation and may
  not assume a persistent tunnel is already open.
- **Bytes are opaque.** The gateway does not parse POP3 or SMTP. It moves octets.
- **The codec is ours.** The SAM client half — version negotiation, command/reply codecs,
  state machines, `STREAM CONNECT`, naming lookup, and the "async runtime and blocking
  facade" — is assigned by upstream to *a future separate repository*. i2pr will never
  ship it. Milestone M010 therefore owns it.
- **The gateway is async-only.** `AppGatewaySession::open_sam` (`app_gateway.rs:148`),
  `open_i2cp` (`:194`), `shutdown` (`:261`), and `wait_closed` (`:310`) are all
  `pub(crate) async fn`, and they yield in-process handles over `tokio::io` streams.

## 4. Authority analysis

This is the part M006 cannot currently satisfy.

`AppGatewayAuthorization` is constructed only by `AppGatewayAuthorization::from_trusted_composition`,
which is `pub(crate)`, deliberately non-serializable, and has no constructor from wire
values or requested capabilities. Upstream states this as a security property, not an
oversight: an application's deserialized `hello` and `AppPrincipal` cannot construct
authority.

For mail to obtain transport, something must:

1. launch i2pr-mail as a supervised process,
2. authenticate that process or channel,
3. project administrator grants into an `AppPrincipal` + `EffectiveCapabilities`,
4. construct `AppGatewayAuthorization` from that trusted composition, and
5. hand i2pr-mail an app channel carrying framed logical streams.

**None of these exists upstream.** The v1 contract states that `hello` "is not
authentication proof" and that "a future trusted transport owner must bind that claim to
its authenticated process/IPC principal before authorizing access". ADR 0032 assigns the
AppManager to a user-space component outside the router and explicitly declines to decide
a "process protocol adapter". The roadmap records the next milestone as "package/lifecycle
+ AppManager administrative owner (eligible for a future plan)" and the registry confirms
"none is registered yet".

So the blocker is **reachability**, not design. Plan 355 closed the router half and
deliberately left the app half to a milestone that does not exist. `AppGatewaySession`
carries `#![allow(dead_code)]` with the comment "No production app-runtime caller exists
yet; this infrastructure API is exercised by its module tests until a separately planned
consumer lands" — that comment is still literally true on `main`.

## 5. The path that exists and is not taken

i2pr runs a full SAM 3.1 server on `127.0.0.1:7656` (`crates/i2pr-daemon/src/sam.rs`),
disabled by default (`default_sam_enabled() -> false`), and `SamConfig` has **no password
or credential field** (`config.rs:1529-1537`) — only `enabled`, `bind_address`, `port`,
and `limits`. A local process can therefore `HELLO VERSION`, `NAMING LOOKUP`, and
`STREAM CONNECT` to Postman today, with no credential and no router change.

i2pr-mail does not use it. `ADR-0001`, `docs/architecture/transport-boundary.md`, and the
repository architectural constraints all forbid a loopback fallback: a loopback listener
proxying to the router "would reintroduce a second, unmanaged authority and a clearnet-
visible surface". Revisiting that is an ADR supersession decision, not a milestone, and
it is explicitly out of scope here.

## 6. SAM 3.1 wire forms this client must speak

Verified against the SAM v3 specification (<https://i2p.net/en/docs/api/samv3/>) and
against upstream's implementation.

| Direction | Line |
|---|---|
| request | `HELLO VERSION MIN=3.1 MAX=3.1` |
| reply | `HELLO REPLY RESULT=OK VERSION=3.1` |
| request | `NAMING LOOKUP NAME=<name>` |
| reply | `NAMING REPLY RESULT=OK NAME=<name> VALUE=<base64>` |
| request | `SESSION CREATE STYLE=STREAM ID=<id> DESTINATION=<destination>` |
| reply | `SESSION STATUS RESULT=OK DESTINATION=<destination>` |
| request | `STREAM CONNECT ID=<id> DESTINATION=<base64>` |
| reply | `SAM <session>:<stream> OK` **or** `STREAM STATUS RESULT=OK` |

The last row is a real divergence, not a transcription error. The SAM v3 specification
documents `SAM $session:$stream OK`; upstream i2pr encodes the stream reply as
`STREAM STATUS RESULT=OK` (`crates/i2pr-api/src/sam/reply.rs`, asserted in
`crates/i2pr-daemon/tests/sam_loopback.rs`). `i2pr-mail-sam` accepts both and normalizes
them to one typed outcome, because the mail client must interoperate with whichever router
answers it and must not silently accept a malformed line to do so.

Base64 uses the I2P alphabet `A-Z a-z 0-9 - ~` with `=` padding, not standard base64.

## 7. What M010 settles and what it does not

Settled by M010:

- the client-side SAM 3.1 codec, as a deterministic sans-I/O state machine with no
  dependencies and no sockets;
- the execution model, by `ADR-0002`: the synchronous seam stays, and async is confined
  to the adapter crate behind a bounded bridge;
- the exact upstream contract this adapter will target, recorded above.

Not settled, and still blocking M006:

- the app-side trusted runtime and process channel (upstream, unregistered);
- a publicly reachable gateway API (upstream, `pub(crate)` today);
- the adapter crate itself, and its bounded async bridge;
- the live POP3/SMTP smoke transcript and the negative no-fallback evidence.

M006 remains blocked. See `plans/implementation/mail-backend-foundation/006-i2pr-managed-app-transport-adapter.md`.