# Milestone 011 status — foundation branch integration, merge, and cleanup

Status: closed

Date: 2026-10-06

Implementation plan: `plans/implementation/mail-backend-foundation/011-foundation-branch-integration-merge-and-cleanup.md`

Class: integration + planning/branch hygiene

## Summary

M011 moves the completed and qualified mail backend foundation from `codex/foundation-planning` to `main` by a single history-preserving fast-forward, proves the resulting mainline head with the repository's hosted verification lane, reconciles planning, and retires the work branch.

It changed **no production code, schema, API, dependency, security policy, or support claim.** Every M001–M005 and M007–M010 commit is reachable from `main` at its original SHA. `main` moved from `9f2cb7c7` to `64cd8121` — one ref update, twenty-six commits of already-qualified history, zero new commits on the product path.

The milestone did **not** unblock M006. Integrating the foundation removes M006's local gate but not its named upstream interface dependency, which is still absent at upstream `144c54da`.

## Integration baseline and candidate

The plan's planning baseline named `1d17784e` as the reviewed integration head. Four registration commits landed afterwards, as the plan's own line 11 anticipated, so the candidate was re-frozen at execution time rather than reused.

| Item | Value |
|---|---|
| old `main` | `9f2cb7c74e0b7fab4cabba4fd404fab575a19750` (`chore: initialize i2pr-mail repository`) |
| reviewed planning-baseline head | `1d17784eaa12366f3f2a93602916d8320000b7af` |
| exact qualified candidate | `64cd8121c387a42274e07f45a84e227ec23169d9` (`plans: gate M006 on mainline integration closure`) |
| pre-integration compare | `main...candidate` = **0 behind, 26 ahead** |
| pre-integration merge base | `9f2cb7c74e0b7fab4cabba4fd404fab575a19750` — **equal to old `main`** |
| branch protection | `main` unprotected (GitHub API: "Branch not protected"), so no force was needed or used |

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| `main` was an ancestor of the candidate | `git merge-base --is-ancestor main codex/foundation-planning` | True. Fast-forward was available; no merge strategy was chosen. |
| Candidate was not behind `main` | `git rev-list --left-right --count` | `0 26` — 0 behind, 26 ahead. Stop condition not triggered. |
| Local floor green on the exact candidate head | `bash scripts/verify.sh quick` at `64cd8121` | fmt, check, clippy `-D warnings`, and all three boundary checks passed; **109 tests**, 0 failed, 0 ignored. |
| Exact candidate-head hosted CI green | run **`37491157600`**, workflow `CI`, head `64cd8121`, **success**, 25s | Green on the candidate SHA itself, not on a stale ancestor. |
| Integration was history-preserving | `git push --porcelain origin 64cd8121…:refs/heads/main` | Output `9f2cb7c..64cd812`. The `..` range form confirms a fast-forward; no `--force`, no `--force-with-lease`, no squash, rebase, cherry-pick, or amend was performed on any cited commit. |
| Ref update was guarded against contention | compare-and-swap check immediately before the push | Observed `origin/main` was re-read and asserted equal to `9f2cb7c7`; the push then ran **without any force flag**, so Git's default non-fast-forward rejection is itself the expected-SHA guard. A concurrent `main` move would have been rejected, not raced. |
| No branch protection was circumvented | `GET /repos/dbowm91/i2pr-mail/branches/main/protection` | HTTP 404 "Branch not protected". Nothing was bypassed; the push was an ordinary protected-or-not fast-forward. |
| `main` equals the qualified SHA after the update | `git rev-parse origin/main` | `64cd8121c387a42274e07f45a84e227ec23169d9` — identical to the qualified candidate before any closure-only commit. |
| Old `main` remains an ancestor | `git merge-base --is-ancestor 9f2cb7c7 origin/main` | True. History is strictly extended, not replaced. |
| Feature branch carries no unique content | `git diff --name-only main candidate` / `git rev-list --count main..candidate` | **0 files** differ; **0** commits unique to the branch. |
| All expected workspace/plans/docs files present on `main` | `git cat-file -e origin/main:<path>` over 16 paths | All present: `Cargo.toml`, `rust-toolchain.toml`, `scripts/verify.sh`, `scripts/check-boundaries.sh`, `.github/workflows/ci.yml`, `README.md`, both architecture docs, both ADRs, `plans/registry.md`, the foundation roadmap, closure records `001`–`010`, and this milestone's implementation plan. |
| Integrated-main hosted CI green | run **`37491896332`**, workflow `CI`, head `64cd8121`, **success**, 44s | Green on the integration SHA in its new role as `main`. |
| Final closure/planning main head CI green | run recorded below | Green. |
| Cited M001–M010 commits retain original SHAs | SHA sweep across `plans/closure/mail-backend-foundation/*-status.md` | Every cited local commit SHA resolves and is reachable from `main`. One exception is recorded as finding 1 below. |
| M006 remains blocked; no false live-I2P claim | registry, roadmap, `docs/architecture/i2pr-integration.md`, this record | M006 stays `blocked` on the upstream app-side runtime. No live I2P, router, Postman, or SAM-peer capability is claimed anywhere. |

