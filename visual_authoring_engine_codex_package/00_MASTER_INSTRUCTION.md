# PROFESSIONAL VISUAL AUTHORING ENGINE — CODEX MASTER INSTRUCTION

Version: 1.1  
Status: Authoritative project instruction  
Execution gate: Phase 0A only  

## 1. Authority and precedence

This package defines the architecture and execution rules for a professional GPU-accelerated visual authoring engine. Treat these documents as project constraints, not as suggestions.

Precedence when requirements appear to conflict:

1. `00_MASTER_INSTRUCTION.md`
2. `01_ENGINE_ARCHITECTURE_SPEC_V1_1.md`
3. `02_ENGINE_DEVELOPMENT_RULES.md`
4. `03_UI_WANTED_DESIGN_SYSTEM_SPEC.md`
5. Current phase task file, beginning with `05_TASK_PHASE0A.md`
6. Review and benchmark specifications
7. Implementation convenience

Do not silently reinterpret a higher-priority document to satisfy a lower-priority one.

## 2. Project objective

Build the foundation for a professional visual authoring engine with the long-term target of:

- Figma-class or better direct manipulation freedom.
- Photoshop-class layer/compositing extensibility.
- After Effects-class animatable property and motion architecture.
- Semantic, data-driven broadcast graphics.
- Deterministic AI-native editing through an editor command API.
- PSD and After Effects interoperability without making either application the source of truth.

This is not a Figma clone, a canvas demo, or a wrapper around a JavaScript drawing library. It is an editor engine.

## 3. Architecture-first rule

Architecture requirements are mandatory constraints, not implementation suggestions.

Completing more features with an incorrect architecture is considered failure.

If an architectural requirement conflicts with implementation speed, choose architecture.

If a required subsystem cannot be implemented correctly:

1. Stop implementation of that subsystem.
2. Document the blocker.
3. Leave it incomplete.
4. Do not substitute a materially different architecture merely to make the demo work.

If a foundational architecture is discovered to be incorrect, rewrite it. Do not preserve incorrect legacy code merely to reduce code changes.

## 4. Current execution limit

Read the entire package for architectural context, but implement **Phase 0A only**.

Do not proceed to Phase 0B, 0C, 0D, or 0E until an external architecture review explicitly authorizes continuation.

At the end of Phase 0A:

- run all required tests;
- create `REVIEW_PACKET_0A.md`;
- record deviations and blockers;
- stop.

## 5. UI design authority

The supplied file:

`reference/wanted-design-system/Wanted Design System (Community).fig`

is the visual design and component reference for the editor UI.

When UI implementation begins in Phase 0E and later phases:

- follow its visual language, spacing logic, typography hierarchy, component patterns, surface treatment, control density, border/radius conventions, interaction-state treatment, and component composition;
- reuse or faithfully reproduce its components where applicable;
- do not substitute an unrelated design system such as Material UI, Ant Design, Bootstrap, Chakra, shadcn defaults, or a generic AI-generated dashboard aesthetic;
- do not treat the attached system as a mood board; it is a design-system reference.

Detailed UI rules are in `03_UI_WANTED_DESIGN_SYSTEM_SPEC.md`.

Phase 0A does not implement UI, but no architecture decision may make faithful adoption of the Wanted Design System difficult later.

## 6. Required behavior from Codex

Before editing code:

- inspect the repository;
- read every document in this package;
- identify whether an existing implementation conflicts with the architecture;
- prefer correction or rewrite over compatibility shims when the foundation is wrong.

During work:

- keep architectural boundaries explicit;
- add tests while implementing foundational behavior;
- avoid hidden global mutable state;
- record meaningful design decisions as ADRs;
- do not claim a subsystem exists unless it is on the real execution path.

At completion:

- do not start the next phase;
- provide exact commands to build, test, and inspect results;
- produce the required review packet.
