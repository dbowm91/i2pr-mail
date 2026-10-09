# Milestone 012 status — managed-app v1 application client and multiplexer

Status: closed

Date: 2026-10-09

Implementation plan: `plans/implementation/mail-backend-foundation/012-managed-app-v1-client-and-multiplexer.md`

Class: corrective integration infrastructure + invariant

## Summary

M012 implements the application side of i2pr managed-app v1 as a bounded,
router-independent Rust client/multiplexer above an injected duplex byte
stream. It corrects the stale M006 assumption that no application
runtime/channel exists: upstream Plans 368–371 and 382–383 now launch a real
application through `i2pr-daemon → manager channel → i2pr-appd →
i2pr-apphost → application stdin/stdout (managed-app v1)`, and this milestone
supplies the language-neutral wire client for that contract without touching
private `i2pr-daemon` Rust types.

M012 deliberately stops below SAM. It produces an async logical `sam` service
stream that M013 will hand to the canonical `dbowm91/i2pr-sam` client once that
library's injected-connection provider (M018) closes. The temporary
`i2pr-mail-sam` implementation from M010 is intentionally retained during this
milestone; M013 owns its retirement after parity proof.

No live router, SAM implementation, OS sandbox, package administration, GUI,
clearnet, I2CP, or Proposal 170 work was performed or claimed.

## Implementation

New adapter-layer crate `i2pr-mail-managed-app` (`crates/mail-managed-app/`):

- `launch.rs` — trusted `ManagedLaunchIdentity { app_id, instance_id }`
  parser for exactly `--i2pr-app-id=<bounded AppId>` and
  `--i2pr-app-instance=<canonical decimal nonzero u128>`, each exactly once,
  with the reserved arguments stripped from the remaining application view.
- `handshake.rs` — 8-byte application-role handshake (`I2PA`, major 1, minor
  0, role Application 1, zero reserved) with exact golden bytes and per-field
  rejection.
- `frame.rs` — 12-byte envelope (version 1; kind 1 control / 2 data; zero
  flags; stream id 0 for control / nonzero for data; 32-bit declared length),
  payload `<= 65_536`, control JSON `<= 16_384`, plus the named ceilings
  `MAX_PENDING_REQUESTS = 64`, `MAX_LIVE_STREAMS = 128`,
  `PER_STREAM_MAX_CHUNKS = 32`, `PER_STREAM_MAX_QUEUED_BYTES = 262_144`,
  `CHANNEL_MAX_QUEUED_BYTES = 1_048_576`, `WRITER_QUEUE_CAP = 64`.
- `control.rs` — strict JSON vocabulary with raw-payload duplicate-key
  rejection before any semantic value, unknown-field rejection, and direction
  enforcement. App→host: `hello`/`open`/`close`/`reset`. Host→app: `reply`/
  `capabilities`/`stream_closed`/`stream_reset`/`health`.
  `permission_request` is absent by design; any unknown `type` fails closed.
- `client.rs` — `ManagedAppClient::connect` (handshake, one hello with the
  trusted identity, correlated successful reply, exactly one `capabilities`
  event frozen as read-only effective authority), single-owner bounded
  multiplexer (one reader task parsing all inbound frames, one serialized
  bounded writer queue, monotonic nonzero request/stream ids with
  deterministic exhaustion instead of wraparound), `open_service(AppService::Sam)`
  with the effective-grant check before any id allocation or emitted byte,
  and `ManagedLogicalStream` (`AsyncRead + AsyncWrite + Unpin + Send`) with
  close-at-most-once Drop that never blocks. All `select!` races between a
  definitive completion (reply) and shutdown are `biased` toward the
  definitive outcome; shutdown races are biased toward shutdown.
- `tests/managed_app.rs` (16 tests), `tests/adversarial.rs` (9 tests), and 26
  unit tests.

Modified:

- `Cargo.toml` — workspace member + `serde_json`/`tokio` (io-util, sync, rt,
  time, macros) workspace dependencies.
- `Cargo.lock` — pins `tokio 1.53.2`, `serde_json 1.0.151` and transitive
  dependencies.
- `scripts/check-boundaries.sh` — fourth guard for adapter authority and
  seam isolation (see below).
- `docs/architecture/i2pr-integration.md` — M012 supersession note marking
  the 2026-10-06 "no app runtime" rows historical; current runtime/channel
  truth with the M012/M013 split.
