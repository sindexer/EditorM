# MASTER CONSOLIDATED SPEC

This file consolidates the authoritative text documents for single-file reading. Individual source documents remain authoritative according to the precedence defined in `00_MASTER_INSTRUCTION.md`.


---

## SOURCE FILE: 00_MASTER_INSTRUCTION.md

# PROFESSIONAL VISUAL AUTHORING ENGINE — CODEX MASTER INSTRUCTION

Version: 1.1  
Status: Authoritative project instruction  
Execution gate: Phase 0A only  

## 1. Authority and precedence

This package defines the architecture and execution rules for a professional GPU-accelerated visual authoring engine. Treat these documents as project constraints, not as suggestions.

Precedence when requirements appear to conflict:

1. `00_MASTER_INSTRUCTION.md`
2. `01_ENGINE_ARCHITECTURE_SPEC_V1_1.md`
3. `02_ENGINE_DEVELOPMENT_RULES.md`
4. `03_UI_WANTED_DESIGN_SYSTEM_SPEC.md`
5. Current phase task file, beginning with `05_TASK_PHASE0A.md`
6. Review and benchmark specifications
7. Implementation convenience

Do not silently reinterpret a higher-priority document to satisfy a lower-priority one.

## 2. Project objective

Build the foundation for a professional visual authoring engine with the long-term target of:

- Figma-class or better direct manipulation freedom.
- Photoshop-class layer/compositing extensibility.
- After Effects-class animatable property and motion architecture.
- Semantic, data-driven broadcast graphics.
- Deterministic AI-native editing through an editor command API.
- PSD and After Effects interoperability without making either application the source of truth.

This is not a Figma clone, a canvas demo, or a wrapper around a JavaScript drawing library. It is an editor engine.

## 3. Architecture-first rule

Architecture requirements are mandatory constraints, not implementation suggestions.

Completing more features with an incorrect architecture is considered failure.

If an architectural requirement conflicts with implementation speed, choose architecture.

If a required subsystem cannot be implemented correctly:

1. Stop implementation of that subsystem.
2. Document the blocker.
3. Leave it incomplete.
4. Do not substitute a materially different architecture merely to make the demo work.

If a foundational architecture is discovered to be incorrect, rewrite it. Do not preserve incorrect legacy code merely to reduce code changes.

## 4. Current execution limit

Read the entire package for architectural context, but implement **Phase 0A only**.

Do not proceed to Phase 0B, 0C, 0D, or 0E until an external architecture review explicitly authorizes continuation.

At the end of Phase 0A:

- run all required tests;
- create `REVIEW_PACKET_0A.md`;
- record deviations and blockers;
- stop.

## 5. UI design authority

The supplied file:

`reference/wanted-design-system/Wanted Design System (Community).fig`

is the visual design and component reference for the editor UI.

When UI implementation begins in Phase 0E and later phases:

- follow its visual language, spacing logic, typography hierarchy, component patterns, surface treatment, control density, border/radius conventions, interaction-state treatment, and component composition;
- reuse or faithfully reproduce its components where applicable;
- do not substitute an unrelated design system such as Material UI, Ant Design, Bootstrap, Chakra, shadcn defaults, or a generic AI-generated dashboard aesthetic;
- do not treat the attached system as a mood board; it is a design-system reference.

Detailed UI rules are in `03_UI_WANTED_DESIGN_SYSTEM_SPEC.md`.

Phase 0A does not implement UI, but no architecture decision may make faithful adoption of the Wanted Design System difficult later.

## 6. Required behavior from Codex

Before editing code:

- inspect the repository;
- read every document in this package;
- identify whether an existing implementation conflicts with the architecture;
- prefer correction or rewrite over compatibility shims when the foundation is wrong.

During work:

- keep architectural boundaries explicit;
- add tests while implementing foundational behavior;
- avoid hidden global mutable state;
- record meaningful design decisions as ADRs;
- do not claim a subsystem exists unless it is on the real execution path.

At completion:

- do not start the next phase;
- provide exact commands to build, test, and inspect results;
- produce the required review packet.


