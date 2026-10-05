# Mail Backend Foundation Milestone 009 — Hosted Verification and Corrective Closure

Status: conditionally closed

Repository baseline: planning baseline `e1db79b55e3fdd128b9dafe701b50c5dbf5db9c5`; execute after M008 closure

Source roadmap:

- `plans/subsystems/mail-backend-post-m005-corrective-addendum.md`

Primary class: infrastructure + polish/qualification

## 1. Objective

Add one bounded hosted verification lane for the backend foundation, collect exact hosted evidence for the corrective head, and reconcile planning so M006 is gated only by real local/upstream dependencies.

## 2. Why this milestone is ready

Blocked on M008. Hosted verification should validate the corrected/decomposed shape rather than an intermediate branch.

## 3. Current implementation evidence

scripts/verify.sh quick already owns the deterministic local verification floor: fmt, locked workspace check/tests, clippy with warnings denied, and boundary guard. M001-M005 report green local runs, but no routine .github/workflows verification lane exists at this baseline.

## 4. Invariants that must not regress

- hosted CI invokes the repository-owned verification entry point;
- routine CI requires no public I2P service, Postman account, router, secret, or live network fixture;
- no release/publish/signing automation is introduced;
- tests/lints/guards are not weakened to pass CI;
- MSRV/toolchain policy remains explicit;
- M006 remains blocked until local corrective closure and upstream Plan 355 closure.

## 5. Scope

### In scope

- one GitHub Actions workflow/job for normal Linux verification;
- checkout/toolchain setup compatible with rust-toolchain.toml;
- simple correctness-neutral caching if useful;
- bash scripts/verify.sh quick as canonical hosted command;
- README/contributor verification docs;
- M007-M009 closure/registry/roadmap reconciliation once evidence is green.

### Explicitly out of scope

Live I2P/Postman CI, multi-OS matrix, coverage gates, benchmarks, dependency bots, releases, signing/SBOM/provenance, and M006.

## 6. Required production changes

### A. Hosted workflow

Add .github/workflows/ci.yml with one routine verification job on supported Ubuntu, least required permissions, no secrets, and normal push/pull-request triggers.

### B. Toolchain evidence

Use repository toolchain policy. Do not silently raise rust-version merely to satisfy hosted CI.

### C. Corrective closure reconciliation

After M007 and M008 closure and green hosted CI:

- write plans/closure/mail-backend-foundation/009-status.md;
- close this corrective addendum if no high/medium finding remains;
- update parent roadmap and registry;
- audit M006 against current upstream i2pr state.

If Plan 355 is still not closed, M006 stays blocked on that named interface. If it has closed, perform a fresh interface review before marking M006 ready.

## 7. Ordered work packages

WP1 adds workflow. WP2 runs hosted verification. WP3 reconciles closure/planning.

## 8. Failure semantics

A hosted failure is recorded truthfully. Do not mark M009 closed on local-only success when required hosted evidence is red/unexecuted. Do not hide failures with unconditional retries.

## 9. Compatibility and migration

No product/schema migration.

## 10. Required tests

The workflow runs everything owned by scripts/verify.sh quick, including M007 and M008 regressions. No routine network-dependent test is added.

## 11. Required verification commands

Local:

```bash
bash scripts/verify.sh quick
```

Hosted: exact GitHub Actions run on the corrective head, with run id/result recorded in closure.

## 12. Documentation updates

README verification section, corrective addendum, parent roadmap, registry, M009 closure, and M006 blocker/readiness note.

## 13. Acceptance criteria

- normal hosted CI exists and is green;
- it runs scripts/verify.sh quick;
- no secrets/live network dependencies are required;
- M007/M008 are closed;
- no stale Plan-345-only assertion remains;
- M006 blocker reflects current i2pr Plan 354/355 state exactly.

## 14. Stop conditions

Stop if hosted success requires weakening checks, CI requires live mail/I2P secrets, toolchain incompatibility implies MSRV change, M007/M008 has high/medium findings, or upstream state is ambiguous.

## 15. Closure evidence required

Workflow/permissions review, hosted run id/result, local verification, M007/M008 closure links, registry/roadmap diff, and M006 dependency audit.

## 16. Handoff notes

One deterministic CI lane is sufficient. Do not expand this into release or multi-platform work.
