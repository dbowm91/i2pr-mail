# i2pr-mail

Rust backend-first mail client for I2P in-network mail services, designed as a managed native application for i2pr.

The initial product boundary is intentionally narrow: Postman-compatible POP3 receipt, SMTP submission, durable local mail state, MIME parsing/building, and a frontend-neutral application API. Direct clearnet networking, generic Internet mail-provider support, and the graphical frontend are outside the foundation.

Development sequencing and implementation handoff are controlled by plans/registry.md.

## Development

The workspace targets Rust 1.88+ / edition 2024, pinned exactly by `rust-toolchain.toml`. Runtime integration is currently verified with scripted in-memory transport; no direct host networking is implemented.

## Verification

`bash scripts/verify.sh quick` is the single canonical verification entry point. It runs, in order:

1. `cargo fmt --all -- --check`
2. `cargo check --locked --workspace --all-targets`
3. `cargo test --locked --workspace --all-targets`
4. `cargo clippy --locked --workspace --all-targets -- -D warnings`
5. `bash scripts/check-boundaries.sh`

The boundary guard enforces the dependency direction (`domain <- mime/proto/store <- runtime`), rejects network, filesystem, and async capability references in the lower crates, rejects untyped durable-state parameters on public store and runtime APIs, and carries its own positive controls so the guard itself is proven to reject a violation.

Verification is deterministic and offline: no I2P service, Postman account, router, secret, or live network fixture is required.

Hosted verification runs the same command in `.github/workflows/ci.yml` on every push and pull request, with `contents: read` permissions and no secrets. Running the script locally is equivalent to the hosted lane; if the two ever disagree, that is a defect in the workflow or the script, not a reason to weaken either.