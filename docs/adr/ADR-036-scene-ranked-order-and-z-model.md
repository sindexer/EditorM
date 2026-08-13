# ADR-036: Scene Ranked Order and Z Model

- Status: Implemented for Phase 0E-R2 review
- Date: 2026-08-11
- Supersedes the dense cached-order detail in ADR-027

## Context

Caching dense `sibling_index` or `order_key` on every Scene node makes insertion near the front require rewriting unaffected siblings. Avoiding those rewrites without changing the representation leaves stale rank and topmost ordering.

## Decision

Scene parents use the same ranked `OrderSequence` model as Document. Structural changes detach, attach, group, and ungroup only the affected nodes through tracked split/merge operations. Runtime Scene nodes no longer cache a dense sibling index or structural order key.

Logical rank is derived from the current parent sequence when comparison or a semantic Scene snapshot needs it. Hit testing and topmost ordering use live ancestry/rank, so selected and unselected overlapping siblings remain consistent with Document after Group/Ungroup, undo/redo, persistence, and sibling edits. A semantic snapshot may materialize children and sibling ranks because it is an explicit proof boundary; the edit path may not.

Scene work counters are independent from Document counters and record the same nine sequence costs plus full Scene and fallback rebuild counts. Incremental structural updates require both rebuild counts to remain zero. RenderModel full scans, clones, GPU dirty slots, and uploads remain zero when hierarchy changes do not change visual instances.

## Memory and consequences

Scene pays one ranked-sequence node and ID locator per structural edge. Rank queries become expected O(log N); k-node structural changes are O(k log N), excluding ancestor depth. No O(N) dense index rewrite is necessary.

Rejected alternatives were cached dense indices, sparse numeric order keys that eventually rebalance, and rebuilding a complete Scene order after each structural patch.
