#!/usr/bin/env bash
set -euo pipefail
mode="${1:-quick}"
if [[ "$mode" != quick ]]; then echo "usage: $0 quick" >&2; exit 2; fi
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
bash scripts/check-boundaries.sh