- `docs/architecture/transport-boundary.md` — M012 logical-stream seam note;
  `MailTransport`/`ByteStream` unchanged per ADR-0002.
- `plans/registry.md`, `plans/subsystems/mail-backend-foundation-roadmap.md`
  — M012 closed; M013/M006 blockers unchanged (see unblock audit).
- `plans/implementation/mail-backend-foundation/012-managed-app-v1-client-and-multiplexer.md`
  — status to closed pointing here.

Not modified: `mail-domain`, `mail-mime`, `mail-proto`, `mail-store`,
`mail-runtime`, `i2pr-mail-sam` production code; closure records 001–011;
M006/M013 plans (M006 already describes the M012/M013 split; no text change
was required beyond this audit).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Launch context from exact trusted reserved args | `launch::tests::{exact_identity_parses_and_strips_reserved,missing_identity_fails,duplicate_identity_fails,malformed_app_ids_fail,malformed_instance_decimals_fail}` | Each arg exactly once; malformed/duplicate/missing fail before handshake; reserved args stripped; no env/exe identity |
| AppId grammar bounded | `validate_app_id`: 1–128 bytes, alnum start, `[A-Za-z0-9._-/]`, no `..`/`//`/whitespace | Strict, tested at/beyond ceiling |
| Instance id canonical nonzero u128 | `validate_instance_decimal`: digits only, no sign/space/leading-zero/exponent/fraction, `1..=u128::MAX` | `"0"`/`"01"`/`"+42"`/`>u128::MAX` rejected; `u128::MAX` accepted |
| Handshake exact bytes | `handshake::tests::golden_bytes_are_exact` = `[49 32 50 41 01 00 01 00]` | Magic `I2PA`, 1.0, role 1, zero reserved |
| Handshake wrong values rejected | `wrong_magic/version/role/reserved/length` tests | Each field independently fatal |
| Frame exact/max/max+1/directional | `frame::tests::{golden_control/data,max_plus_one,directional,malformed}` | 12-byte layout; 65_536 frame / 16_384 control ceilings; control≠0 / data=0 rejected; bad version/kind/flags rejected |
| Duplicate/unknown control JSON | `control::tests::{duplicate_keys,unknown_fields,unknown_types}` + channel `duplicate_control_keys_fatal`, `unknown_control_field_is_fatal`, `unknown_control_type_is_fatal` | Raw duplicate scan before semantics; unknown fields/types fail; channel terminates (fatal) |
| Every directional variant + wrong-direction | `every_required_variant_round_trips`, `wrong_direction_rejected` + channel `wrong_direction_control_is_fatal` | All 4 app + 5 host shapes round-trip; cross-direction rejected; channel fatal |
| Hello success + correlation/identity failures | `hello_and_capabilities_establish_and_freeze`, `wrong_hello_reply_fails_closed`, `hello_reply_failure_is_hello_failed`, `missing_capabilities_event_fails` | Wrong id / `ok:false` / wrong event fail as `HelloFailed`/`CapabilitiesFailed`; no authority used |
| Capabilities exactly once | `duplicate_capabilities_fatal_closes_channel` | Second event terminates generation; new opens get `ChannelClosed` |
| Open without grant before allocation | `open_without_grant_denied_before_id_allocation` | `Denied`; host observes zero frames in 150 ms |
| Request ledger ceiling/max+1 | `request_ledger_saturates_at_64` | 64 pending held; 65th = `RequestSaturated` |
| Stream ledger ceiling/max+1 | `stream_ledger_saturates_at_128` | 128 live; 129th = `StreamSaturated` |
| Id exhaustion without wraparound | `id_tests::{monotonic,at_u64_max}` | `1..=3` then `IdExhausted`; `u64::MAX-1..=MAX` then exhausted, never reused |
| Stale/duplicate reply fatal | `stale_reply_id_is_fatal` | Unknown id 999 terminates; opens fail `ChannelClosed` |
| Data-before-open/foreign fatal | `data_before_open_is_fatal`, `error_display_carries_no_data_sentinel` driver | Data for unknown id terminates |
| Close vs reset semantics | `close_produces_eof_and_reset_produces_reset_error` (with data sync) | Close → `read` returns 0 (EOF); reset → `ConnectionReset` |
| Sibling isolation | `sibling_isolation_close_one_keeps_other` | First stream EOF while second echoes; no cross-talk |
| Fragmented/coalesced traffic | `fragmented_and_coalesced_traffic_survives` | 1-byte fragmented establishment + coalesced control/data frames survive via `read_exact` sequencing |
| Oversize header fatal | `oversize_frame_header_is_fatal` | Declared `MAX+1` terminates |
| Truncated payload terminates | `truncated_payload_terminates_channel` | Declared 100, sent 10 + EOF → terminal |
| Inbound queue + backpressure | `inbound_backpressure_delivers_all_without_drop` | 40 frames through the 32-chunk queue, ordered, none dropped |
| Reader failure / channel EOF | `channel_eof_terminates_and_wakes_open` | Host drop → `is_closed`; new opens `ChannelClosed` |
| Cancellation at open with id release | `open_cancellation_releases_ids_without_reuse` | Timeout-cancelled open emits best-effort close for the reserved sid; next open uses strictly greater request/stream ids |
| Soak returns to baseline | `soak_open_close_returns_to_baseline` | 50 open/write/echo/close cycles, monotonic ids, channel healthy |
| No secret in diagnostics | `error_display_carries_no_data_sentinel`, control/launch sentinel tests | `Display`/`Debug` of every error type excludes the sentinel payload |
| Guard negative controls | boundary §4 (see below) | Injected dep/authority violations rejected; restored green |

