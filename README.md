# i2pr-mail

Rust backend-first mail client for I2P in-network mail services, designed as a managed native application for i2pr.

The initial product boundary is intentionally narrow: Postman-compatible POP3 receipt, SMTP submission, durable local mail state, MIME parsing/building, and a frontend-neutral application API. Direct clearnet networking, generic Internet mail-provider support, and the graphical frontend are outside the foundation.

Development sequencing and implementation handoff are controlled by plans/registry.md.

## Development

The workspace targets Rust 1.88+ / edition 2024. Run `bash scripts/verify.sh quick` for locked workspace checks, tests, lint, and dependency-boundary validation. Runtime integration is currently verified with scripted in-memory transport; no direct host networking is implemented.