---

## SOURCE FILE: 01_ENGINE_ARCHITECTURE_SPEC_V1_1.md

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


---

## SOURCE FILE: 02_ENGINE_DEVELOPMENT_RULES.md

# ENGINE DEVELOPMENT RULES

## 1. Correctness before feature count

A smaller correct engine is superior to a broader demo with incorrect ownership or hidden shortcuts.

## 2. No architectural substitution

Do not replace required architecture with easier technology solely to complete the task faster.

Examples of prohibited substitutions unless explicitly approved by ADR and review:

- TypeScript document core instead of Rust core;
- Canvas2D instead of the planned GPU renderer for renderer phases;
- SVG DOM as document truth;
- Fabric.js/Konva/Pixi scene objects as document truth;
- React state as persistent document truth;
- main-thread-only engine because worker integration is difficult;
- full-scene rebuild under a nominal dirty-flag API;
- linear full-node hit testing under an unused spatial-index class.

## 3. Library policy

Libraries are allowed and encouraged when they solve generic infrastructure well.

Good candidates for external ownership:

- UUID generation;
- serialization;
- matrix primitives;
- GPU abstraction;
- spatial data structures;
- Unicode/font shaping;
- image decoding.

Project-owned semantics:

- document meaning;
- node model;
- editor behavior;
- command model;
- interaction model;
- scene derivation;
- semantic graphics;
- AI editing API.

For every critical dependency, document:

- why it is used;
- which layer depends on it;
- replaceability;
- data crossing its boundary;
- licensing concerns if relevant.

## 4. No hidden coupling

A subsystem may only communicate through declared interfaces. Do not reach across layers to modify private/internal state because it is convenient.

Renderer must not access history or UI state. UI must not directly own document nodes. Spatial structures must not become semantic storage.

## 5. Rewrite rule

If the foundation is wrong, rewrite it. Avoid compatibility wrappers around incorrect architecture during Phase 0.

## 6. Gate rule

Development sequence:

- Phase 0A: Core foundation
- external review
- Phase 0B: Commands/history/selection foundation
- external review
- Phase 0C: Scene/spatial/hit/camera/dirty
- external review
- Phase 0D: WASM/worker/WebGPU/rendering/batching/culling
- external review
- Phase 0E: React editor shell/direct manipulation/instrumentation
- final Phase 0 review

Read future phase descriptions, but do not implement beyond the authorized gate.

## 7. Proof, not presence

A subsystem is not accepted merely because a class/module exists.

Examples:

- Spatial index must be on the actual hit-test/query path.
- Dirty propagation must measurably reduce recomputation.
- Batching must reduce draw submissions.
- Worker architecture must run engine work in the worker where claimed.
- WebGPU backend must be the real renderer path where claimed.
- Command-only mutation must be enforced by API boundaries, not just conventions.

## 8. Transform quality

Transform correctness is foundational. Add deterministic tests and randomized/property tests for matrix composition and conversion.

Test nested non-uniform scale and rotation, not only translation.

## 9. Testability

Pure math and document logic should be testable without browser/GPU initialization.

Every phase adds its own tests before it is considered complete.

## 10. Debuggability

Expose enough instrumentation to prove behavior in later phases:

- node counts;
- visible counts;
- dirty counts;
- recompute counts;
- spatial candidate counts;
- draw calls;
- batches;
- frame time;
- backend/worker runtime status.

## 11. Error handling

Use typed recoverable errors for invalid operations. Avoid uncontrolled panic for user-editable data.

Examples:

- NodeNotFound
- InvalidParent
- CycleDetected
- LockedNode
- InvalidTransform
- UnsupportedOperation

## 12. Documentation discipline

Maintain ADRs for material decisions.

At each gate produce a review packet containing build instructions, test results, deviations, known issues, important source paths, and architecture risks.


---

## SOURCE FILE: 03_UI_WANTED_DESIGN_SYSTEM_SPEC.md

# UI SPEC — WANTED DESIGN SYSTEM REFERENCE

## 1. Authority