## Verification

Local floor on the closure branch (deterministic, no router/network/secret):

```text
cargo fmt --all -- --check                                                    passed
cargo check --locked --workspace --all-targets                              passed
cargo test --locked --workspace --all-targets                              passed (160 tests)
cargo clippy --locked --workspace --all-targets -- -D warnings             passed (0 warnings)
bash scripts/check-boundaries.sh                                             passed (all four checks)
bash scripts/verify.sh quick                                                 passed
```

Test distribution: domain 6, managed-app 26 unit + 16 integration + 9
adversarial (= 51 new), mime 8, proto 11, runtime 19, sam 44 unit + 8
adversarial, store 13 — total 160 (foundation was 109 through M011).

Focused:

```text
cargo test --locked -p i2pr-mail-managed-app
  26 unit + 9 adversarial + 16 integration, all passing
cargo tree --workspace -e normal
  i2pr-mail-managed-app depends only on serde/serde_json/thiserror/tokio;
  no mail crate depends on it; mail-sam still has zero dependencies
```

Hosted verification (workflow `CI`, lane `bash scripts/verify.sh quick` on
`ubuntu-latest`, toolchain pinned by `rust-toolchain.toml`):

| Role | Run | Head SHA | Result |
|---|---|---|---|
| Implementation head | `37967058720` | `82c2fe5c4ab08cc9fcbd7027b82d9ff58b933441` | **success** |
| Closure/planning head | `37967359475` | `d5e5d0915acbb0deadcdb5bac96b7e2d16409f3c` | **success** |

Both runs execute the same repository-owned deterministic floor; neither
needs a router, I2P network, Postman account, live SAM peer, or secret. Run
`37967058720` proves the implementation head; run `37967359475` proves the
closure head (same tree as the implementation plus this record and the
registry/roadmap/plan-status updates). The run ids were filled by this
follow-up planning commit, which changes no production code, test, schema,
API, dependency, or guard. This follow-up head has its own green `CI` run,
verifiable via `gh run list --branch plans/m012-managed-app-adapter-prep`;
the file intentionally does not cite that third run id to avoid a
self-referential bootstrap (the same disclosure pattern as M011 finding 1).

## Guard self-proof

The fourth boundary check was verified to reject, not merely pass, then
restored green:

| Injected violation | Guard response |
|---|---|
| `[dependencies.mutation]` in `mail-managed-app/Cargo.toml` | `i2pr-mail-managed-app has unexpected dependencies ['mutation']` |
| `// MUTATION std::net` in `mail-managed-app/src/lib.rs` | `... direct authority references ['std::net']` (exit 1) |
| `// MUTATION unbounded_channel` in `mail-managed-app/src/lib.rs` | `... direct authority references ['unbounded_channel']` (exit 1) |

A misplaced `i2pr-mail-managed-app` entry under mail-runtime
`[dev-dependencies]` is (consistently with the pre-existing SAM guard) not a
`[dependencies]` violation; direct `[dependencies]` coupling is rejected by
both the first check (exact project-dep sets) and the fourth check (lower
crates must not depend on the adapter). All mutations restored; the guard is
green again.

