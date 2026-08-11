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
