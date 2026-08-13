# ADR-031: Atomic Reversible Group and Ungroup Compound Commands

- Status: Accepted for Phase 0E
- Date: 2026-08-10

## Context

Implementing Group or Ungroup as a sequence of React-side reparent calls can leave a partially changed hierarchy on validation or numeric failure, create several history entries, and expose intermediate RenderModel states.

## Decision

`Group` and `Ungroup` are Rust core commands. They validate stable IDs, parent compatibility, uniqueness, lock state, insertion order, and world-preserving transforms before committing. Execution uses a cloned candidate Document, and only a completely successful candidate replaces the live state.

Their reversible effect is a compound effect that records the exact structural and transform changes needed for one-step undo/redo. Child order and world transforms are preserved. The Worker protocol exposes each operation as one typed command; React never synthesizes it from multiple mutations.

## Consequences

- Group/Ungroup are atomic and each creates one history entry.
- Failed operations do not partially change hierarchy or transforms.
- Nested group editing can use the same stable IDs and projection-delta boundary.
