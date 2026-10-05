---
name: planning
description: i2pr-mail planning lifecycle for roadmaps, implementation plans, closure records, registry updates, and unblock audits
version: 1.0.0
---

# Planning Process Skill

Use this skill whenever creating, executing, closing, correcting, or registering work under plans/.

Authority order:

1. plans/000-long-term-specification.md
2. plans/001-terminology-and-domain-model.md
3. accepted ADRs
4. plans/002-long-term-roadmap.md
5. subsystem roadmap
6. implementation plan
7. current repository evidence

Current repository evidence may correct operational mechanics but cannot silently weaken canonical invariants.

Statuses: proposed, ready, active, blocked, closing, closed, conditionally closed, superseded, archived.

Every implementation plan must be bounded to one coherent outcome and include failure/restart/security/migration semantics, exact verification requirements, stop conditions, and required closure evidence.

Every closure must audit blocked work and update plans/registry.md if all hard/interface dependencies for a downstream registered plan are now satisfied.

Corrective work receives a new plan and closure record; historical closure evidence is not rewritten to hide defects.