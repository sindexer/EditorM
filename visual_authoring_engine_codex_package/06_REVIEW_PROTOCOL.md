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
