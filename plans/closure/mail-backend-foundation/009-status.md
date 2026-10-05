# Mail Backend Foundation M009 Closure — Hosted Verification and Corrective Closure

Status: conditionally closed

Date: 2026-10-05

Predecessor closures:

- `plans/closure/mail-backend-foundation/007-status.md` — closed
- `plans/closure/mail-backend-foundation/008-status.md` — closed

Source plan: `plans/implementation/mail-backend-foundation/009-hosted-verification-and-corrective-closure.md`

## Outcome

The hosted verification lane exists, invokes the repository-owned verification entry point, requires no secrets and no network fixture, and runs on the corrected and decomposed head. Local verification of the same command is green.

M009 is **conditionally closed**, not closed. The one unmet acceptance criterion is that hosted CI is *green*. Both runs created for the corrective head concluded unsuccessfully: the job was **cancelled with zero steps executed and no runner assigned**. Per this milestone's own failure semantics, local-only success is not sufficient evidence to record closure, so the condition is named below rather than papered over.

## Named condition to close

A GitHub Actions run of `.github/workflows/ci.yml` on this branch must complete with a successful conclusion. As of 2026-10-05T21:28Z it has not: two runs concluded red because GitHub could not supply a hosted runner, and the outage was still active when this record was written.

| Run | Head | Created | Completed | Run conclusion | Job conclusion | Steps executed | Runner assigned |
|---|---|---|---|---|---|---|---|
| 37372987942 | `f4f5d0e` | 20:58:07Z | 21:16:08Z | failure | **cancelled** | **0** | none |
| 37373787767 | `c777a36` | 21:06:02Z | 21:21:36Z | failure | **cancelled** | **0** | none |

Evidence that this is external infrastructure rather than a defect in this workflow:

- Both jobs report `steps=0` and an empty `runner_name`. Nothing in the repository ran: not checkout, not the toolchain install, not `scripts/verify.sh quick`. The job was cancelled while still waiting for a runner.
- The workflow file itself was accepted and scheduled correctly. GitHub parsed it, created a job with the intended name and the `ubuntu-latest` label, and dispatched it. A malformed workflow fails earlier with an `invalid workflow` annotation and creates no job at all.
- Both cancellations landed at roughly 15-18 minutes with no step output, which is the documented signature of runner-assignment failure rather than a build failure.
- GitHub status reports the `Actions` component as `major_outage`. The open critical incident "Incident with Actions", created 2026-10-05T19:11:58Z and still `investigating`, reports "delays in assigning GitHub-hosted runners to Actions jobs", "job failures and delays affecting GitHub-hosted runner assignment and workflow start times", and, as of 21:09:15Z, "ongoing issues with Actions and Hosted Runners".

No retry storm, no conditional re-run, and no weakening of any check was used to work around this. Per the plan's failure semantics, a failed hosted run is recorded as failed rather than hidden; re-running now would only queue against an active outage. One re-run should be issued once GitHub reports Actions operational, and its result recorded here. Until then nothing in this repository is waiting on a code change.

## Workflow and permission review

`.github/workflows/ci.yml`, one routine job:

| Property | Value | Note |
|---|---|---|
| Triggers | `push`, `pull_request` | normal; no `pull_request_target`, so no privileged-context fork execution |
| Permissions | `contents: read` at workflow level | least privilege; verification only reads the repository |
| Secrets | none | no step references a secret, environment, or service container |
| Runner | `ubuntu-latest`, `timeout-minutes: 30` | one platform, as scoped |
| Verification command | `bash scripts/verify.sh quick` | the repository-owned entry point, not a re-implementation of it |

The job deliberately contains no step referencing an I2P service, Postman account, router, secret, or live network fixture; there is no `curl`, `wget`, `apt-get`, `docker`, or `ssh` step. Nothing was weakened to make CI pass: the hosted lane runs the identical five checks as local verification.

Toolchain policy: the job installs the toolchain with `rustup toolchain install`, which resolves `rust-toolchain.toml` (channel 1.88.0, rustfmt and clippy components) instead of repeating a version literal in the workflow. That keeps `rust-toolchain.toml` the single source of truth, so hosted CI cannot silently diverge from the MSRV policy or raise `rust-version` to make itself pass. This was verified locally against the same file: `rustup show active-toolchain` reports `1.88.0-aarch64-apple-darwin (overridden by rust-toolchain.toml)`.

