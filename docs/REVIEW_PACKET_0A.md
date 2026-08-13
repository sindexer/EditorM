# REVIEW PACKET - PHASE 0A GATE DEFECT REPAIR

## 1. Revision

- Commit hash: N/A - the supplied workspace was not a Git repository.
- Branch: N/A.
- Date: 2026-08-07 (Asia/Seoul).
- Revision purpose: Gate 0A external-review defect repair only.

## 2. Build

- Build command: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets`
- Required toolchain: Rust/Cargo 1.89.0, pinned by `rust-toolchain.toml`.
- Local verification target: `x86_64-pc-windows-gnu`.
- Build result: passed for all workspace targets.

`tools/cargo.ps1` uses Cargo from `PATH`; it can also use an isolated toolchain from
`.tools` or the path named by `VAE_TOOL_ROOT` without changing the user's system PATH.

## 3. Tests

- Test command: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets`
- Format command: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check`
- Lint command: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings`
- Passed: 42 tests (12 core math, 21 document, 9 serialization).
- Failed: 0.
- Skipped/ignored: 0.
- Format result: passed.
- Clippy result: passed with warnings denied.
- Build result: passed for all workspace targets.
- Test result: passed for all workspace targets.

The core-math count includes property tests for combined affine round trips and for the rule
that finite inputs can never produce a successful non-finite inverse.

## 4. Repository tree

```text
.
|-- Cargo.toml
|-- Cargo.lock
|-- rust-toolchain.toml
|-- rustfmt.toml
|-- README.md
|-- crates
|   |-- core_math
|   |   |-- Cargo.toml
|   |   `-- src/lib.rs
|   |-- document
|   |   |-- Cargo.toml
|   |   `-- src/lib.rs
|   `-- serialization
|       |-- Cargo.toml
|       `-- src/lib.rs
|-- docs
|   |-- REVIEW_PACKET_0A.md
|   `-- adr
|       |-- ADR-001-rust-core-workspace-boundaries.md
|       |-- ADR-002-stable-node-id-strategy.md
|       |-- ADR-003-transform-matrix-representation.md
|       `-- ADR-004-serialization-version-strategy.md
|-- tools
|   `-- cargo.ps1
`-- visual_authoring_engine_codex_package
    |-- authoritative specification documents
    |-- templates
    `-- reference/wanted-design-system
        |-- Wanted Design System (Community).fig
        |-- meta.json
        `-- thumbnail.png
```

Generated `target/` output and isolated `.tools/` toolchains are ignored.

## 5. Implemented scope

- Three-crate Rust workspace with one-way dependency direction.
- `f64` `Vec2`, `Rect`, and true 2D affine matrix primitives.
- Translation, rotation, uniform/non-uniform/negative scale, composition, point/vector
  transformation, transformed rectangles, approximate comparisons, and safe inversion.
- UUID v4 stable `NodeId` with map/set traits and string serialization.
- Persistent Document, Frame, Group, Rectangle, and Ellipse node kinds.
- Separate geometry, appearance, and metadata payload boundaries.
- Invariant-safe register/create, attach, detach, local-preserving reparent,
  world-preserving reparent, ordered children, subtree deletion, and lookup.
- Explicit invariant validation for root identity, unique IDs, parent/child consistency,
  container eligibility, duplicate child references, dangling references, cycles, finite
  transforms, geometry, and appearance.
- Local/world transform resolution and local/world point conversion.
- Geometry/local and geometry-only world bounds; ellipse bounds use affine ellipse extents.
- Explicit version 1 JSON envelope and private schema DTOs.
- Validated load path that rejects duplicate, dangling, inconsistent, and cyclic data.
- Stable-ID, semantic, child-order, and text-stable save/load/save round trips.
- Deterministic and property-generated tests for the required transform combinations.

### 5.1 External-review defects repaired

- `Affine2::inverse` now rejects non-finite determinants, unsafe normalized singularity
  calculations, non-finite reciprocals, and any non-finite final inverse. The existing
  scale-relative near-singular rejection policy remains in effect.
