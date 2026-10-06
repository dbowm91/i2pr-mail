# Mail Backend Foundation M009 Closure — Hosted Verification and Corrective Closure

Status: closed

Date: 2026-10-05 (condition cleared 2026-10-05T21:30:16Z)

Predecessor closures:

- `plans/closure/mail-backend-foundation/007-status.md` — closed
- `plans/closure/mail-backend-foundation/008-status.md` — closed

Source plan: `plans/implementation/mail-backend-foundation/009-hosted-verification-and-corrective-closure.md`

## Outcome

The hosted verification lane exists, invokes the repository-owned verification entry point, requires no secrets and no network fixture, and runs on the corrected and decomposed head. Local verification of the same command is green.

M009 is **closed**. Hosted verification is green on the corrective head.

Two earlier runs were recorded red: the job was cancelled with zero steps executed and no runner assigned, because GitHub Actions was in a major outage. The condition named at conditional closure was met when the outage cleared and run 37376165523 completed successfully, including all six steps and the full verification floor. That history is kept below rather than erased.

## Condition history and clearance

A GitHub Actions run of `.github/workflows/ci.yml` on this branch had to complete with a successful conclusion. The first two runs could not execute.

| Run | Head | Created | Completed | Run conclusion | Job conclusion | Steps executed | Runner assigned |
|---|---|---|---|---|---|---|---|
| 37372987942 | `f4f5d0e` | 20:58:07Z | 21:16:08Z | failure | **cancelled** | **0** | none |
| 37373787767 | `c777a36` | 21:06:02Z | 21:21:36Z | failure | **cancelled** | **0** | none |

Evidence that this is external infrastructure rather than a defect in this workflow:

- Both jobs report `steps=0` and an empty `runner_name`. Nothing in the repository ran: not checkout, not the toolchain install, not `scripts/verify.sh quick`. The job was cancelled while still waiting for a runner.
- The workflow file itself was accepted and scheduled correctly. GitHub parsed it, created a job with the intended name and the `ubuntu-latest` label, and dispatched it. A malformed workflow fails earlier with an `invalid workflow` annotation and creates no job at all.
- Both cancellations landed at roughly 15-18 minutes with no step output, which is the documented signature of runner-assignment failure rather than a build failure.
- GitHub status reports the `Actions` component as `major_outage`. The open critical incident "Incident with Actions", created 2026-10-05T19:11:58Z and still `investigating`, reports "delays in assigning GitHub-hosted runners to Actions jobs", "job failures and delays affecting GitHub-hosted runner assignment and workflow start times", and, as of 21:09:15Z, "ongoing issues with Actions and Hosted Runners".

No retry storm, no conditional re-run, and no weakening of any check was used to work around this. Per the plan's failure semantics, a failed hosted run was recorded as failed rather than hidden, and no re-run was issued while the outage was active.

**Clearance.** GitHub reported the `Actions` component `operational` with no unresolved incidents by 2026-10-06. Run 37376165523 then executed normally and passed. No repository change was required between the cancelled runs and the green run, which is itself confirmation that the earlier failures were infrastructural rather than a defect in the workflow.

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
| **Green run** | **37376165523**, head `9bc320636fd9f70da69119858d69b71264aab3e6` |
| URL | https://github.com/dbowm91/i2pr-mail/actions/runs/37376165523 |
| Event | push to `codex/foundation-planning` |
| Created / completed | 2026-10-05T21:29:19Z / 2026-10-05T21:30:16Z |
| Run conclusion | **success** |
| Job conclusion | **success** |
| Steps | all six green: set up job, checkout, install pinned toolchain, show active toolchain, cache, verify |
| Earlier cancelled runs | 37372987942 (head `f4f5d0e`), 37373787767 (head `c777a36`): run failure, job cancelled, 0 steps, no runner |

Green-run evidence taken from the run log:

- Toolchain resolved from `rust-toolchain.toml` as `1.88.0-x86_64-unknown-linux-gnu`, confirming the MSRV pin is honored on the hosted runner rather than raised to satisfy CI.
- `test result: ok` for all five crates: domain 6, mime 8, proto 11, runtime 19, store 13 — 57 tests, 0 failures.
- `crate dependency and source boundary checks passed` and `typed durable-state API check passed`, so the M007 guard runs in hosted CI too.
- First green run reported a cache miss and populated the cache, as expected for a cold lane.

The two cancelled runs remain listed as red history. That red state was uninformative about the code, since no verification step ever ran in them, and the successful run supersedes it.

## Registry and roadmap reconciliation

- M007: closed, recorded in the roadmap status table and the registry's recently-closed list.
- M008: closed, recorded in the same places.
- M009: closed; the registry's active-closure-work section is now empty and M009 joins the recently-closed list.
- The post-M005 corrective addendum closes: M007-M009 all have accepted closure records with no high or medium finding remaining, hosted deterministic verification is green, and M006's remaining blocker is exactly the named upstream dependency.
- The parent roadmap dependency graph reads M007, M008, and M009 closed.

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

None. The one named condition, a successful hosted run, was cleared by run 37376165523 on 2026-10-05T21:30:16Z. The two earlier cancelled runs were caused by the GitHub Actions major outage, executed zero steps, and required no repository change; they are retained as history rather than reopened work.

## Unblock audit

No further corrective milestone exists in this sequence: M007, M008, and M009 are the whole of the post-M005 corrective line, and M007/M008 are closed with M009 conditionally closed on the run above.

M006 remains blocked for the two reasons above. M009 is closed and the post-M005 corrective sequence M007-M009 is complete with accepted closure records and no high/medium finding.

The corrective addendum therefore closes, subject to its own completion definition. M006 remains blocked solely on upstream i2pr Plans 354 and 355, which no local work can satisfy; when Plan 355 closes, M006 becomes eligible and must perform a fresh interface review against the closed contract before implementation. When upstream Plan 355 closes, M006 becomes eligible and must perform a fresh interface review against the closed contract before implementation.