The supplied Figma file:

`reference/wanted-design-system/Wanted Design System (Community).fig`

is the required visual and component reference for the web editor UI.

It is not merely inspiration. It is the baseline design system for editor chrome and application UI unless a specific professional editor interaction requires an extension.

The extracted `thumbnail.png` is included for quick visual identification only. The `.fig` source is authoritative.

## 2. Mandatory implementation intent

When UI implementation begins:

- inspect the Figma reference before writing UI code;
- identify reusable component families and visual tokens;
- derive the editor UI token layer from the reference rather than inventing a new palette or spacing system;
- preserve the recognizable Wanted visual language while adapting it to a dense professional graphics editor.

## 3. What to follow

Match the reference as closely as practical for:

- typography hierarchy;
- type scale relationships;
- font weights;
- spacing rhythm;
- grid/alignment logic;
- corner radius conventions;
- border treatment;
- surface elevation and separation;
- input/button/select patterns;
- icon sizing and alignment;
- default/hover/pressed/focus/disabled states;
- menus;
- dropdowns;
- tabs;
- cards/panels;
- form controls;
- labels and helper text;
- density and padding logic;
- component composition patterns.

## 4. What not to do

Do not:

- substitute Material Design defaults;
- substitute Ant Design defaults;
- substitute Bootstrap defaults;
- substitute Chakra defaults;
- substitute stock shadcn visual defaults without restyling;
- use a generic dark SaaS dashboard appearance;
- invent neon gradients or AI-tool styling unrelated to Wanted;
- copy Figma's visual UI wholesale when it conflicts with Wanted's system;
- use browser-default controls as final UI.

Using a headless behavior library is acceptable if the rendered presentation is restyled to the Wanted system.

## 5. Professional editor adaptation

Wanted Design System is the visual system; Figma/Photoshop/After Effects-class editors are interaction references.

Therefore:

- visual language: Wanted Design System;
- editor interaction model: professional visual authoring conventions;
- engine architecture: this package's architecture spec.

If Wanted lacks a specialized editor component, derive a new component from its tokens and patterns rather than introducing a second unrelated design language.

Examples requiring extension:

- Layers tree;
- transform inspector;
- numeric scrub fields;
- canvas toolbar;
- selection/transform overlay;
- zoom control;
- timeline controls later;
- color/stroke/effect inspectors later.

## 6. Component mapping requirement

Before Phase 0E implementation, create:

`docs/UI_COMPONENT_MAPPING.md`

It must map editor controls to reference components/tokens, for example:

| Editor UI | Wanted reference | Adaptation |
|---|---|---|
| Primary button | corresponding Wanted button | compact editor density |
| Inspector text field | Wanted input | numeric suffix/scrub behavior |
| Dropdown | Wanted select/menu | keyboard navigation retained |
| Toolbar icon button | derived button/icon style | 28-32px dense control |
| Panel tabs | Wanted tabs | editor panel width constraints |

Do not implement Phase 0E production UI until this mapping exists.

## 7. Token extraction requirement

Create a token representation from the reference where feasible:

- colors;
- typography;
- spacing;
- radii;
- borders;
- shadows/elevation;
- interactive state values.

Store the app-facing token representation in a centralized UI token layer. Avoid scattered literal values.

If the `.fig` cannot be programmatically parsed in the implementation environment, document that limitation and inspect the file through available Figma tooling/export. Do not guess token values and claim they came from the source.

## 8. Component-first rule

Do not style every screen independently. Establish reusable components and variants before composing editor panels.

Expected categories eventually include:

- buttons/icon buttons;
- text fields/numeric fields;
- selects/dropdowns;
- tabs;
- segmented controls;
- menus/context menus;
- tooltips;
- dialogs/popovers;
- switches/checkboxes;
- panel headers;
- tree rows;
- badges/status indicators.

## 9. Accessibility and keyboard behavior

Visual fidelity does not justify breaking accessibility.

Use semantic DOM for editor chrome where possible. Support:

