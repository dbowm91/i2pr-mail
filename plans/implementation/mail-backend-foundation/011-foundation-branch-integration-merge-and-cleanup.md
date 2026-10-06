# Mail Backend Foundation Milestone 011 — Foundation Branch Integration, Merge, and Cleanup

Status: closed — see `plans/closure/mail-backend-foundation/011-status.md`

Executed as written. `main` moved `9f2cb7c74e0b7fab4cabba4fd404fab575a19750` → `64cd8121c387a42274e07f45a84e227ec23169d9` by a single non-forced fast-forward, with no production change and no rewrite of any cited commit. Candidate CI `37491157600` and integration-main CI `37491896332` are both green. The sections below are retained unedited as the plan of record; where execution refined a value, the closure record is authoritative.

Planning baseline:

- integration branch: `codex/foundation-planning` at `1d17784eaa12366f3f2a93602916d8320000b7af`;
- main: `9f2cb7c74e0b7fab4cabba4fd404fab575a19750`;
- compare at review: 22 commits ahead, 0 behind;
- exact-head hosted CI: run `37488611420`, success;
- this plan's registration commits necessarily advance the integration branch, so execution must re-freeze and re-qualify the exact final branch head before moving `main`.

Source roadmap:

- `plans/subsystems/mail-backend-foundation-roadmap.md`

Closed prerequisites:

- M001-M005 backend foundation;
- M007-M009 corrective/qualification sequence;
- M010 SAM 3.1 client codec and downstream contract qualification.

Blocked successor:

- M006 i2pr managed-app transport adapter, independently blocked on the upstream app-side managed-app runtime.

Applicable ADRs:

- `plans/adrs/ADR-0001-backend-first-i2p-mail-and-transport-boundary.md`
- `plans/adrs/ADR-0002-synchronous-transport-seam-and-async-confinement.md`

Primary class: integration + planning/branch hygiene

## 1. Objective

Integrate the completed and qualified mail foundation from `codex/foundation-planning` into `main` without rewriting evidence-bearing history, prove the resulting mainline head with the repository's hosted verification lane, reconcile planning so M006 is the only remaining active capability blocker, and retire the work branch only after mainline closure is durable.

This milestone is intentionally not product development. It prevents a mature, independently useful backend from remaining stranded on a long-lived planning branch while upstream i2pr's app-side runtime is still unplanned.

## 2. Why this milestone is ready

All local capability/corrective work that can proceed without the upstream app runtime is closed:

- M001-M005 are closed;
- M007-M009 are closed with hosted verification;
- M010 is closed with no high/medium finding;
- M006 is blocked for a named external reachability dependency, not unfinished local backend work.

At the review baseline, `main` is an ancestor of the integration branch: the branch is 22 commits ahead and 0 behind. Both `main` and `codex/foundation-planning` are unprotected. The reviewed integration head has a successful exact-head CI run.

Because existing closure records cite exact commit SHAs, a history-preserving fast-forward is the preferred integration mechanism.

## 3. Current implementation evidence

The integration branch contains:

- the Rust workspace and five backend crates plus the isolated `i2pr-mail-sam` codec;
- typed state/schema-v4 persistence and bounded request lifecycle;
- decomposed runtime ownership;
- 109 deterministic tests at M010 closure;
- dependency/sans-I/O/typed-state boundary guards with mutation-style negative controls;
- GitHub Actions routine CI invoking `bash scripts/verify.sh quick`;
- accepted ADRs, architecture/security documentation, implementation plans, and closure records through M010.

Current main contains only the initial repository README commit. No independent mainline product work needs to be reconciled at the planning baseline.

The reviewed exact integration head `1d17784e` passed hosted CI run `37488611420`. Because plan registration advances the branch, this run is evidence of readiness but is not sufficient by itself for the eventual merge head.

## 4. Invariants that must not regress

1. Do not squash, rebase, cherry-pick/recreate, or otherwise rewrite commits that are cited by closure records.
2. Never force-push `main` or the integration branch.
3. Fast-forward `main` only when it is still an ancestor of the exact qualified integration head.
4. The exact final integration head must have green hosted CI before `main` moves.
5. The exact resulting mainline head must have green hosted CI before branch cleanup or M011 closure.
6. No production code, schema, API, security policy, dependency, or support claim changes merely to complete integration.
7. M006 stays blocked on the upstream app-side managed-app runtime; merging the foundation must not be represented as transport capability.
8. Historical implementation/closure plans remain in Git history and on main; branch cleanup does not mean deleting planning evidence.
9. Branch deletion is the last destructive operation and happens only after mainline evidence and closure/planning reconciliation are durable.
10. If `main` moves independently, stop and reconcile rather than choosing a merge strategy ad hoc.

