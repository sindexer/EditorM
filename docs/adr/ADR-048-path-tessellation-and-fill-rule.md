# ADR-048: Path Tessellation, Fill Rule, and the Path GPU Pipeline

- Status: Implemented for Phase 2A review
- Date: 2026-08-23
- Follows: ADR-043, ADR-024, ADR-022

## Context

Phase 2A's schema checkpoint gave the Document persistent path geometry: ordered anchors with stable identities and node-local absolute Bezier handles. Nothing rendered it. `Geometry::Path(_)` returned `None` from the render model, paths were invisible on the GPU, and hit testing could not pick them.

Making a path visible raises three questions that must be answered once, in one place, or the editor will disagree with itself: how a pair of anchors becomes a curve, what "inside" means for a closed path, and how curves become triangles without re-doing that work on every frame.

Analytic primitives do not answer these questions. A rectangle and an ellipse are signed distance fields evaluated per fragment in the shared WGSL; there is no closed-form SDF for an arbitrary cubic outline, and per-fragment curve solving does not scale.

## Decision

**One segment rule, one geometry module.** `core_math::path` owns the whole answer. A segment between two anchors is a line when neither the outgoing handle of the first nor the incoming handle of the second exists, and a cubic otherwise, with the missing handle defaulting to its own anchor. A closed path adds the last-to-first segment. Bounds, flattening, distance, containment, and tessellation all read the path through this module, so the renderer cannot interpret an anchor pair differently from the hit test.

**Exact bounds.** A path's local bounds are the box the curve actually occupies, computed from the roots of each segment's derivative, not the control-point hull. The hull remains available as `conservative_bounds` for callers that want a cheap outer box. This changes what selection, culling, and the Inspector report for a curved path, and it is the reason the Phase 2A schema test now asserts curve bounds rather than hull bounds.

**Even-odd, on the flattened outline.** The fill rule is fixed as even-odd, evaluated on the same flattened polyline the GPU receives. CPU hit testing and GPU triangles therefore agree by construction rather than by coincidence. A self-intersecting outline is refused rather than guessed: it produces no fill triangles and no interior hits, because Phase 2A will not claim a region it does not draw. Open paths never fill.

**Tessellation is derived, cached, and keyed by silhouette.** The Document stores anchors and nothing else. `render_model::path_tessellation` converts a path into a triangle list, cached per node and keyed by the geometry plus only the appearance that changes the silhouette: stroke width, and whether fill and stroke are visible at all. Colour, opacity, the node transform, and the camera are not in the key, so panning, zooming, and recolouring never re-tessellate. The flatten tolerance is a fraction of the path's own size, so a cached tessellation is camera-independent.

**Antialiasing is a screen-space feather, not MSAA.** Every vertex carries an outward normal and a coverage value: interior vertices have a zero normal and full coverage, feather vertices carry a unit outward normal and zero coverage. `vs_path` expands feather vertices by one pixel in screen space, so a single cached tessellation stays antialiased at every zoom level, with no multisampling and no discarded fragments.

**Paths get their own pipeline and their own compact index space, without disturbing primitives.** Render binary schema v3 adds an 80-byte path instance record and a 32-byte path vertex record. Path items never enter the instanced quad draw: they carry a compact `path_index` instead of appearing in `visible_slots`, so a document of rectangles pays nothing for path support and the Phase 1 fast path is untouched. Culling emits ordered draw batches — runs of instanced primitives and runs of path vertices, bottom to top — so paths and primitives keep their scene z-order across a pipeline switch, and adjacent paths collapse into one draw call.

**Uploads follow revisions, not frames.** The render model exposes a path vertex revision that moves only when some path's triangles change. Both GPU consumers re-send the vertex buffer only when that revision moves, so a camera-only frame uploads no triangles at all.

## Consequences

A path is now a first-class rendered object across Document → ComputedScene → RenderModel → Worker → WASM → WebGPU, and the browser never computes path geometry: React remains an interaction overlay.

The cost is a CPU tessellation step that primitives do not have. One geometry edit re-tessellates exactly one path; a static frame and a camera frame re-tessellate none. Building 1,000 curved closed paths from a cold document costs roughly 90 ms of tessellation on the measured machine — a real one-time cost, recorded as debt rather than hidden.

Even-odd was chosen over non-zero because it is decidable on a flattened polyline without winding bookkeeping and matches what the triangulator can honestly produce. If a later phase needs non-zero fills or self-intersecting shapes, it replaces the triangulator and the containment test together, in this one module, and both consumers follow.

**This is a Phase 2A limitation, not the final vector fill capability.** Refusing to fill a self-intersecting closed path is a deliberate scope boundary for this checkpoint, not a statement about what the engine can eventually draw. General polygon resolution — self-intersection resolution, non-zero winding, and boolean operations over paths — is later-phase work. Until then the rule is that the hit test and the renderer agree, even when that means both decline.

Stroke joins, caps, miter limits, and dashes are deliberately absent: Phase 2A strokes each segment as a quad with a feather on both sides. That is enough to draw and pick a stroked path, and it is honest about what Phase 2B still owes.