Caching is correctness-neutral: `actions/cache` over the cargo registry, cargo git, and `target`, keyed on `runner.os` plus a hash of `Cargo.lock` and `rust-toolchain.toml`, with a coarse OS-only restore key. A dependency or toolchain change therefore never reuses a stale artifact.

## Local verification

Commands run from the repository root at the corrective head, all passing:

- `cargo fmt --all -- --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets` — passed, 57 tests (domain 6, mime 8, proto 11, runtime 19, store 13).
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed, including the dependency, typed-state, and positive-control checks.
- `bash scripts/verify.sh quick` — passed.

This run covers the M007 regressions (request-ledger bounds and release, typed state round-trip and fail-closed migration, protocol ceilings) and the M008 shape (decomposed modules, per-owner tests, unchanged public API).

## Hosted evidence

| Item | Value |
|---|---|
| Workflow | `.github/workflows/ci.yml`, name `CI` |
| Runs | 37372987942 (head `f4f5d0e`), 37373787767 (head `c777a36`) |
| URL | https://github.com/dbowm91/i2pr-mail/actions/runs/37372987942 |
| Event | push to `codex/foundation-planning` |
| Run conclusion | **failure** for both runs |
| Job conclusion | **cancelled** for both runs |
| Steps executed | **0** for both runs; no step produced output |
| Runner assigned | none for either job; `runner_name` empty |
| Interpretation | the lane was scheduled correctly but never obtained a hosted runner, during the GitHub Actions major outage |

The branch therefore currently shows red CI. That red state is uninformative about the code: no verification step ran. It is recorded here so the branch status is not mistaken for a code failure.

## Registry and roadmap reconciliation

- M007: closed, recorded in the roadmap status table and the registry's recently-closed list.
- M008: closed, recorded in the same places.
- M009: conditionally closed, listed under active closure work in the registry.
- The post-M005 corrective addendum remains `active`. Its completion definition requires M007-M009 to have accepted closure records with no high/medium finding; M009 is not yet accepted, so the addendum is not closed. M009 plan status is recorded as conditionally closed.
- The parent roadmap dependency graph now reads M007 and M008 closed and M009 dependency-ready.

## M006 dependency audit

Audited against the actual upstream repository on 2026-10-05, branch `work/router-console-plans-356-358` at `f6036a9e`:

- Closure records exist for Plans 345, 349, 352, 353 under `plans/closure/managed-native-app-runtime/`.
- No closure record exists for Plan 354 or Plan 355.
- Upstream registry row: "Plans 345, 349, 352, 353 passed; 354 ready; 355 blocked". Plan 354 owns listener-independent private SAM/I2CP connection drivers; Plan 355 owns the trusted app-principal router gateway and is hard-blocked on 354.
- Implementation plans exist for both 354 and 355.
- The commit registering them (`82080dbd`) is on `work/router-console-plans-356-358` and `work/plans-352-353`, but is not an ancestor of upstream `main`.

Conclusion: the gateway M006 targets is Plan 355, which has not started and is itself blocked on Plan 354. M006 remains blocked on two independent conditions:

1. local: i2pr-mail M009 closure — M007 and M008 are closed, M009 is conditionally closed on its hosted run;
2. upstream: i2pr Plan 354 closure and Plan 355 closure.

Neither condition can be satisfied by further local corrective work. Stale-planning cleanup performed as part of this audit:

- `006-i2pr-managed-app-transport-adapter.md` external-evidence section rewritten from the superseded Plan-345-only reading to the exact current state, with commits, branch, and closure-record paths.
- `ADR-0001` received a dated correction to its upstream-state observation. The decision text itself was not modified, since an accepted ADR is not rewritten by repository evidence.
- The historical M005 closure record retains its original Plan-345 statement. Closure records are immutable; its claim was accurate when written and is superseded by this record, not edited.

## Unresolved findings

High/medium severity: none.

Named operational condition (this milestone): a hosted run must complete successfully. Runs 37372987942 and 37373787767 both concluded red with the job cancelled after zero steps and no runner, during the GitHub Actions major outage. This is the sole reason for conditional rather than full closure.

## Unblock audit

No further corrective milestone exists in this sequence: M007, M008, and M009 are the whole of the post-M005 corrective line, and M007/M008 are closed with M009 conditionally closed on the run above.

M006 remains blocked for the two reasons above. When a hosted run goes green, record the run id and result and upgrade this record to closed; no other plan changes status at that point, because M006 is still gated by upstream Plans 354 and 355. When upstream Plan 355 closes, M006 becomes eligible and must perform a fresh interface review against the closed contract before implementation.