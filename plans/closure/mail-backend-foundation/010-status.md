# Milestone 010 status — SAM 3.1 client codec and downstream contract qualification

Status: closed

Date: 2026-10-06

Implementation plan: `plans/implementation/mail-backend-foundation/010-sam31-client-codec-and-contract-qualification.md`

Class: capability + infrastructure

## Summary

M010 lands the client half of the i2pr adapter as a deterministic, sans-I/O SAM 3.1 codec, and records M006 §7A's downstream contract and authority matrix. It removes M006's codec and execution-model dependencies. It does **not** unblock M006: the transport itself is still gated on an upstream app-side managed-app runtime that does not exist.

The decisive reason this milestone was possible at all: upstream `specs/references/portable-service-tunnel-sam-adapter-handoff.md` assigns SAM *client* implementation — "HELLO/version/capability negotiation; command/reply codecs and state machines; STREAM connect…; async runtime and blocking facade" — to a future separate repository. i2pr will never ship a SAM client, so this work belongs here regardless of how the managed-app runtime resolves.

## Implementation

- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md` — new accepted ADR.
- `crates/mail-sam/` — new crate, package `i2pr-mail-sam`, zero dependencies.
  - `version.rs` — `SamVersion`, strict `parse_version`, fail-closed `negotiate`.
  - `reply.rs` — `SamResult`, `SamReplyKind`, `SamReply`, bounded `parse_reply`, typed reads `expect_hello` / `expect_naming_value` / `expect_destination`.
  - `client.rs` — `SamClient`, `SamClientState`, `SamEvent`, command builders, validators, ordering state machine.
  - `tests/adversarial.rs` — cross-cutting adversarial properties exercised from outside the crate.
- `Cargo.toml` — `crates/mail-sam` appended to `members`.
- `scripts/check-boundaries.sh` — `i2pr-mail-sam` added to the dependency map (empty allowed set) and the sans-I/O source scan; new third check for zero dependencies, sans-I/O source, and no inbound dependency from any crate below the seam.
- `docs/architecture/i2pr-integration.md` — new: the §7A interface and authority matrix.
- `docs/architecture/transport-boundary.md` — execution-model stop condition resolved by ADR-0002; adapter obligations updated.
- `plans/subsystems/mail-backend-foundation-roadmap.md`, `plans/registry.md` — M010 registered.
- `README.md` — boundary-guard scope note.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Codec is sans-I/O with no socket, clock, filesystem, or async | `crates/mail-sam/**`; guard check 3; `cargo tree -p i2pr-mail-sam` | No `std::net`, `std::fs`, `std::process`, `std::time`, `tokio::`, or `async fn` in the crate. Zero dependencies, project or external. |
| Codec depends on no existing crate, so nothing below the seam gains a router-protocol type | guard check 3 reverse assertion over domain/mime/proto/store/runtime | No crate below the seam may depend on `i2pr-mail-sam`; proven to reject when violated. |
| Every outbound command M006 needs is encoded correctly | `command_builders_emit_exact_bytes_with_trailing_newline`; `probe_generated_lines_are_well_formed` | `HELLO VERSION MIN=3.1 MAX=3.1`, `NAMING LOOKUP NAME=…`, `SESSION CREATE STYLE=STREAM ID=… DESTINATION=…`, `STREAM CONNECT ID=… DESTINATION=…`. No doubled spaces, one trailing newline, no interior newline. |
| Bounds are enforced before formatting, never by truncation | `builders_reject_empty_over_length_and_malformed_values`; `maximum_length_values_still_fit_the_line_ceiling`; `probe_line_ceiling_is_exact_and_never_truncates` | `MAX_LINE` 8192, `MAX_TOKEN_COUNT` 64, `MAX_OPTION_COUNT` 32, `MAX_NAME_LEN` 256, `MAX_SESSION_ID_LEN` 256, `MAX_DESTINATION_LEN` 1024. A line at the ceiling parses; one byte over is refused. |
| A caller-supplied value cannot alter command structure | `injection_attempts_cannot_add_a_token_or_split_a_line`; `probe_injection_cannot_alter_command_structure` | Space, `"`, `\`, control bytes and non-ASCII are refused in names, session ids and destinations. `=` alone is accepted because SAM splits on whitespace *then* on the first `=`, so it cannot cross a token boundary — verified against upstream `parser.rs::collect_options`. |
| Destinations use the I2P base64 alphabet only | `destination_alphabet_accepts_i2p_base64_and_rejects_standard_base64`; `probe_destination_alphabet_and_bounds` | `A-Z a-z 0-9 - ~` plus trailing `=` accepted; standard base64 `+` and `/` rejected; padding-only rejected. |
| Every reply M006 will encounter parses into a typed value | per-kind parse tests; `probe_typed_reads_pull_the_destination` | `HELLO REPLY`, `NAMING REPLY`, `SESSION STATUS`, `PONG` parse; quoted values, escaped quotes and `=` inside a value parse. |
| The documented stream-reply divergence is handled explicitly | `both_stream_connect_shapes_normalize_to_the_same_reply`; `sam_shape_requires_a_session_stream_anchor`; `probe_both_stream_shapes_normalize_identically` | `SAM <session>:<stream> OK` and i2pr's `STREAM STATUS RESULT=OK` normalize to one typed value. An unanchored `SAM OK` is refused. |
| Unknown input fails closed | `unknown_result_fails_closed_instead_of_defaulting_to_ok`; `rejects_unknown_shapes_and_missing_results`; `probe_unknown_results_and_shapes_fail_closed` | Unknown result tokens and unknown shapes are refused; no coercion to `Ok`. |
| The client state machine rejects out-of-order use | `session_status_before_hello_is_out_of_order`; `stream_status_before_a_session_is_out_of_order`; `probe_state_machine_rejects_out_of_order_use` | `Unestablished → HelloEstablished → SessionEstablished`; `SESSION CREATE` before hello and `STREAM CONNECT` before session are typed errors. `NAMING LOOKUP` stays legal at any time, as SAM requires. |
| A full transcript is deterministic | `full_deterministic_transcript_from_hello_to_stream` | hello → session → connect asserted on exact bytes. |
| No error type carries a secret | `SamReplyError` variants hold `SamResult` or `&'static str` only | No supplied value is stored in any error variant. |
| The boundary guard proves it rejects violations | three injected-violation controls | See below. |
| M006 §7A is recorded against the closed upstream contract | `docs/architecture/i2pr-integration.md` | Cites `main` `2f82c799`, Plan 354 `6cd35bfe`, Plan 355 `2b96f1bc`, freeze `d2f17f38`, and the two frozen spec files. |
| M006 remains blocked and is not claimed ready | registry, roadmap, `docs/architecture/i2pr-integration.md` §7 | Recorded explicitly in every one of those documents. |

## Verification

Local results on this repository: no router, no I2P network, no Postman account, and no live SAM peer is claimed. Hosted deterministic verification is separately recorded below.

Focused:

```
cargo test --locked -p i2pr-mail-sam
```

52 tests in the new crate (44 unit + 8 integration), all passing. No failures, no ignored tests.

Full floor:

```
cargo fmt --all -- --check                                                    passed
cargo check --locked --workspace --all-targets                              passed
cargo test --locked --workspace --all-targets                              passed (109 tests)
cargo clippy --locked --workspace --all-targets -- -D warnings             passed (0 warnings)
bash scripts/check-boundaries.sh                                             passed (all three checks)
bash scripts/verify.sh quick                                                 passed
```

Test distribution after M010: domain 6, mime 8, proto 11, runtime 19, sam 44 + 8, store 13.

Hosted verification, run `37488448169` on `3a997ea`, workflow `CI`, status **success**, 31s. This run executes the same repository-owned deterministic floor on `ubuntu-latest`; it requires no network, no router, and no secret. No hosted run has exercised a live SAM peer, and none is claimed.

## Guard self-proof

The boundary guard was verified to *reject*, not merely to pass, by injecting three violations and restoring after each:

| Injected violation | Guard response |
|---|---|
| `i2pr-mail-sam` given a dependency on `i2pr-mail-domain` | `i2pr-mail-sam: project dependencies ['i2pr-mail-domain'] != []` |
| `i2pr-mail-runtime` given a dependency on `i2pr-mail-sam` | `i2pr-mail-runtime: project dependencies [...] != [...]` |
| `std::net::TcpStream::connect` added to the codec source | `i2pr-mail-sam: forbidden capability references ['std::net']` |

All three restored; the guard is green again.

## Independent adversarial review

The codec implementation was written against a written specification. A separate adversarial pass was then run from outside the crate, in `crates/mail-sam/tests/adversarial.rs`, probing the security argument rather than re-reading the implementation.

That pass initially reported three failures. All three were **faults in the probe, not in the codec**, and each was resolved by checking the disputed assumption against evidence rather than by changing the codec:

1. The probe asserted a name containing `=` must be rejected. It is safe and correct to accept it: upstream `crates/i2pr-api/src/sam/parser.rs` tokenizes on whitespace and then on the first `=`, so `NAME==v` is one token `NAME` with value `=v`. The probe was corrected to assert the actual invariant — no additional token may appear.
2. The probe assumed `MAX_LINE` would be reachable through an over-long name. It is not: `MAX_NAME_LEN` (256) is stricter and fires first. That is correct and better than the probe assumed. The probe was corrected to assert that ordering, and moved the `MAX_LINE` check to the reply side where it is genuinely the operative bound.
3. The probe asserted a `NAMING REPLY` lacking `VALUE` must fail at parse time. It correctly parses; the failure belongs to the typed read `expect_naming_value`, which rejects it. The probe was corrected to assert the failure at the typed read.

Two further probe defects were arithmetic: an over-ceiling buffer built by `pop` then `push` returned to exactly `MAX_LINE`, and a later construction produced `MAX_LINE + 2` while asserting `+ 1`. Both were rebuilt so the over-ceiling line differs from the at-ceiling line **only in length**, so it cannot be refused for an unrelated malformed-quote reason and pass for the wrong cause.

No codec behaviour was changed in response to any of these.

## Security review

- **Injection.** The only bytes that cross a SAM token boundary are space, `"`, `\`, control bytes and non-ASCII. All are refused in caller-supplied values before any line is formatted. A line is built through a `token`/`pair` builder rather than by joining fragments, so a `KEY= VALUE` split is unrepresentable — the first implementation of the worker produced exactly that bug and it was corrected before acceptance.
- **Bounds.** All bounds are checked before formatting. Nothing is truncated. A rejected value never becomes a partial command.
- **Fail-closed.** Unknown result tokens, unknown reply shapes, missing required options, unterminated quotes and over-ceiling lines all produce typed errors. No path coerces malformed input into success. `PONG` is the one normalized case, because SAM 3 defines PING/PONG with no `RESULT` token and exactly one outcome.
- **Secrets.** The codec handles no credentials. No error variant stores a supplied value. Nothing in the crate logs.
- **Authority.** The codec opens nothing. It is handed bytes and returns typed values; it has no transport and no network authority. It is not a dependency of any existing crate, so it cannot become a hidden network path.
- **No fallback.** Nothing in M010 introduces a socket, a loopback path, or a default that could degrade to direct networking. The i2pr loopback SAM listener that *would* work today is documented in `docs/architecture/i2pr-integration.md` §5 as deliberately unused.

## Failure, cancellation, restart, and contention

The codec holds no I/O, so it has no contention of its own. It is a pure function of client state and input bytes. It owns no clock and no retry timing, so it cannot introduce a second timeout or retry clock — deadline and cancellation remain `OperationControl`'s responsibility at the seam, preserving the single-clock invariant from ADR-0002. It keeps no durable state, so a restart begins again at `Unestablished` and no stale session id is reused.

## Compatibility and migration

New crate. No existing public API changed: `MailTransport`, `ByteStream`, `OperationControl`, `TransportError`, `BackendService`, and every durable schema are untouched. No persistence, so no schema version and no migration.

The codec targets SAM 3.1, which upstream i2pr advertises as its only supported version and which the SAM v3 documentation recommends as the minimum. No 3.2 or 3.3 feature is used, so the same codec is usable against Java I2P and i2pd — which matters, because i2pd does not support most 3.2/3.3 features and because every i2pr crate is `publish = false`, making any future consumption a git-commit pin with no deprecation window.

## Unresolved findings

No high or medium finding.

Low / informational, carried forward:

1. **Upstream stream-reply divergence.** i2pr emits `STREAM STATUS RESULT=OK` where the SAM v3 specification documents `SAM <session>:<stream> OK`. Handled explicitly and narrowly, with both shapes tested. If upstream later adopts the specification spelling, the accepted-shape logic narrows and the normalized outcome does not change.
2. **`SamClient::bind_session_id` is an added API.** A SAM session id is client-chosen and never echoed by the router, so the state machine cannot derive it from a reply; `bind_session_id` validates with the same rule and records it. This is a small deviation from the planned API shape and is recorded rather than hidden.
3. **No live SAM peer has spoken to this codec.** Every result is a deterministic transcript or an injected input. A live check requires a router, which requires the blocked adapter path or a manual router, and is deliberately not claimed here. M006 retains that obligation.
4. **`publish = false` upstream.** Recorded in `docs/architecture/i2pr-integration.md` §2 as a compatibility constraint rather than a defect.

## Unblock audit

Against `plans/registry.md`:

- **M006 remains blocked.** The named upstream prerequisite is satisfied — Plans 354 and 355 are closed on `main` `2f82c799` — but the blocker moved to reachability. `AppGatewaySession`, `AppGatewayAuthorization`, `AppGatewayLimits` and `AppGatewayComposition` are all `pub(crate)` in `crates/i2pr-daemon/src/app_gateway.rs` under a module-level `#![allow(dead_code)]` reading "No production app-runtime caller exists yet". No app-side runtime, process channel, or authentication path exists; every i2pr crate is `publish = false`; and upstream's package/lifecycle + AppManager milestone is "eligible for a future plan" and unregistered. M010 does not and cannot change this.
- **No other plan becomes eligible.** M007, M008, and M009 remain closed. The post-M005 corrective addendum remains closed.
- **M010 is closed** with no remaining local work.
- **Next action.** M006 proceeds when upstream registers and closes an app-side managed-app runtime milestone exposing a reachable app channel. At that point M006 must perform a fresh interface review against the closed contract rather than assuming this reading.

## Files changed

New:

- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`
- `plans/implementation/mail-backend-foundation/010-sam31-client-codec-and-contract-qualification.md`
- `crates/mail-sam/Cargo.toml`
- `crates/mail-sam/src/lib.rs`
- `crates/mail-sam/src/version.rs`
- `crates/mail-sam/src/reply.rs`
- `crates/mail-sam/src/client.rs`
- `crates/mail-sam/tests/adversarial.rs`
- `docs/architecture/i2pr-integration.md`
- `plans/closure/mail-backend-foundation/010-status.md`

Modified:

- `Cargo.toml`
- `scripts/check-boundaries.sh`
- `docs/architecture/transport-boundary.md`
- `plans/subsystems/mail-backend-foundation-roadmap.md`
- `plans/registry.md`
- `README.md`