- Document derived-transform, point-conversion, and bounds APIs now convert arithmetic
  overflow into typed recoverable errors instead of returning successful non-finite values.
- World-preserving reparenting completes every fallible calculation before hierarchy or
  transform mutation, so failure leaves the document unchanged.
- Ellipse extents use `f64::hypot` to avoid unnecessary intermediate-square overflow;
  rectangle and ellipse bounds both reject non-finite final values.
- Subtree collection now uses an iterative preorder stack, preserving order and siblings
  without depending on the process call stack for deep valid hierarchies.
- Regression coverage now includes uniform scale, determinant and inverse-translation
  overflow, valid large transforms, nested composition overflow, typed point errors,
  non-orthogonal ellipse bounds, atomic failed reparent, same-parent reorder persistence,
  and a 20,000-node subtree deletion.

## 6. Explicitly unimplemented
Per the gate, there is no React/browser shell, UI, Wanted token mapping, WASM, Worker,
WebGPU/renderer, computed scene, spatial index, hit testing, camera, selection, command
system, transaction/history, snapping, text/vector engine, effects, AI, PSD, or After
Effects bridge. No Phase 0B interface or placeholder was started.

## 7. Architecture summary

```text
visual_authoring_core_math
          |
          v
visual_authoring_document
          |
          v
visual_authoring_serialization
```

`serialization` also imports `core_math` only to map explicit matrix/size fields. Math owns
no document or platform concern. Document is persistent semantic truth and exposes read-only
nodes plus mutation methods that maintain hierarchy invariants. Serialization owns file
shape, not semantic validity; all loaded data crosses `Document::from_snapshot`. There are
no cyclic local-crate dependencies and no renderer/browser/UI dependencies.

## 8. Important source map

- Workspace/dependencies: `Cargo.toml`, `Cargo.lock`.
- Core math and tests: `crates/core_math/src/lib.rs`.
- Document root and node definitions: `crates/document/src/lib.rs` (`Document`, `Node`,
  `NodeSpec`, `NodeKind`, `Geometry`).
- Stable ID: `crates/document/src/lib.rs` (`NodeId`).
- Hierarchy/invariants: `crates/document/src/lib.rs` (`attach_child`, `detach`, `reparent`,
  `reparent_preserving_world`, `delete_subtree`, `validate_invariants`).
- Transform and bounds resolution: `crates/document/src/lib.rs` (`world_transform`,
  `local_to_world`, `world_to_local`, `local_bounds`, `world_bounds`).
- Serialization: `crates/serialization/src/lib.rs`.
- Tests: inline `#[cfg(test)]` modules in all three crate roots.

## 9. ADRs created

- ADR-001: Rust core workspace boundaries.
- ADR-002: Stable node ID strategy.
- ADR-003: Transform and matrix representation.
- ADR-004: Serialization and version strategy.

## 10. Crate dependency summary

- `core_math`: no production dependencies; `proptest` is test-only.
- `document`: `core_math`, `uuid`, `serde`, and `thiserror`; `proptest` is test-only.
- `serialization`: `document`, `core_math`, `serde`, `serde_json`, and `thiserror`.
- Critical dependency purposes, replaceability, boundary data, and licenses are recorded in
  ADR-001 through ADR-004.

## 11. Deviations from specification

None known. The workspace was empty, so there was no existing implementation to preserve
or reconcile.

## 12. Known bugs

None known after format, warnings-as-errors Clippy, build, and 42 passing tests.

## 13. Known limitations

- Registered nodes and detached subtrees may intentionally have no parent; the registry is
  therefore a validated forest anchored by one document root, not a reachability-enforced
  single tree. This supports explicit detach operations.
- The document root's local transform is fixed to identity; camera/viewport concerns remain
  outside document transforms.
- Geometry is limited to non-negative axis-aligned local sizes for Frame, Rectangle, and
  Ellipse. Effects/visual bounds are absent.