- visible keyboard focus;
- keyboard navigation;
- accessible labels;
- sufficient hit targets;
- disabled states;
- predictable tab order.

Canvas-specific interactions may use custom input routing, but the surrounding UI should remain accessible.

## 10. Phase 0A note

Phase 0A does not implement UI. The Wanted design-system file is included now so later UI architecture is not allowed to drift into a different design system.


---

## SOURCE FILE: 04_PHASE0_EXECUTION_PLAN.md

# PHASE 0 EXECUTION PLAN

## Phase 0A — Core foundation

Implement only:

- repository/workspace foundation;
- core math;
- document node model;
- stable IDs;
- hierarchy/invariants;
- transform math;
- serialization/version envelope;
- automated tests.

No renderer, browser, UI, commands, history, or spatial code unless a tiny interface placeholder is essential and contains no implementation logic.

### Gate 0A review
Review architecture before continuing.

## Phase 0B — Mutation and history foundation

Future authorized scope:

- command model;
- command validation;
- transaction model;
- undo/redo;
- selection session state.

No renderer.

### Gate 0B review
Prove all persistent mutations cross the command boundary.

## Phase 0C — Computed scene foundation

Future authorized scope:

- scene derivation;
- dirty propagation;
- spatial index;
- hit testing;
- camera;
- screen/world conversion.

### Gate 0C review
Prove hot paths do not full-scan/rebuild unnecessarily.

## Phase 0D — Runtime and GPU foundation

Future authorized scope:

- WASM bridge;
- Dedicated Worker host;
- WebGPU/wgpu renderer;
- render representation;
- batching;
- culling;
- GPU/resource instrumentation.

### Gate 0D review
Prove real runtime path and performance structure.

## Phase 0E — Editor shell and direct manipulation

Future authorized scope:

- React editor shell;
- Wanted Design System token/component mapping;
- toolbar/layers/inspector/canvas shell;
- direct manipulation;
- debug overlay;
- benchmark UI fixtures.

### Gate 0E review
Validate architecture plus Figma-class interaction direction and Wanted visual-system fidelity.


---

## SOURCE FILE: 05_TASK_PHASE0A.md

# CODEX TASK — PHASE 0A

## Mission

Implement the smallest correct Rust foundation on which the later professional editor can safely be built.

Read all package documents first. Implement **only Phase 0A**.

## Required outputs

### A. Rust workspace
Create/normalize the Rust workspace with clear dependency direction.

Required core crates may include:

- `core_math`
- `document`
- `serialization`

Do not create unnecessary empty crates just to mimic the future tree.

### B. Core math
Implement/test the math primitives required for document transforms.

Required concepts:

- Vec2 or equivalent;
- affine transform/matrix representation;
- translation;
- rotation;
- non-uniform scale;
- matrix composition;
- matrix inversion with safe failure semantics;
- transform point/vector as appropriate;
- approximate comparison helpers for tests.

Use `f64` for document/editor math unless an ADR convincingly justifies an exception.

### C. Node identity
Implement persistent stable IDs.

Requirements:

- unique IDs;
- serializable;
- stable across save/load;
- not tied to index/memory address;
- easy to use in maps/sets.

### D. Document model
Implement initial node kinds:

- Document/root concept;
- Frame;
- Group;
- Rectangle;
- Ellipse.

Keep geometry/appearance boundaries extensible without overengineering future systems.

### E. Hierarchy
Implement safe hierarchy operations needed for the foundation:

- create/register node;
- attach child;
- detach/reparent;
- child ordering;
- lookup by stable ID;
- delete semantics with explicit behavior;
- cycle prevention;
- invariant validation.

Do not expose arbitrary mutable access that makes it trivial for future callers to bypass invariants.

### F. Transform system
Implement local/world transform resolution.

Required tests:

- identity;
- translation;
- rotation;
- uniform scale;
- non-uniform scale;
- nested transforms;
- 10-level nested hierarchy;
- local → world → local round trip;
- reparent preserving world transform;
- negative/flip scale where mathematically valid;
- inversion failure for singular matrices.

