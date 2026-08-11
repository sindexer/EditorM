# ADR-025: Indexed Viewport Culling and Gate 0D Proof

- Status: Accepted
- Date: 2026-08-09

## Context

FPS is environment-dependent and cannot prove structural batching, culling, or incremental
resource behavior.

## Decision

Camera converts the logical viewport to a finite world Rect. RenderModel queries the Scene
R*-tree, filters exact visible/renderable items, restores structural order, and emits only
visible stable slots. Camera changes alter neither Document nor Scene/Render revision.

Gate proof uses structural counters: candidate, exact visible, culled, submitted instance,
draw call, batch, GPU buffers/bytes, upload calls/bytes, dirty ranges, full/fallback rebuilds,
and validation errors. Native offscreen wgpu proof and actual Chrome hardware proof are stored
separately; no mock or Canvas2D substitute qualifies.

## Consequences

Extreme zoom-out returns all actual visible objects without a typed query-size failure.
Offscreen objects are absent from submitted instances. Gate 0D requires fallback rebuild and
GPU validation counts to remain zero.