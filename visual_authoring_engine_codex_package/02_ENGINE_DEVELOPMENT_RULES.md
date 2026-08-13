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
