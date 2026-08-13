# ADR-033: Bounded Structural Patches and UI Projection Schema V2

- Status: Accepted for Phase 0E-R1
- Date: 2026-08-10
- Supersedes the clone-based implementation detail in ADR-031

## Context

The initial Group/Ungroup implementation validated atomically by cloning the full Document. Its change expansion could then force a Scene fallback, clone RenderModel data, serialize a parent's complete child list, rebuild the React hierarchy, and flatten the full Layers order. A 100K document therefore made a three-leaf structural edit proportional to total document size.

## Decision

Group and Ungroup validate the common parent, target uniqueness and order, locks, transforms, insertion position, and all reversible data before commit. They then apply a bounded Document patch containing the parent, group, selected children, and exact original sibling positions. One reversible structural effect restores the semantic snapshot through undo/redo and save/load.

`DocumentChange::StructuralGroupChanged` is consumed directly by Scene and RenderModel. Unchanged leaf world transforms and z-order produce no GPU instance upload. Scene, Render, and projection fallbacks are errors in the R1 proof rather than recovery paths.

UI projection schema version 2 is independent of the Worker request protocol. It carries explicit attach, detach, move, group, and ungroup operations. Structural deltas do not resend the parent's complete child array. React applies these operations incrementally and virtualizes mounted Layers rows.

## Consequences

- Group and Ungroup create exactly one history entry each.
- Noncontiguous sibling positions round-trip exactly through undo/redo and save/load.
- The measured Scene visit count is 8 for Group and 6 for Ungroup at both 10K and 100K.
- Structural GPU dirty slots and uploads are zero when render meaning is unchanged.
- Initial full snapshots remain supported; structural deltas require projection schema version 2.

