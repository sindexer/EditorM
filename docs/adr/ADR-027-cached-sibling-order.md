# ADR-027: Cached Sibling Order for Candidate Sorting

Status: Accepted for Phase 0D-R1

## Context

The prior render representation recomputed dense structural order for every persistent change.
Scene z-order comparison also searched a parent's complete child list to find each sibling
position. Fully visible flat hierarchies therefore repeated linear searches during sorting, and
transform-only edits performed unrelated order work.

## Decision

- RenderItem no longer duplicates a dense whole-document `order_key`.
- Every attached Scene node caches its sibling index. A hierarchy or reorder change refreshes
  indexes only for the affected parent's child list; transform, geometry, appearance, and
  visibility changes perform zero order refreshes.
- Candidate z-order paths are built from cached sibling indexes in O(depth). Candidate sorting
  therefore performs no linear sibling-position lookup.
- Persistent Document child order remains authoritative. Cached indexes are disposable derived
  data and are rebuilt from Document order after a full Scene rebuild.
- `order_nodes_visited` and `sibling_search_steps` make the selected strategy observable. The
  Phase 0D-R1 gate requires `sibling_search_steps == 0` for the flat 10k proof.

## Consequences

Reorder cost is proportional to the affected sibling list, while non-hierarchy updates avoid
order maintenance entirely. Candidate sorting still costs according to candidate count and
depth, but no comparison rescans sibling arrays. Undo and redo preserve the same topmost result
because they continue to mutate the authoritative Document child order.