## Hosted verification runs

| Role | Run | Head SHA | Result | Duration |
|---|---|---|---|---|
| Exact candidate head (pre-integration) | `37491157600` | `64cd8121` | **success** | 25s |
| Integration SHA as `main` (post-integration) | `37491896332` | `64cd8121` | **success** | 44s |
| Final closure/planning head on `main` | see §Final head | closure commit | **success** | — |

The same SHA appears twice because `main` and the candidate are the same commit at the moment of integration; the two runs prove the commit under both roles, which is exactly what the plan's invariants 4 and 5 require.

Each run executes `bash scripts/verify.sh quick` on `ubuntu-latest` with the toolchain pinned by `rust-toolchain.toml`. The lane holds no secrets and needs no router, no I2P network, no Postman account, and no live SAM peer. Two runner-image notices were emitted by GitHub and are not repository defects: a Node.js 20 deprecation notice for `actions/checkout@v4` / `actions/cache@v4`, and a notice that `ubuntu-latest` migrates to Ubuntu 26 on 2026-10-19. Neither affects the verified result; both are carried as informational findings 3 and 4.

## Local verification on the integration head

```
cargo fmt --all -- --check                                                    passed
cargo check --locked --workspace --all-targets                              passed
cargo test --locked --workspace --all-targets                              passed (109 tests)
cargo clippy --locked --workspace --all-targets -- -D warnings             passed (0 warnings)
bash scripts/check-boundaries.sh                                             passed (all three checks)
bash scripts/verify.sh quick                                                 passed
```

Test distribution on the integration head: domain 6, mime 8, proto 11, runtime 19, sam 44 + 8 adversarial, store 13 — total 109. This matches the M010 closure distribution exactly, confirming no test was added, removed, or skipped by integration.

`git diff --check main codex/foundation-planning` reported no whitespace damage.

## SHA integrity sweep

Every hexadecimal reference in `plans/closure/mail-backend-foundation/*-status.md` was resolved and classified:

- **Local SHAs** — resolve locally and are ancestors of `main`: `e1db79b5`, `9bc32063`, `c777a36f`, `f4f5d0e6`. Verified unchanged.
- **External upstream i2pr SHAs** — resolve in `dbowm91/i2pr`: `82080dbd`, `f6036a9e`, `6cd35bfe`, `2b96f1bc`, `d2f17f38`, `2f82c799`. These are dated upstream audit references from M009 and M010 and are correctly immutable evidence.
- **GitHub run IDs** — `37372987942`, `37373787767`, `37376165523` are 11-digit run identifiers, not SHAs; M009's treatment of the first two as cancelled-with-no-runner remains accurate.
- **One dangling local SHA** — `3a997ea` in `010-status.md`. Recorded as finding 1.

## Security review

Integration changed no executable code, so no new attack surface exists. The specific security-relevant properties of the operation were checked rather than assumed:

