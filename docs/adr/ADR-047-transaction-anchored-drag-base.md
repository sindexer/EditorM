# ADR-047: Transaction-Anchored Drag Base for Multi-Node Translation

- Status: Implemented for Phase 1B review
- Date: 2026-08-22
- Follows: ADR-046

## Context

Phase 1A dragged one node by sending an absolute transform computed in React from the node's transform at pointer-down. With several selected nodes the UI would have to hold and re-derive a transform per node, and any coalesced or replayed pointer frame that sent a relative delta instead would accumulate drift. Snapping makes this worse, because the applied delta differs from the requested delta.

## Decision

When a transaction opens, the EngineHost captures a drag base: for every selected node, its local transform and its parent's world transform, plus the union of the selection's world bounds. Nodes that are locked, or inside a locked container, are skipped and counted rather than failing the drag.

`translate_selection` takes one world delta measured from that base. The host resolves snapping against the base union bounds, converts the applied delta into each node's parent space, and applies one `SetLocalTransform` per node through the batched transaction path. Every frame therefore recomputes absolute positions from the same anchor: repeating, coalescing, or dropping pointer frames cannot drift, and the final position depends only on the last delta received.

The response reports the requested delta, the applied delta, whether snapping moved it, the guides to draw, and how many nodes moved. A request that arrives without an active transaction or without a captured base is a typed failure, not a silent no-op. A failure inside the batch rolls the transaction back: the drag is cancelled rather than leaving part of the selection moved.

Commit and rollback both clear the base, so a drag can never reuse geometry from a previous one.

New request types are additive: existing request shapes and response fields are unchanged, so the request protocol stays at version 1 and the render binary schema is untouched at version 2.

## Consequences

The editor sends one request per drag frame regardless of selection size, and holds no per-node transform state. The engine owns drag geometry, so the same path will serve future AI-issued moves. The cost is one bounded snapshot per transaction, proportional to selection size.