## 5. Scope

### In scope

- exact-head integration preflight;
- compare/ancestry verification against current `main`;
- final branch-local deterministic verification and exact-head hosted CI;
- history-preserving fast-forward of `main`;
- exact-head mainline hosted CI;
- mainline planning/registry/roadmap reconciliation and M011 closure record;
- confirmation that M006 remains blocked only by its named upstream dependency;
- remote work-branch retirement after closure;
- stale branch-specific wording cleanup where it would be false on main after integration.

### Explicitly out of scope

- M006 implementation;
- any i2pr upstream change;
- live SAM/Postman interoperability;
- release/tag/publication;
- version bump solely for merge;
- GUI/frontend work;
- feature additions, dependency upgrades, refactors, schema changes, or API cleanup;
- rewriting M001-M010 closure records to make them read as if they originally ran on main.

## 6. Required production changes

No production-code change is required or authorized by this milestone.

Only repository integration and planning/documentation hygiene may change. If preflight reveals a production defect, stop and register a corrective instead of hiding it inside merge work.

### A. Freeze the final integration candidate

Immediately before integration:

1. read current `main` and `codex/foundation-planning` SHAs;
2. compare `main...codex/foundation-planning`;
3. require `behind_by == 0` and the merge base to equal current `main`;
4. run the full local routine floor on the exact branch head;
5. require a successful GitHub Actions CI run for that exact head SHA.

Record all SHAs and the CI run id in the closure record.

### B. Preserve history

Preferred integration is a direct fast-forward of `main` from its observed SHA to the exact qualified branch head, using a compare-and-swap/expected-SHA guard where the execution tool supports it.

Do not:

- squash;
- rebase;
- recreate commits;
- force-update;
- merge a stale previously qualified head after the branch has advanced.

A non-fast-forward result is a stop condition, not permission to improvise.

### C. Prove mainline integration

After moving `main`:

- confirm `main` equals the qualified integration SHA before any closure-only commit;
- confirm the old `main` is an ancestor;
- confirm the feature branch has zero product/planning diff against that main integration head;
- require hosted CI success on that exact mainline integration SHA.

### D. Land closure/planning reconciliation on main

After integration-head CI is green, write `plans/closure/mail-backend-foundation/011-status.md` on `main` and reconcile:

- registry: M011 closed; no dependency-ready local implementation work; M006 blocked on upstream app-side runtime;
- parent roadmap: foundation through M010 integrated to main; M011 closed;
- any wording that still calls `codex/foundation-planning` the authoritative product branch;
- branch status: work branch is cleanup-only/deletable.

The closure/planning commit itself must also pass routine CI on main. This post-integration commit may make main one commit ahead of the feature branch; that is expected.

### E. Retire the work branch last

Delete `codex/foundation-planning` only after:

- the integration SHA is reachable from main;
- the M011 closure/planning commit is on main;
- hosted CI is green on the final main head;
- M006's blocked status is present on main;
- no open work or unique commit exists only on the branch.

If branch deletion is unavailable in the execution environment, record it as a low-severity repository-hygiene remainder and leave the branch untouched rather than using unsafe ref manipulation.

## 7. Ordered work packages

### WP1 — Re-freeze and qualify exact branch head

Recompute compare/ancestry, run the routine floor, and obtain exact-head hosted CI.

### WP2 — Fast-forward integration

Move `main` to the exact qualified integration SHA using expected-old-SHA protection. Perform no other changes in this operation.

### WP3 — Mainline qualification

Verify ancestry/file parity and obtain successful hosted CI on the integration SHA as `main`.

### WP4 — Closure and planning reconciliation

Create M011 closure on main, update roadmap/registry/status wording, and rerun routine hosted CI on the final main head.

### WP5 — Branch cleanup

Audit for unique branch commits/open work, then delete the integration branch if all cleanup gates pass.

## 8. Failure, cancellation, restart, and contention semantics

This milestone treats Git refs as concurrently mutable state.

- If `main` changes between preflight and update, the expected-SHA update must fail; do not retry blindly.
- If the integration branch advances after exact-head qualification, the old green run is stale. Re-freeze and re-run qualification.
- If the main ref update succeeds but CI fails, keep the branch, keep M011 open, and diagnose/revert only through a separately reasoned corrective; do not delete evidence.
- If the closure/planning commit fails CI, the product integration remains on main but M011 remains open until the planning head is green.
- If branch deletion occurs partially or cannot be confirmed, verify refs before further action; never recreate/delete refs from guesses.
- No automatic retry loop is authorized for ref updates or merge operations.