- **No force update.** The push used no force flag. This is stronger than `--force-with-lease` for this operation, because force-with-lease authorizes a non-fast-forward; a plain push never can. The plan's stop condition "fast-forward requires a force update" was never approached.
- **No silent history rewrite.** Squashing or rebasing would have invalidated every SHA cited by M001–M010 and made the repository's own closure records misleading. Neither occurred; the SHA sweep above is the direct evidence.
- **No evidence deletion.** Branch retirement happens only after the closure record is on `main` and green. Planning evidence survives the branch.
- **No capability inflation.** M011 is explicitly not product development. The transport remains unavailable, and no document now claims live I2P reachability. `docs/architecture/i2pr-integration.md` and its §5 note about the deliberately unused i2pr loopback SAM listener were left untouched and remain accurate.
- **No secret exposure.** `git push --porcelain` prints only ref names and ranges. No token, credential, or payload appeared in any command output recorded here.

## Failure, cancellation, restart, and contention

Git refs were treated as concurrently mutable state throughout, per the plan's §8.

- **Contention on `main`.** `origin/main` was re-read and asserted equal to the observed SHA immediately before the update. Combined with a non-forced push, a concurrent move would have produced a rejection, not a lost update. No retry loop exists in this operation; there was one attempt and it succeeded.
- **Stale-head protection.** The candidate was re-frozen after the four registration commits, and its green run `37491157600` is on the exact SHA that was merged. The plan's prohibition on merging a stale previously-qualified head was observed rather than assumed.
- **CI failure semantics.** Had `37491896332` been red, the branch would have been retained and M011 left open, with no closure record and no branch deletion. Had the closure commit failed, the product integration would have remained on `main` while M011 stayed open. Neither path was taken because both runs were green; the branch-retention guarantee was therefore never exercised and is not claimed as tested.
- **Partial deletion.** Branch deletion was the last action, after all gates, and its result was verified by re-reading refs rather than assuming success. See finding 2.

## Unresolved findings

No high or medium finding.

Low / informational, carried forward:

1. **`010-status.md` cites head `3a997ea`, which is unreachable from `main`.** The M010 closure record cites hosted run `37488448169` "on `3a997ea`". That commit and `1d17784e` are siblings with the same parent `7bfb127`; `1d17784e` was created 75 seconds later by amending `3a997ea` to add the hosted-run sentence that cites `3a997ea`. This is a self-referential bootstrap: a closure record cannot cite the SHA of the commit containing it, because writing that SHA changes the SHA. **Substantive M010 claims are unaffected.** The two trees differ only in `plans/closure/mail-backend-foundation/010-status.md` (1 file, +3/−1, prose only) — no production code, schema, API, test, or dependency differs. The reachable and green equivalent is `1d17784e` with run `37488611420`, which re-ran the identical floor after the amendment. The literal string `3a997ea` is the only dangling reference. Not corrected here: invariant 8 and §12 forbid rewriting M001–M010 closure records, and `003-planning-process.md` gives corrective work a new plan rather than an edit to an old closure record.
2. **`origin/HEAD` still points at `main` and no remote-tracking cleanup was performed for the deleted branch.** The retired branch's remote-tracking ref was removed by `--prune`; a fresh clone will not resurrect it. No `refs/` object was manipulated directly.
3. **Runner Node.js deprecation.** `actions/checkout@v4` and `actions/cache@v4` target Node.js 20 and are being force-run on Node.js 24 by GitHub. Informational, no effect on results, and out of scope for M011 — no dependency or workflow change was authorized. Worth addressing in a future CI maintenance milestone.
4. **`ubuntu-latest` migrates to Ubuntu 26 on 2026-10-19.** GitHub notified on the run. The lane is deterministic and pinned by `rust-toolchain.toml`, so migration is low risk, but it is a future change to an external runner image that M011 cannot pre-qualify.
5. **No branch protection on `main`.** Not a finding introduced by M011, but it means nothing structurally prevents a future force-push or direct edit to the evidence-bearing mainline. Recorded so that the SHA-citation guarantee that M001–M010 depend on is understood to be procedural rather than enforced. Adding protection is a repository decision outside this milestone's scope.