Use randomized/property testing where practical. Include combined rotation + non-uniform scale cases.

### G. Bounds foundation
Implement geometry/local bounds and world-space bounds for Rectangle and Ellipse sufficient for later scene/spatial systems.

Do not implement effects/visual bounds.

### H. Serialization
Create a versioned schema envelope.

Example:

```json
{
  "format": "visual-authoring-document",
  "version": 1,
  "document": {}
}
```

Requirements:

- serialize document;
- deserialize document;
- validate hierarchy after load;
- stable IDs preserved;
- semantic round-trip equality test;
- corrupted/invalid parent/cycle data rejected or normalized only according to documented policy.

Do not simply serialize incidental private caches or future renderer state.

## Explicit non-goals

Do not implement:

- React UI;
- browser app shell beyond existing build preservation;
- WASM bridge;
- Worker;
- WebGPU;
- renderer;
- scene graph/runtime scene;
- spatial index;
- hit testing;
- selection;
- command dispatcher;
- history;
- transaction system;
- snapping;
- text;
- vectors;
- AI;
- PSD/AE bridges.

If the existing repository already contains these, do not unnecessarily destroy unrelated work, but do not couple the new Phase 0A core to it. Document any conflict.

## Architecture prohibitions

Failure conditions include:

- renderer types imported into document/core math;
- browser APIs imported into pure Rust document/math crates;
- node identity based on vector index;
- hierarchy mutations that can create cycles;
- transform represented only as x/y/width/height without a real affine matrix foundation;
- serialization of raw renderer/runtime state;
- no tests for nested non-uniform transform composition.

## Required tests

At minimum:

1. Stable ID round trip.
2. Duplicate ID rejection/avoidance.
3. Hierarchy attach/detach.
4. Cycle prevention.
5. Delete behavior.
6. Child order persistence.
7. Local/world transform correctness.
8. World/local round trip.
9. Nested rotation + scale correctness.
10. Reparent preserving world transform.
11. Rectangle/Ellipse world bounds sanity.
12. Serialization semantic round trip.
13. Invalid document rejection.
14. Invariant checker catches deliberately corrupted fixtures if test-only corruption is available.

## ADRs required

Create at least:

- ADR-001 Rust core workspace boundaries
- ADR-002 stable node ID strategy
- ADR-003 transform/matrix representation
- ADR-004 serialization/version strategy

If using third-party math/ID/serialization crates, explain why.

## Completion packet

Create `docs/REVIEW_PACKET_0A.md` following the provided template.

Include:

- exact commit hash if git is available;
- build command;
- test command;
- test results;
- repository tree;
- crate dependency summary;
- important source paths;
- architecture deviations;
- known limitations;
- temporary implementations;
- future risks;
- unresolved questions;
- confirmation that Phase 0B was not started.

## Stop condition

When all Phase 0A tests pass and the review packet is complete:

**STOP. Do not implement Phase 0B.**


---

## SOURCE FILE: 06_REVIEW_PROTOCOL.md

# EXTERNAL REVIEW PROTOCOL

Use this document after Codex completes a gate.

## Review priority

1. Architectural ownership and dependency direction.
2. Correctness/invariants.
3. Whether claimed subsystems are on the real execution path.
4. Tests that would fail if the subsystem were removed/bypassed.
5. Performance structure.
6. Code clarity and extension safety.
7. Feature completeness.

## Phase 0A review checklist

### Repository and dependency direction
- [ ] Pure math has no renderer/browser dependency.
- [ ] Document has no renderer/browser/UI dependency.
- [ ] Serialization does not leak runtime caches.
- [ ] No cyclic dependencies.

### Identity and hierarchy
- [ ] Stable IDs are persistent and not index-based.
- [ ] IDs remain stable across save/load.
- [ ] One parent maximum.
- [ ] Cycle creation is prevented.
- [ ] Child ordering is deterministic/persistent.
- [ ] Reparent/delete semantics are explicit.

