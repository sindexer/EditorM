# ADR-022: Backend-Neutral RenderModel and Stable Slots

- Status: Accepted
- Date: 2026-08-09

## Context

Document is persistent truth and ComputedScene is rebuildable derived geometry. GPU-specific
objects must not enter either semantic layer.

## Decision

`visual_authoring_render_model` derives backend-neutral `RenderItem` records keyed by NodeId.
Each record carries primitive kind, size, f64 world transform, f64 bounds, opacity,
renderability, structural order, and a stable instance slot. A free list reuses deleted slots.
Semantic `DocumentChangeSet` values drive incremental upsert/removal and coalesced dirty ranges.

## Consequences

RenderModel is disposable derived state. It cannot mutate Document/history. A one-node
transform updates one 48-byte instance and does not rebuild Scene, RenderModel, or the full
instance buffer. Full document replacement is an explicit full reset.