# ADR-044: Frame Creation and Preset Semantics

- Status: Implemented for Phase 1A review
- Date: 2026-08-14
- Follows: ADR-043

## Context

A professional editor needs a visible artboard rather than an abstract rectangle. Frame already exists as a persistent node and geometry kind, but Phase 1A must define creation, naming, placement, editing, and default-project behavior without claiming transform-aware clipping.

## Decision

The editor loads a dedicated Editor fixture with one selected-capable, visible Frame 1920×1080 centered at world origin. Historical Phase 0 Preview and benchmark fixtures remain unchanged.

The Frame tool uses shortcut F. Its preset menu offers 1920×1080, 3840×2160, 4096×2160 DCI 4K, and finite positive custom dimensions. Preset frames are centered on the current camera center, selected after creation, shown as Frame nodes in Layers, and editable in Inspector. Duplicate preset names receive deterministic numeric suffixes. Drag creation, direct move, resize, fit selection, undo, and redo reuse the typed engine command and transaction paths.

Frame is a renderable rounded-rectangle primitive with appearance, bounds, culling, stable slots, and GPU deltas. It is not downgraded to rectangle metadata. Phase 1A does not claim nested or rotated Frame clipping; CSS overflow and axis-aligned clipping are prohibited substitutes.

## Consequences

tools/start-editor.ps1 provides the human entry point. The default product editor can be evaluated independently from immutable Phase 0 proof fixtures. Future transform-correct clipping can extend Frame semantics without changing its identity or preset behavior.