## 9. Compatibility and migration

No runtime compatibility or storage migration is introduced.

The schema remains at the version closed by M007. Public Rust APIs remain those qualified through M010.

Git-history compatibility is itself a requirement: preserving existing commit SHAs keeps closure records and architectural citations valid.

## 10. Required tests

Before integration, on the exact candidate head:

- full `scripts/verify.sh quick`;
- all workspace tests including `i2pr-mail-sam`;
- boundary guards;
- hosted CI exact-head success;
- compare shows zero commits behind main.

After fast-forward, on the exact integration main head:

- hosted CI success;
- compare of old integration branch to main shows no unique integrated content;
- all expected workspace/plans/docs files exist on main.

After closure/planning commit:

- `scripts/verify.sh quick`;
- hosted CI success on final main head;
- registry/roadmap consistency assertions by review;
- M006 remains blocked and no false live-I2P support claim appears.

No live network test is required by M011.

## 11. Required verification commands

Repository floor:

```bash
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
bash scripts/verify.sh quick
```

Git/integration evidence, or equivalent connector operations:

```bash
git merge-base --is-ancestor main codex/foundation-planning
git rev-list --left-right --count main...codex/foundation-planning
git diff --check main...codex/foundation-planning
```

Expected before fast-forward: main is ancestor, left count 0, right count >0.

Expected immediately after fast-forward: main and the qualified integration head identify the same commit.

Hosted evidence:

- successful CI on the exact final branch candidate;
- successful CI on the exact integration SHA after it becomes main;
- successful CI on the final main closure/planning SHA.

## 12. Documentation updates

Required during closure:

- `plans/closure/mail-backend-foundation/011-status.md`;
- `plans/registry.md`;
- `plans/subsystems/mail-backend-foundation-roadmap.md`;
- only those architecture/README references that would otherwise falsely describe the work branch as current authority.

Do not rewrite M001-M010 closure records. Their branch/head statements are historical evidence.

## 13. Acceptance criteria

M011 closes only when all are true:

- all M001-M005 and M007-M010 commits are reachable from `main`;
- no evidence-bearing commit was rewritten;
- integration used a fast-forward from a verified ancestor relationship;
- exact candidate-head CI was green;
- exact integrated-main-head CI was green;
- final closure/planning main head CI was green;
- M011 closure record maps pre/post SHAs and CI run ids;
- registry and roadmap identify M006 as the only remaining active capability plan and mark it blocked on the upstream app-side runtime;
- no unique production/planning work remains solely on `codex/foundation-planning`;
- the work branch is deleted, or branch deletion is explicitly recorded as the only low-severity cleanup remainder because the execution environment cannot delete it.

## 14. Stop conditions

Stop and report rather than merge if:

- `main` is no longer an ancestor of the integration branch;
- compare shows the branch behind main;
- exact candidate-head hosted CI is absent, pending, or red;
- any high/medium finding appears in pre-merge review;
- integration would require squash/rebase/cherry-pick/history rewriting;
- fast-forward requires a force update;
- branch contents contain unclosed product work not represented in the registry;
- M006 is accidentally made ready by planning cleanup despite the upstream app-side runtime remaining unavailable.

After integration, stop closure/cleanup if mainline CI is red.

## 15. Closure evidence required

The M011 closure record must contain:

- old main SHA;
- exact qualified integration-branch SHA;
- pre-integration compare/merge-base result;
- candidate exact-head CI run id/result;
- ref-update/fast-forward evidence;
- integrated main SHA;
- integrated-main CI run id/result;
- closure/planning commit SHA;
- final-main CI run id/result;
- confirmation that cited M001-M010 commits retain their original SHAs;
- registry/roadmap diff summary;
- M006 unblock audit against current i2pr state;
- branch deletion evidence or explicit low-severity remainder;
- unresolved findings by severity.

## 16. Handoff notes

This is an integration milestone, not an invitation to improve code while merging.

The strongest constraint is history preservation. Existing closure records intentionally cite exact SHAs; a squash or rebase would make the repository's own evidence misleading. As long as `main` remains the ancestor seen at planning time, fast-forward is both the smallest and the most auditable operation.

Do branch deletion last. A long-lived branch is harmless for a few more minutes; deleting the only easy recovery/reference point before mainline verification is not.
