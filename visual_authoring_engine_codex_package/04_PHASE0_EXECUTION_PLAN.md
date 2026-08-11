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
