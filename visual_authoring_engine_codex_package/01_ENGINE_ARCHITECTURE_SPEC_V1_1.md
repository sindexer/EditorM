# ENGINE ARCHITECTURE SPEC V1.1

## 1. Product definition

The product is a professional visual authoring engine for freeform design, broadcast graphics, structured graphics, data-driven graphics, motion graphics, and AI-controlled editing.

The long-term product should combine:

- freeform editing freedom;
- semantic graphic objects;
- non-destructive structure;
- data binding;
- motion;
- professional export/interop;
- deterministic automation.

A generated object must remain manually editable. Automation must not destroy editability.

## 2. Foundational separation

The following are separate concepts and must remain separate in code:

`Document Model != Editor Session != Computed Scene != Render Representation != GPU Resources != UI State`

### Document Model
Persistent semantic source of truth. Contains nodes, hierarchy, stable identity, transforms, appearance data, and future semantic graphic structures.

### Editor Session
Ephemeral editor state such as current selection, active tool, transient transform session, hover state, viewport/camera state, and current interaction transaction.

### Computed Scene
Runtime-derived representation used to resolve world transforms, effective visibility, bounds, derived geometry, dirty propagation, and eventually layout/effects.

### Render Representation
Renderer-facing primitives and batches. It may be recreated without mutating or redefining document semantics.

### GPU Resources
Buffers, textures, pipelines, bind groups, caches, atlases, device resources, and backend-specific state.

### UI State
Panel expansion, inspector tabs, menu state, dialogs, and other React presentation concerns.

No layer may become the accidental source of truth for another.

## 3. Ownership rule

Every subsystem has one primary responsibility.

- Document owns persistent semantics.
- Editor owns editing behavior and session state.
- Commands own valid document mutation requests.
- Transactions own interactive atomicity.
- History owns reversible committed operations.
- Scene owns computed runtime state.
- Spatial subsystem owns spatial queries/indexes.
- Renderer owns render preparation and GPU-facing state.
- UI owns presentation and user-facing controls only.

Subsystems communicate through explicit public interfaces. A subsystem may not inspect another subsystem's internals simply because language visibility permits it.

## 4. Primary technology direction

### Application shell
- React
- TypeScript
- Vite or equivalent modern bundler

React is for UI. It is not the graphics document model and not the canvas object model.

### Core
- Rust

Rust hosts the long-lived editor engine foundation: math, document, commands, history, scene computation, spatial structures, hit testing, serialization, render preparation, and later geometry/layout subsystems.

### Web runtime
- WebAssembly
- Dedicated Worker where viable

The engine should be hostable outside the browser later. Browser APIs must not contaminate the pure document/math crates.

### GPU
- WebGPU as the primary web backend.
- `wgpu` is the preferred abstraction candidate unless a documented ADR demonstrates a superior option.
- WebGL2 fallback may be added later behind a backend boundary; it must not change document semantics.

### Precision
- Document and editor transform calculations: `f64` by default.
- GPU payloads may use `f32` when appropriate.

## 5. Repository topology

Preferred baseline:

```text
/
├─ apps/
│  └─ web/
├─ crates/
│  ├─ core_math/
│  ├─ document/
│  ├─ commands/
│  ├─ history/
│  ├─ scene/
│  ├─ spatial/
│  ├─ editor/
│  ├─ render_core/
│  ├─ render_wgpu/
│  ├─ serialization/
│  └─ wasm_bridge/
├─ packages/
│  ├─ protocol/
│  └─ ui/
├─ benchmarks/
├─ tests/
├─ docs/
└─ tools/
```

Phase 0A may create only crates required by its scope, but paths and dependencies must remain compatible with this direction.

Avoid cyclic crate dependencies. Keep pure math and persistent document types usable without the renderer or browser runtime.

## 6. Document model

Initial node kinds:

- Document
- Frame
- Group
- Rectangle
- Ellipse

The schema must be extensible toward:

- Vector
- Text
- Image
- Video
- Mask
- BooleanGroup
- Component
- Instance
- Chart
- Table
- Map
- DataContainer
- Adjustment
- Effect

