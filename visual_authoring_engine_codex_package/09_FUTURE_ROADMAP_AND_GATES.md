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
