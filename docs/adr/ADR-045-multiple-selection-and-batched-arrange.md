# ADR-045: Multiple Selection and Batched Transactional Arrange

- Status: Implemented for Phase 1B review
- Date: 2026-08-22
- Follows: ADR-044

## Context

Phase 1A moved and edited one node at a time. Phase 1B adds multiple selection with alignment and distribution. The obvious shortcut is to compute new positions in React from projected bounds and then send one transform command per node. That would place graphics logic in UI code, produce one history entry per node, and leave the document half aligned whenever a command in the middle of the sequence fails.

## Decision

Selection remains ephemeral editor state owned by the engine. `select_many` and `extend_selection` validate every requested ID before any selection state changes, so a rejected request leaves the previous selection intact. Rubber-band selection resolves through the scene's spatial index and returns only *top-level* nodes of the active root: a band crossing a group selects the group, matching what clicking one of its members does.

Alignment and distribution are planned in the runtime, not in the UI. Planning reads world bounds from the computed scene and emits typed `SetLocalTransform` commands. A world translation `d` for a node whose parent has world transform `P` becomes the parent-local translation `P_linear^-1 * d`, so rotation and scale are never disturbed. Planning is a pure function: it touches no document, scene, render, history, or selection state, and it reports how many targets it examined and how many needed no movement.

Applying a plan opens one transaction, applies every command, and commits. Any failure inside the batch rolls the whole transaction back and returns a typed error. One alignment is therefore exactly one undo step, and a locked target rejects the operation instead of aligning the rest.

Rejections are typed and deterministic: fewer than two targets for alignment, fewer than three for distribution, duplicate targets, detached targets, the document root, unresolved bounds, a non-invertible parent, and a selection that contains both a container and something inside it. The last rule exists because such a target would otherwise be moved twice by one operation.

Distribution equalizes edge-to-edge gaps and keeps the extreme targets in place. Overlapping input yields a negative gap rather than a rejected request, which keeps the operation total.

Several commands applied in one request still owe consumers one description of what changed, so `DocumentChangeSet::merge_consecutive` merges the per-command sets and rejects any chain that is not contiguous, and render deltas are unioned into a single dirty-slot upload with recomputed ranges.

## Consequences

The same planner serves the toolbar today and the AI command layer later: both produce typed commands against one engine. UI code cannot silently become the source of layout truth, because it never computes a transform. The cost is that a single locked or invalid target fails a whole arrange request; the typed error names the offending node so the editor can explain it.