A node minimally needs stable identity, name, kind, hierarchy, transform, visibility, lock state, geometry/appearance payload boundaries, and metadata/extensibility boundaries.

Node identity must not rely on array index, pointer address, traversal index, or renderer resource index.

IDs survive save/load round trips.

## 7. Hierarchy invariants

Always enforce:

- every node ID is unique;
- a node has at most one parent;
- parent and child references are consistent;
- a node may not become its own descendant;
- the document tree contains no cycles;
- child order defines sibling z-order;
- deletion/reparenting cannot leave dangling parent-child relationships.

In debug/test builds, provide invariant validation helpers.

## 8. Coordinate system

Document logical unit:

`1 unit = 1 logical pixel`

Axis convention:

- +X right
- +Y down

Spaces:

- Local Space
- World Space
- Screen Space

Camera/viewport transform is not a node transform.

## 9. Transform model

Transforms are matrix-based, not merely a bag of x/y/width/height fields.

Core relation:

`WorldTransform = ParentWorldTransform * LocalTransform`

Phase 0 foundation must support or be structurally compatible with:

- translation;
- non-uniform scale;
- rotation;
- flip/reflection;
- nested transform composition;
- world/local conversion;
- reparent while preserving world transform.

Future compatibility:

- skew;
- custom pivot;
- transform origin manipulation;
- animated properties.

Bounds must be conceptually separated:

- Geometry Bounds
- World Bounds
- Visual Bounds

Visual Bounds may be deferred until effects/strokes exist.

## 10. Mutation architecture

Persistent document mutation must occur through a command boundary once the command subsystem exists.

UI code must never expose a mutable document object graph that can be casually changed.

Long-term flow:

```text
UI / AI / Plugin
→ typed command
→ validation
→ transaction
→ document mutation
→ dirty propagation
→ history
→ derived scene update
→ render update
```

This command model becomes the future deterministic AI editing API.

## 11. Transaction architecture

Interactive editing such as drag, resize, rotate, slider scrubbing, and inspector nudge must support:

- begin;
- update;
- commit;
- rollback.

A single drag must become one undoable committed operation, not hundreds of history entries.

Escape/cancel must be able to rollback the transient interaction.

## 12. History architecture

History is command/transaction oriented.

Required direction:

- Undo
- Redo
- branch invalidation after new edits
- transaction coalescing
- future checkpoints/snapshots as optimization, not the only semantic model

Do not use full document deep-copy per pointer event as the foundation.

## 13. Scene derivation

The document is persistent semantic data. The computed scene is runtime data.

Dirty categories should evolve from:

- TransformDirty
- GeometryDirty
- AppearanceDirty
- HierarchyDirty

Toward:

- LayoutDirty
- TextDirty
- EffectDirty
- AnimationDirty

A small change must not require unconditional whole-document rebuilds.

## 14. Camera and infinite canvas

World coordinates are not constrained to the viewport or one fixed artboard.

Camera owns:

- position;
- zoom;
- viewport dimensions;
- device pixel ratio.

Long-term required interactions:

- pan;
- zoom;
- zoom around pointer;
- fit selection;
- fit document/frame.

## 15. Spatial architecture

All pointer operations must not scale by scanning every node.

Provide an abstraction such as:

```text
SpatialIndex
- insert
- remove
- update
- query_point
- query_rect
```

Implementation may use R-tree, BVH, or another justified data structure.

The critical requirement is that the real hit-test and viewport-query hot paths use it.

## 16. Hit testing

Pipeline:

```text
screen pointer
→ world conversion
→ spatial candidate query
→ geometry-aware test
→ z-order resolution
→ editor selection target
```

Geometry-aware means an ellipse is not considered hit merely because the pointer lies in its bounding rectangle.

## 17. Selection and direct manipulation

Selection is editor-session state, not persistent document semantics.

Long-term direct manipulation:

- single selection;
- shift multi-selection;
- marquee selection;
- selection bounds;
- move;
- resize;
- rotate;
- nested selection;
- group/ungroup;
- unified multi-selection transform;
- Figma-like precision behavior.

Selection overlays and handles are editor overlays, not document nodes.