## Independent adversarial review

Control and frame codecs were written against the frozen contract, then
probed from outside via `tests/adversarial.rs` and hostile channel tests.
Two product defects were found by evidence and fixed before closure (no
separate corrective plan needed; both are within-milestone corrections
recorded here rather than hidden):

1. **Unbiased `select!` race (fixed).** `open_service` raced a definitive
   reply against shutdown with an unbiased `select!`; when the host's reply
   and a racing terminate (EOF after an immediate close) were both ready,
   the open could report `ChannelClosed` instead of using the reply. All
   completion-vs-shutdown races are now `biased`: replies first (definitive
   outcome wins), shutdown first for sends/waits (fail fast on terminal).
   The pending ledger now carries `Result<(), ManagedError>` so a drained
   (terminated) open reports `ChannelClosed` while a host-denied open
   reports `Denied` without ambiguity.
2. **Close-confirmation strictness (fixed).** The reader initially treated a
   host `stream_closed`/`stream_reset` for an unknown stream as fatal. After
   a local close releases the entry, the host confirmation arrives for an
   unknown id; killing healthy siblings for that benign race is wrong. Unknown
   close/reset confirmations are now ignored; unknown *data* remains fatal,
   and stale/duplicate *replies* remain fatal. Sibling isolation and
   data-before-open tests pin both behaviors.

No codec behavior was changed to accommodate a probe; the probes assert the
documented fail-closed invariants.

## Security review

- **Authority.** The crate opens nothing: handshake/hello/frames move over
  injected async I/O only. `std::net`, `tokio::net/fs/process`, `std::fs`,
  `std::process`, `std::env`, `std::os`, `std::time`, `TcpStream`,
  `UdpSocket`, DNS lookup, and `unbounded_channel` are all rejected by the
  guard in `src/` (tests use `tokio::time` timeouts only as harness clocks;
  production code owns no clock). Requested capabilities are never authority;
  only the frozen host-issued set is consulted, and `sam` open is refused
  before id allocation without a grant.
- **Bounds.** Every frame, control payload, request id, stream id, queue,
  and pending operation is bounded by the v1 contract or a stricter named
  local ceiling. Nothing is truncated; over-ceiling input is refused or fails
  the channel closed. No `unbounded_channel`, `std::sync::mpsc::channel`, or
  hidden VecDeque growth path is used.
- **Fail-closed.** Unknown, duplicate, stale, wrong-direction, oversized,
  truncated, and uncorrelated inputs terminate the generation (fatal) or the
  operation (typed error). One stream close/reset never destroys siblings.
  EOF/fatal wakes every waiter and drops every stream sender.
- **Secrets.** No error or debug output carries data payload bytes,
  credentials, Destinations, or authority values beyond bounded ids and
  static reasons. Sentinel tests cover `Display` and `Debug` for launch,
  control, frame, and client errors.
- **Ownership.** Exactly one reader task and one serialized writer own the
  shared channel; logical streams exchange only routed opaque chunks. Tokio
  stays in this adapter crate; lower mail crates gain no async, managed-app,
  or SAM dependency (guard-enforced both directions).

## Failure, cancellation, restart, and contention

- **Fatal channel errors** (bad handshake use, malformed/oversized frame,
  duplicate/unknown/wrong-direction control, stale/duplicate reply,
  duplicate capabilities, foreign data, EOF): stop new opens, wake every
  pending waiter with `ChannelClosed`, terminate every stream, drop queues,
  abort tasks. Never reconnects; a new i2pr launch is a new generation.
- **Logical stream failure:** host close → EOF after drain; host reset →
  `ConnectionReset`; local write after terminal → typed failure with no new
  frame. Benign close/reset confirmations for locally-released ids are
  ignored.
