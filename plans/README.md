# i2pr-mail Planning System

This planning layout follows the CodeGG convention: stable direction is separated from operational implementation plans and evidence-based closure records.

## Canonical documents

- 000-long-term-specification.md — stable end-state product and security invariants.
- 001-terminology-and-domain-model.md — normative vocabulary and identity/state model.
- 002-long-term-roadmap.md — dependency-ordered capability roadmap.
- 003-planning-process.md — lifecycle, status, registration, and closure rules.

Ordinary implementation work must not silently rewrite canonical direction.

## Directory roles

- adrs/ — durable architectural decisions; accepted ADRs are superseded rather than rewritten.
- subsystems/ — subsystem roadmaps spanning multiple milestones.
- implementation/ — bounded milestone plans suitable for handoff.
- closure/ — evidence records deciding whether a milestone actually closed.
- archive/ — superseded or historical interim plans.
- registry.md — compact active control surface.

## Required work classification

Every roadmap and implementation plan distinguishes invariant, capability, infrastructure, and polish. Infrastructure is not a completed product capability until a consumer path and acceptance evidence exist.

## Planning lifecycle

1. Identify applicable canonical requirements.
2. Record durable unresolved architecture decisions as ADRs.
3. Create or update the subsystem roadmap.
4. Write one bounded implementation plan per milestone.
5. Register dependency-ready work in registry.md.
6. Implement and verify.
7. Write a closure record.
8. Update the roadmap and registry, including an unblock audit.
9. Archive obsolete interim documents without rewriting history.