### Transform
- [ ] Real affine matrix math exists.
- [ ] Uses appropriate precision.
- [ ] Local/world conversion tested.
- [ ] Nested non-uniform scale + rotation tested.
- [ ] Inverse failure is handled safely.
- [ ] Reparent preserving world transform tested.
- [ ] Camera concerns have not leaked into node transform.

### Serialization
- [ ] Versioned envelope exists.
- [ ] Semantic round trip passes.
- [ ] Invalid hierarchy is handled intentionally.
- [ ] Stable IDs persist.
- [ ] No renderer/UI/session state is serialized as document truth.

### Extensibility
- [ ] Initial Rectangle/Ellipse model does not block future Vector/Text/Image nodes.
- [ ] Data model does not hard-code WebGPU or DOM assumptions.
- [ ] Transform structure remains compatible with future animation.

### Scope discipline
- [ ] Phase 0B+ was not implemented.
- [ ] No hidden UI/renderer shortcut was introduced.

## Critical red flags

Reject the gate if any are present:

- document node objects are renderer objects;
- React state owns document truth;
- node IDs are array indices;
- world transforms are manually accumulated ad hoc in UI code;
- hierarchy can be corrupted by normal public APIs;
- save/load changes node identity;
- tests cover only trivial translation;
- future phases have been built on top of an unreviewed foundation.


---

## SOURCE FILE: 07_BENCHMARK_AND_INSTRUMENTATION_SPEC.md

# BENCHMARK AND INSTRUMENTATION SPEC

This applies primarily to Phase 0C-0E, but architecture should anticipate it.

## Benchmark fixtures

- BENCH-A: 1,000 rectangles
- BENCH-B: 10,000 rectangles
- BENCH-C: 100,000 rectangles distributed through infinite canvas
- BENCH-D: 10,000-node nested hierarchy stress fixture

## Required metrics

Where measurable:

- total document nodes;
- visible nodes;
- culled nodes;
- spatial candidates tested;
- geometry hit tests;
- dirty nodes;
- scene nodes recomputed;
- spatial entries updated;
- render items regenerated;
- GPU instances updated;
- draw calls;
- GPU batches;
- CPU frame time;
- GPU frame time;
- memory;
- runtime backend;
- worker/main-thread execution status.

## Structural performance acceptance

Do not reduce quality to one FPS target. The more important conditions are:

- spatial queries are active;
- culling is active;
- dirty update is active;
- batching is real;
- one-node edits do not cause unconditional full-document rebuilds;
- 100k-node documents do not imply 100k render submissions every frame when only a small subset is visible.

## Proof fixtures

### Dirty proof
With 10,000 nodes, change one isolated node and report visited/recomputed/updated counts.

### Spatial proof
Compare candidate count against total count. A point query in a 100k-node document should not geometry-test 100k nodes under normal distribution.

### Batch proof
Report visible primitive count versus draw calls/batches.

### Worker proof
Debug output must state where engine and rendering execute. A worker file existing on disk is not proof.

### WebGPU proof
Report actual backend/device initialization. A WebGPU-named class using Canvas2D is failure.


---

## SOURCE FILE: 09_FUTURE_ROADMAP_AND_GATES.md

# FUTURE ROADMAP AND REVIEW GATES

This document gives Codex long-term context. It does not authorize implementation beyond the current gate.

## Phase 0B — Command, transaction, history, selection foundation

Goals:
- define typed editor commands;
- validate mutations;
- implement transaction begin/update/commit/rollback;
- implement undo/redo and branch invalidation;
- implement session selection state without renderer coupling.

Proof requirements:
- normal persistent mutations cannot bypass the command API;
- one drag-like transaction yields one undo step;
- rollback restores semantic state;
- redo is invalidated after divergent edit;
- selection is not serialized as document truth.

## Phase 0C — Computed scene, spatial, hit testing, camera

Goals:
- derive runtime scene from persistent document;
- add dirty propagation;
- add spatial-index abstraction and implementation;
- geometry-aware hit testing;
- camera and infinite-canvas coordinate conversion.

