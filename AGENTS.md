# AGENTS.md

## Purpose

i2pr-mail is a Rust backend-first mail client for I2P in-network mail. The initial target is the Postman-style POP3/SMTP service used by Susimail, with later integration as a managed native application for i2pr.

## Planning authority

- plans/registry.md is the active planning control surface.
- plans/000-long-term-specification.md defines stable product and security invariants.
- plans/001-terminology-and-domain-model.md defines project terminology and state vocabulary.
- plans/002-long-term-roadmap.md defines dependency ordering.
- plans/003-planning-process.md defines plan lifecycle and status rules.
- plans/subsystems/ contains durable workstream roadmaps.
- plans/implementation/ contains bounded implementation handoffs.
- plans/closure/ contains immutable evidence-based closure records.
- plans/adrs/ contains durable architecture decisions.

Before implementing a milestone, read its source roadmap and the active row in plans/registry.md. Do not mark a milestone complete merely because code landed; closure requires the evidence named by its implementation plan.

## Architectural constraints

- The backend is frontend-neutral. No GUI toolkit belongs in core crates.
- Direct clearnet, LAN, loopback, and generic host networking are not part of the secured product path.
- POP3 and SMTP protocol logic should remain deterministic and testable without opening sockets.
- i2pr integration is an adapter boundary, not an excuse to couple mail-domain or mail-proto to router internals.
- Raw mail and credentials are sensitive. Never log passwords, authentication payloads, full message bodies, or attachment bytes.
- Credentials must not be persisted in ordinary configuration or mailbox metadata.
- Outbound mail must not leak local hostname, OS username, router version, i2pr-mail version, machine architecture, or local timezone through generated headers.
- POP3 UIDLs are opaque remote identifiers. Message ordinals are session-local and must not become persistent identity.
- SMTP transport failure after DATA may make delivery ambiguous; never turn that state into an automatic blind retry.
- No HTML renderer may independently fetch remote resources.

## Initial Rust policy

The initial workspace should track the compatible i2pr toolchain floor: Rust 1.88+ and edition 2024 at the planning baseline. Advancing MSRV is allowed only as an intentional repository decision.

Prefer narrow verification first, then the repository verification script. The foundation plans establish the exact commands and dependency-boundary checks.