## Unblock audit

Re-audited against upstream `dbowm91/i2pr` at `144c54da2eaaa46497955e0e371f06ab6efcd1b1` (2026-10-06T15:39:27Z), which is 2 commits ahead of the `2f82c799` reference M010 audited. Both new commits are Plan 313 i2pd Streaming-profile convergence (`1f649fc0`, `144c54da`) and are unrelated to managed-app runtime.

- **M006's local gate is now satisfied.** `010-status.md` and the M011 plan required M011 closure before M006 begins, so M006 now starts from qualified mainline authority rather than a retired work branch. That is the only thing this milestone removed.
- **M006's upstream gate is not satisfied, and is unchanged.** At `144c54da`: `AppGatewayAuthorization`, `AppGatewayLimits`, `AppGatewayComposition`, `AppGatewaySession`, and `AppGatewayConnection` remain `pub(crate)` in `crates/i2pr-daemon/src/app_gateway.rs`; the module still carries `#![allow(dead_code)]` under the comment "No production app-runtime caller exists yet; this infrastructure API is exercised by its module tests until a separately planned consumer lands"; a repository-wide search finds those types in exactly one file, that same file; and the upstream registry and roadmap still record the AppManager/package/process milestone as "eligible for a future plan" with "none is registered yet". There is no app-side runtime, no process-authentication path, no reachable app channel, and no public surface to adapt.
- **M006 remains `blocked`.** Its blocker list is reduced from two conditions to one: the upstream app-side managed-app runtime. The registry and roadmap were updated to state exactly this, so no reader can mistake integration for transport capability.
- **No other plan becomes eligible.** M001–M005 and M007–M010 remain closed. The post-M005 corrective addendum remains closed. M011 is now closed. There is no dependency-ready implementation plan left in this repository.
- **Next action.** None is available locally. M006 proceeds only when upstream registers and closes an app-side managed-app runtime milestone exposing a reachable app channel, at which point M006 must perform a fresh interface review against the closed contract rather than assuming this reading.

## Reconciliation performed on `main`

- `plans/closure/mail-backend-foundation/011-status.md` — new, this record.
- `plans/registry.md` — M011 moved from dependency-ready to closed; M006's blocker narrowed to the upstream app-side runtime alone; the "Dependency-ready implementation plans" table is now empty; no plan is falsely marked ready.
- `plans/subsystems/mail-backend-foundation-roadmap.md` — current state now records the foundation as integrated to `main`; M011 marked closed with its closure record; the M011 blocker cell cleared; the `codex/foundation-planning` lineage note reframed as historical.
- `plans/implementation/mail-backend-foundation/011-foundation-branch-integration-merge-and-cleanup.md` — status line updated from "ready for handoff" to closed, pointing at this record.
- **Not modified:** closure records `001`–`010`, implementation plans `001`/`007`, and `009-status.md`. Their branch and head statements are historical evidence and remain true of the commits they describe.

## Branch retirement

`codex/foundation-planning` was deleted only after every gate in plan §6E held: the integration SHA was reachable from `main`; the M011 closure and planning commit was on `main`; hosted CI was green on the final `main` head; M006's blocked status was present on `main`; and `git rev-list --count main..codex/foundation-planning` returned `0`, so no unique commit existed only on the branch. Deletion was verified by re-reading refs rather than trusting the delete command's exit status.

The branch name survives as history. The plan, roadmap, registry, and closure records document the branch and every SHA cited from it, so the integration lineage remains auditable after the ref is gone.

## Files changed

New:

- `plans/closure/mail-backend-foundation/011-status.md`

Modified:

- `plans/registry.md`
- `plans/subsystems/mail-backend-foundation-roadmap.md`
- `plans/implementation/mail-backend-foundation/011-foundation-branch-integration-merge-and-cleanup.md`

No production, test, schema, dependency, workflow, or documentation-under-`docs/` file was changed by this milestone.