Proof requirements:
- one isolated node change does not rebuild all nodes;
- point/rectangle queries use spatial candidates;
- ellipse hit testing is geometry-aware;
- camera changes do not mutate document transforms;
- runtime scene can be recreated from the document.

## Phase 0D — WASM, worker, WebGPU

Goals:
- host core in WASM;
- create EngineHost boundary;
- Dedicated Worker runtime where supported;
- WebGPU renderer using wgpu or reviewed equivalent;
- render-item representation;
- batching and culling;
- backend/instrumentation reporting.

Proof requirements:
- real WebGPU device/backend is initialized;
- claimed worker execution is verifiable at runtime;
- offscreen nodes are culled;
- large homogeneous primitive sets are batched/instanced;
- renderer remains replaceable without changing document semantics.

## Phase 0E — Professional editor shell

Goals:
- React UI shell;
- Wanted Design System token extraction/mapping;
- Layers, Toolbar, Inspector, Canvas, Debug surfaces;
- select/move/scale/rotate interactions;
- group/ungroup and nested editing;
- pan/zoom/zoom-around-pointer;
- instrumented benchmark fixtures.

Proof requirements:
- UI follows Wanted Design System rather than generic library defaults;
- UI mutations route through engine command/transaction interfaces;
- selection overlays are not persistent document nodes;
- direct manipulation feels deterministic and precise;
- 10k/100k fixtures expose real metrics.

## Phase 1 — Figma-class direct manipulation

Focus:
- marquee selection;
- deep/nested selection behavior;
- precision resize/rotate;
- smart snapping and guides;
- alignment/distribution;
- inspector numeric editing and scrubbing;
- duplication/modifier-key behaviors;
- pixel/grid behavior;
- copy/paste/duplicate;
- robust multi-selection transform semantics.

Quality bar:
The editor should begin to feel like a professional vector design tool rather than a canvas demo.

## Phase 2 — Vector and text engine

Vector:
- Bezier nodes/handles;
- path editing;
- open/closed paths;
- strokes, caps, joins, dash;
- gradients;
- booleans;
- live boolean/group representation where practical;
- outline stroke/offset path candidates.

Text:
- Unicode-aware text model;
- shaping and font fallback;
- HarfBuzz/Skia/FreeType-class dependency decision through ADR;
- CJK and Korean IME;
- line breaking;
- paragraph layout;
- kerning/ligature/tracking;
- variable fonts/open type where feasible.

Do not let a browser contenteditable DOM become the document text truth.

## Phase 3 — Layout, components, design-system behavior

- constraints;
- auto layout;
- hug/fill/fixed sizing concepts;
- components;
- variants;
- instances/overrides;
- styles/tokens/variables;
- reusable template structures.

## Phase 4 — Professional compositing

- masks;
- clipping;
- blend modes;
- shadows/blur/effects;
- image nodes;
- non-destructive adjustments;
- renderer effect graph.

## Phase 5 — Motion engine

- `Property<T>` static/animated model;
- timeline;
- keyframes;
- interpolation/easing;
- parenting;
- animated vector/text/effect properties;
- motion graph/editor later;
- frame-rate/timebase rules appropriate for broadcast workflows.

## Phase 6 — Semantic broadcast graphics

Native semantic objects:
- BarChart;
- line/pie/chart families;
- Table;
- MapGraphic;
- LowerThird;
- Ticker;
- ElectionGraphic;
- ProfileCard;
- data containers/bindings.

Structured objects must allow controlled detach into freeform primitives.

## Phase 7 — AI-native editing

- natural language to typed editor commands;
- command planning/validation;
- preview/dry-run;
- deterministic replay;
- undoability;
- target resolution by semantic ID/role;
- optional internal/external model adapters;
- no direct pixel mutation as primary editor mechanism.

## Phase 8 — Professional interop and production

- PSD bridge/export/import strategy;
- After Effects bridge;
- SVG/PDF;
- PNG/video/server rendering;
- desktop/native host reuse;
- plugin/API surface;
- production asset management.

## Mandatory review rule

Every phase may change the implementation plan after review, but may not silently weaken the architectural principles in the master specification.