## 18. Snapping

Keep snapping as a dedicated subsystem rather than hard-coding it inside transform math.

Future candidates:

- edges;
- centers;
- frames;
- guides;
- pixel grid;
- equal spacing;
- smart guides.

Proposed flow:

`Transform Session → Snap Resolver → Proposed Transform → Command/Transaction`

## 19. Rendering architecture

Renderer consumes a computed/render representation, not arbitrary editor state.

Long-term:

```text
Document
→ Computed Scene
→ Render Items
→ GPU Batches
→ Backend
```

Renderer does not own document truth.

Primary backend: WebGPU through `wgpu` or a superior documented abstraction.

Fallback backend is allowed only behind a backend interface.

## 20. Batching and culling

Renderer must be designed for large documents.

Avoid one high-level draw operation per document node when batching/instancing is possible.

Viewport culling must exclude offscreen nodes from active render submission.

A 100k-node document with a small visible subset must not resubmit and reprocess all 100k nodes every frame simply because they exist.

## 21. Dirty updates

Changing one node should update affected data only.

Instrumentation must make it possible to measure:

- document nodes visited;
- scene nodes recomputed;
- spatial entries updated;
- render items regenerated;
- GPU instances/buffers updated.

This prevents a fake dirty-flag architecture that still rebuilds everything.

## 22. WASM boundary

Prefer coarse-grained interfaces.

Avoid chatty JS ↔ WASM getter/setter APIs such as repeated `getNodeX()` / `setNodeX()` calls.

Preferred concepts:

- dispatch command;
- submit input batch;
- retrieve UI projection/snapshot;
- retrieve performance/debug metrics.

React must not manage the Rust object graph.

## 23. Worker boundary

Recommended:

Main thread:
- React;
- DOM;
- panels;
- menus;
- keyboard routing;
- accessibility-facing UI.

Worker:
- Rust/WASM engine;
- scene computation;
- spatial index;
- render system where browser capability permits.

Engine core must not depend on worker semantics. Use an Engine Host abstraction.

## 24. Input pipeline

Never mutate the document directly from raw browser pointer events.

Long-term flow:

`PointerEvent → Input Adapter → Engine Input → Tool/Interaction State Machine → Transaction/Command`

Use explicit interaction states rather than an unbounded collection of booleans.

## 25. Serialization

The file schema is versioned and distinct from Rust's incidental in-memory representation.

Baseline envelope:

```json
{
  "format": "visual-authoring-document",
  "version": 1,
  "document": {}
}
```

Save → load → save must preserve semantic equality and stable node IDs.

Provide migration boundaries from the beginning.

## 26. Semantic graphics

Future domain objects include:

- BarChart
- ElectionGraphic
- LowerThird
- Ticker
- Table
- ProfileCard
- MapGraphic

A semantic object is manipulable as a structured object and can later be detached into ordinary graphic primitives.

Principle:

`Structured Automation + Freeform Editing`

## 27. Motion architecture

Future animateable data should be compatible with a `Property<T>` abstraction:

- Static(T)
- Animated(keyframes)

Potential animated targets:

- position;
- scale;
- rotation;
- opacity;
- color;
- path;
- blur;
- text attributes.

Do not encode transform data in a way that prevents later animation.

## 28. AI architecture

AI never writes pixels directly as the core editing mechanism.

Correct flow:

`Natural language → structured editor command(s) → document → scene → renderer`

AI commands must be deterministic, validated, inspectable, undoable, and ideally replayable.

## 29. Professional interop

PSD and After Effects are future bridges/export targets, not the editor's source of truth.

The same semantic document should support future mapping to:

- PSD layer structure;
- After Effects comps/layers/properties;
- SVG/PDF;
- raster/video rendering;
- server rendering.

## 30. Non-goals of Phase 0 foundation

Do not prematurely implement:

- full text engine;
- Bezier editor;
- Boolean operations;
- masks;
- image editing;
- collaboration/CRDT;
- cloud backend;
- AI UI;
- charts;
- broadcast templates;
- plugin ecosystem;
- production motion timeline.

Foundation correctness comes first.
