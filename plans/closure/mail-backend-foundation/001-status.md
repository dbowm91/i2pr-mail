# M001 Closure — Workspace, Domain, and Boundaries

Status: closed

## Implementation evidence

- Rust 2024 workspace, Rust 1.88 toolchain floor, five ownership crates and lockfile added.
- Domain includes bounded, distinct local ids, opaque Uidl, logical I2P service endpoints, mailbox roles, receive states, and submission states. No credential value type or ambient network capability exists in domain.
- Dependency direction is encoded in manifests and checked by `scripts/check-boundaries.sh`; its injected forbidden-dependency positive control passes.
- Architecture docs and quick verification script added.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| bounded identifiers and max+1 | `crates/mail-domain` tests exercise all local id newtypes at max and max+1; Uidl max+1 and opaque path-like input; empty input rejected |
| identity separation and states | distinct Rust newtypes; state enum test keeps DeliveryUnknown separate from FailedSafeToRetry |
| logical service profile | constructor accepts `.i2p` destination and rejects localhost/zero port |
| dependency graph | `scripts/check-boundaries.sh` exact manifest comparison and capability source scan |
| guard positive control | script injects a simulated `tokio` project dependency and proves the comparator rejects it |
| runtime/network/storage/frontend not in pure crates | source scan and dependency direction guard; only runtime defines MailTransport |

## Verification

Run on 2026-10-05 with Rust 1.98.1 (the project floor is 1.88):

- `cargo fmt --all -- --check` — passed via `bash scripts/verify.sh quick`.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets` — passed, 12 tests.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — passed.
- `bash scripts/check-boundaries.sh` — passed, including positive control.
- `bash scripts/verify.sh quick` — passed.

## Failure, restart, security, and dependency review

M001 owns no persistent or network operation. Invalid values return validation errors. No secret-bearing domain type, socket, resolver, task executor, filesystem persistence implementation, GUI, or i2pr crate is present. `mail-domain` depends only on serde and thiserror. Third-party MIME/storage dependency decisions are deferred to M002.

## Unresolved findings

None at M001 scope.

## Unblock audit

M002 is now active because its only hard dependency, M001, is closed. M003 and M004 remain blocked on M002 closure. M005 remains blocked on M003/M004. M006 remains blocked on M005 and the separately required stable i2pr managed-app transport gateway.

## Implementation commit

Recorded in Git history with this closure update.