- **Cancellation:** dropping an `open_service` future before success removes
  its pending/stream entries and best-effort closes the reserved sid (proven
  by the cancellation test's monotonic-id assertions). `shutdown()`/client
  `Drop` terminates the generation and aborts both tasks. Capacity waits
  (`writer` 64, per-stream 32, aggregate 1 MiB) block and wake on
  `capacity_notify`; they never grow to absorb a stalled peer.
- **Restart:** channel state is per-generation and in-memory only; a new
  `connect` starts at hello id 1, stream id 1, empty ledgers. No mailbox,
  schema, or durable state is touched, so no migration exists.
- **Contention:** `std` mutexes guard only brief ledger mutations, never
  across an await; the writer serializes all outbound bytes; the reader is
  the sole inbound parser. Flaky-parallel analysis during development found
  and fixed the unbiased-select race above; the suite is green repeatedly
  under full-workspace parallel runs.

## Compatibility and migration

New crate only. No existing public API changed: `MailTransport`,
`ByteStream`, `OperationControl`, `TransportError`, `BackendService`, domain
types, MIME, protocol machines, store schemas, and the `i2pr-mail-sam` API
are untouched. No persistence, so no schema version and no migration. The
`i2pr-mail-sam` crate is retained as transitional scaffolding; M013 removes
it only after transcript/API parity against canonical `i2pr-sam` M018.

## Unresolved findings

No high or medium finding.

Low / informational, carried forward:

1. **Integration tests run on a multi-thread runtime.** Each `#[tokio::test]`
   uses `flavor = "multi_thread"`. Early development observed a
   current-thread stall in the open/echo/close path; the unbiased-select race
   above was the evidenced defect and is fixed, but current-thread
   qualification was not re-proven exhaustively. M013 chooses the adapter
   runtime flavor by evidence; it must record its choice and rationale rather
   than assuming either flavor.
2. **Upstream contract is plan-frozen, not live re-audited.** This milestone
   implements the language-neutral v1 contract cited by its plan (upstream
   `main` `acd752b7`, Plans 368–371/382–383). No live router, apphost,
   catalog, or SAM server was contacted; M006 retains live qualification
   against closed SAM/368 and managed-app/407.
3. **Writer/reader tasks are aborted on drop.** `shutdown()`/`Drop` aborts
   both JoinHandles after terminating the generation. Abort is the documented
   teardown (bounded, no new generation), not a leak: queues are dropped,
   waiters woken, ids released.
4. **Runner notices (informational).** Hosted runs emit GitHub's Node 20
   deprecation notice for `actions/checkout@v4`/`actions/cache@v4`; behavior
   is unaffected.

## Unblock audit

Against `plans/registry.md` and the foundation roadmap dependency graph:

- **M012 is closed** with no remaining local work. Its implementation plan
  status is updated to closed pointing here.
- **M013 remains blocked** — correctly. Its two hard dependencies are now
  half-satisfied: local M012 closure is done, but canonical
  `dbowm91/i2pr-sam` M018 closure/mainline integration is still outstanding
  (M018 itself behind i2pr-sam M017). No interface in M012 changed that
  assessment, and M013 must re-read the exact M018 provider API at execution
  start rather than assuming this reading. M013 is *not* moved to ready in
  this update.
- **M006 remains blocked** on M013 closure plus upstream i2pr SAM/368
  (port-aware SAM 3.3 `FROM_PORT`/`TO_PORT`) and managed-app/407 (qualified
  Linux `Secured` backend). Its plan already describes the M012/M013 split,
  so no M006 text change was required; this audit records that the local gate
  is now exactly M013.
- **No other plan becomes eligible.** M001–M005 and M007–M011 stay closed;
  no dependency-ready plan besides the (unchanged, still blocked) M013/M006
  entries exists. Per the registration rule, no downstream status moves
  except M012 → closed.

## Reconciliation performed

New:

- `plans/closure/mail-backend-foundation/012-status.md` (this record)
- `crates/mail-managed-app/Cargo.toml`
- `crates/mail-managed-app/src/{lib,launch,handshake,frame,control,client}.rs`
- `crates/mail-managed-app/tests/{managed_app,adversarial}.rs`

Modified:

- `Cargo.toml` / `Cargo.lock`
- `scripts/check-boundaries.sh`
- `docs/architecture/i2pr-integration.md`
- `docs/architecture/transport-boundary.md`
- `plans/registry.md`
- `plans/subsystems/mail-backend-foundation-roadmap.md`
- `plans/implementation/mail-backend-foundation/012-managed-app-v1-client-and-multiplexer.md`
  (status to closed)

Not modified: closure records 001–011, implementation plans 001–011/013
(except the M012 status line above), M006/M013 plans, ADRs, `docs/` beyond
the two noted files, and all pre-existing production crates.

## Files changed (closure update)

See reconciliation above. Product change is confined to the new
`crates/mail-managed-app` crate plus workspace manifest/lockfile; guards and
the two architecture docs carry the invariant updates; planning updates are
registry, roadmap, the M012 plan status, and this record.
