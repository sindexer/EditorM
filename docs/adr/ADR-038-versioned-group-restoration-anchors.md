# ADR-038: Versioned Internal Group Restoration Anchors

- Status: Implemented for Phase 0E-R2 review
- Date: 2026-08-11
- Refines ADR-031 and ADR-004

## Context

R1 stored absolute original indices in user `NodeSpec.metadata` under `__phase0e_r1_group_positions`. That namespace could collide with user content, exposed an engine implementation detail, and became stale when siblings were inserted, deleted, or reordered while the group existed.

## Decision

A Group node has a separate optional `GroupRestoration` record with an explicit version. For each selected child in original order, the record stores the nearest unselected `before_anchor` and `after_anchor` IDs. The record is not accessible through user metadata.

Ungroup resolves a surviving before anchor first, otherwise a surviving after anchor, otherwise the current group rank. Planned ranks are monotonically advanced so selected children retain their relative order. This produces a deterministic result when siblings are inserted, deleted, or reordered. Validation of the version, child list, anchors, transforms, and placement completes before the reversible structural effect mutates the Document.

Persistence maps the record to the versioned `internal_group_restoration` field. Explicit save/load remains a full serialization boundary. Undo/redo carries the same internal record inside the reversible effect, so immediate Group/Ungroup and history round trips restore byte-equivalent semantic snapshots.

## Memory and consequences

The record stores O(k) child and anchor IDs per live group. Resolution costs O(k log N) rank lookups plus O(k²) ordering work over k only; for the authorized k=3 proof it is independent of parent size. Missing anchors degrade deterministically to the group rank and never to a stale absolute index.

Rejected alternatives were user metadata, absolute indices, globally mutable fractional labels, and retaining a full copy of the original parent sequence.