- Appearance currently contains only opacity. Metadata is a conservative string map.
- Version dispatch exists, but no migration is needed or implemented beyond version 1.
- Matrix decomposition into inspector-friendly position/rotation/scale values is deferred.

## 14. Temporary implementations

- `Appearance { opacity }` and string metadata establish persistent boundaries without
  guessing future fill/stroke/effect or typed semantic schemas. They are intentionally
  minimal and expected to expand through reviewed schema versions.
- The JSON adapter is the sole file encoding at this gate; its DTO boundary is permanent,
  while additional encodings may be added later.

No compatibility shim, renderer stand-in, browser document model, or fake future subsystem
was introduced.

## 15. Future risks

- Affine matrices produced by nested non-uniform scaling and rotation can contain effective
  skew and have non-unique decomposition. Future inspector/animation semantics must not
  discard the authoritative matrix during decomposition.
- Redundant parent and child references improve diagnostics and preserve order but require
  every future migration/importer to maintain consistency.
- Future typed metadata/appearance additions must use schema migration rather than silently
  changing version 1 meaning.
- A future command boundary must wrap the current invariant-safe document mutation methods
  without exposing arbitrary node mutation.

## 16. Architecture violations

None known. Verified by crate manifests, public mutation surfaces, compilation, and tests.

## 17. Review questions

1. Should detached registered subtrees remain legal between edit operations and in saved
   documents, or should the external gate require every non-root node to be root-reachable?
2. Is fixing the document root transform to identity the desired permanent rule, or should a
   future import format be allowed to represent a transformed top-level semantic container?
3. Should version 2 retain redundant parent/children references, or move to a single ordered
   hierarchy representation after import diagnostics are established?

## 18. Gate checklist

### Repository and dependency direction

- [x] Pure math has no renderer/browser dependency.
- [x] Document has no renderer/browser/UI dependency.
- [x] Serialization does not leak runtime caches.
- [x] No cyclic dependencies.

### Identity and hierarchy

- [x] Stable IDs are persistent and not index-based.
- [x] IDs remain stable across save/load.
- [x] One parent maximum.
- [x] Cycle creation is prevented.
- [x] Child ordering is deterministic/persistent.
- [x] Reparent/delete semantics are explicit.

### Transform

- [x] Real affine matrix math exists.
- [x] Document math uses `f64`.
- [x] Local/world conversion is tested.
- [x] Nested non-uniform scale and rotation are tested.
- [x] Inverse failure is handled safely.
- [x] Finite inputs cannot produce a successful non-finite inverse.
- [x] Derived transform, coordinate, and bounds overflow returns typed errors.
- [x] Reparent preserving world transform is tested.
- [x] Failed world-preserving reparent leaves the document unchanged.
- [x] Camera concerns have not leaked into node transforms.

### Serialization

- [x] Versioned envelope exists.
- [x] Semantic round trip passes.
- [x] Invalid hierarchy is intentionally rejected.
- [x] Stable IDs persist.
- [x] No renderer/UI/session state is serialized.

### Extensibility and scope

- [x] Initial geometry payloads do not block future node kinds.
- [x] No WebGPU or DOM assumption is embedded in the data model.
- [x] Matrix transforms remain compatible with future animation properties.
- [x] Phase 0B+ was not implemented.
- [x] No hidden UI/renderer shortcut was introduced.

## 19. Packaging verification

- Review archive: `visual_authoring_engine_phase0a_gate_fix_review_2026-08-07.zip`.
- Generated `target/` and isolated `.tools/` directories are excluded from the archive.
- All 18 files listed by `visual_authoring_engine_codex_package/SHA256SUMS.txt`, including
  the Wanted Design System originals, matched their supplied SHA-256 values after repair.
- The authoritative specification package and Wanted Design System originals were not
  modified.

## 20. Stop confirmation

Gate 0A defects found by external review are repaired. Phase 0B was not started. Work stops
at Gate 0A pending external architecture review and explicit authorization.
