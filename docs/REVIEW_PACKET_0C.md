# REVIEW PACKET - PHASE 0C

## 1. Revision and authorization

- Date: 2026-08-08 (Asia/Seoul).
- Commit/branch: N/A - the supplied workspace is not a Git repository.
- Revision: Phase 0C Computed Scene, Dirty Propagation, Spatial Index, Hit Testing, Camera.
- Authorization: `docs/PHASE_0C_AUTHORIZATION.md` records explicit external Gate 0B approval.
- Approved baseline reverified before implementation: 65/65 unit/integration/property tests
  and 1/1 compile-fail doctest passed, with 0 failed and 0 ignored.
- Phase 0B review ZIP reference: 46,100,878 bytes, SHA-256
  `4df5259941db731d6df1b091c7b866e66b420460d899cf4c3d6b5db54450eaff`.

## 2. Implemented scope

- Revisioned semantic `DocumentChangeSet` emitted from all persistent mutation directions.
- Explicit lower-level `HeadlessEditorCore` and app-facing scene-aware `EngineRuntime`.
- Fully rebuildable, iterative `ComputedScene` with invalid-derived node policy.
- Incremental Transform/Geometry/Appearance/Hierarchy/Visibility dirty propagation.
- Separate own geometry bounds and incrementally aggregated subtree/container bounds.
- Replaceable `SpatialIndex` and actual deterministic uniform-grid implementation.
- Indexed point/rectangle candidate queries and geometry-aware point hit testing.
- Persistent child-order-based structural z-order, including undo/redo/reorder.
- World/viewport/device Camera conversions, pan, zoom, zoom-around-pointer, fit, resize, DPR.
- Per-update and cumulative runtime metrics.
- Deterministic BENCH-A/B/C/D fixtures, release proof binary, JSON metrics, and raw log.
- ADR-010 through ADR-019, README, and 56 new automated tests.

## 3. Explicitly unimplemented

Phase 0D and later work was not started. There is no WASM bridge, Dedicated Worker, WebGPU or
wgpu renderer, render item/representation, GPU resource, batching/instancing, renderer
culling, React/browser UI, Wanted Design System UI, selection overlays/handles, direct
manipulation, snapping, vector/path/text engine, AI interpreter, PSD, or After Effects bridge.

The `wasm-bindgen` packages visible as target-support transitive dependencies of `uuid` in
`Cargo.lock` are not a WASM bridge or runtime implementation.

## 4. Final crate and dependency graph

```text
visual_authoring_core_math
          |
          v
visual_authoring_document ----------------> visual_authoring_serialization
          |  \
          |   \---------------------------> visual_authoring_scene
          v                                    ^
visual_authoring_spatial ----------------------|
                                               |
                                               v
                                  visual_authoring_runtime
```

Concrete manifest direction:

- `core_math`: no production dependency.
- `document`: `core_math`, `serde`, `thiserror`, `uuid`.
- `spatial`: `core_math`, `document`, `thiserror`.
- `scene`: `core_math`, `document`, `spatial`, `thiserror`.
- `runtime`: `core_math`, `document`, `scene`, `serde_json`, `thiserror`, `uuid`.
- `serialization`: `core_math`, `document`, `serde`, `serde_json`, `thiserror`.

`cargo tree --workspace` passed. There are no cyclic local dependencies and no renderer,
browser, GPU, Worker, or UI dependency.

## 5. Scene-aware real execution path

```text
typed Command
  -> HeadlessEditorCore validation and crate-private Document mutation
  -> private ReversibleEffect stored in transaction/history
  -> public revisioned DocumentChangeSet
  -> EngineRuntime revision check
  -> exact dirty propagation
  -> ComputedScene record update
  -> SpatialIndex insert/remove/update
  -> SceneUpdateStats and synchronized success
```

`EngineRuntime` owns `HeadlessEditorCore`, `ComputedScene` (including the spatial index),
Camera, revisions, and metrics. It exposes only `&Document`, not a mutable core or Document.
Dispatch, transaction update, rollback, undo, redo, and replacement automatically synchronize
Scene before success. Transaction commit does not recompute because every preview update was
already synchronized.

Unexpected incremental Scene failure is not returned as normal success. The runtime performs
a fresh rebuild from the valid Document and returns `SceneSyncStatus::FallbackRebuild` with
the typed cause; fallback count is measured. No fallback occurred in final tests or proof.

## 6. Semantic change-set boundary

`DocumentChangeSet` contains a before/after document revision and an ordered list of:

- `NodesInserted { root, nodes, placement }`;
- `NodesRemoved { root, nodes, placement }`;
- `PlacementChanged { root, subtree, before, after, local_transform_changed }`;
- `LocalTransformChanged`;
- `GeometryChanged`;
- `VisibilityChanged`;
- `AppearanceChanged`;
- `PersistentPropertyChanged` for name/metadata/locked;
- `FullDocumentReset`.

Forward reports drive dispatch/preview/redo. Reverse reports drive rollback/undo. Deleted
subtree IDs and old/new parent/index are carried explicitly. `ReversibleEffect` remains
crate-private, raw Document mutators remain `pub(crate)`, and no `&mut Document`, mutable node,
full snapshot diff, or history inspection was exposed.

## 7. Computed Scene and invalid-derived policy

Each Scene record stores `NodeId`, runtime parent/children, root attachment, effective
visibility, optional world transform, typed invalid-derived state, own geometry world bounds,
subtree world bounds, spatial membership, last dirty categories, and revision. It does not
copy names, metadata, selection, history, UI state, or renderer data.

Initial build:

- validates Document;
- finds the Document root and detached forest roots;
- performs parent-first iterative traversal;
- composes each world matrix exactly once;
- performs reverse iterative aggregate construction;
- inserts eligible geometry into Spatial Index.

Detached subtree records remain present with `attached=false`, preserve their internal runtime
hierarchy, and are absent from spatial queries/hit testing.

Finite local transforms that overflow in composition become `InvalidDerivedState` rather than
failing the complete runtime. Non-finite world transforms/bounds and invalid ancestor world
state are typed. Invalid-derived nodes remain inspectable but are not indexed. Singular finite
transforms are stored; exact hit testing treats them as non-hittable.

## 8. Dirty propagation table

| Persistent change | Target/subtree work | Ancestor work | Spatial work |
|---|---|---|---|
| Local transform | world and bounds for target + descendants | aggregate until stable | update affected geometry |
| Geometry | target bounds only | aggregate until stable | update target |
| Visibility | effective visibility for target + descendants; no transform recompute | none | insert/remove descendants |
| Attach/detach/reparent | attached/world/visibility/bounds for moved subtree | old/new aggregate chains | insert/remove/update subtree |
| Same-parent reorder | structural children only | none | none |
| Delete/restore | remove/recreate exact records | original parent chain | remove/insert subtree |
| Appearance | mark one record | none | none |
| Name/metadata/locked | no derived work | none | none |
| Full replacement | explicit full rebuild | rebuild | rebuild |

Every traversal is iterative. Normal incremental updates do not use a full Document snapshot
comparison and do not rebuild the whole Scene.

## 9. Bounds architecture

- Local geometry remains owned by Document geometry payloads.
- Scene caches own geometry world AABB separately from subtree/container world AABB.
- Group/Document may have no own bounds but have child-derived subtree bounds.
- Frame retains own geometry bounds separately from descendants.
- Visual/effect bounds are not implemented or claimed.

Each container maintains child contributions in four ordered endpoint sets plus an exact
NodeId-to-Rect map. One leaf contribution updates in O(log sibling-count), and ancestors are
visited only while resulting aggregate bounds change. BENCH-B measured two Scene records
visited for one leaf update; 10,000 siblings were not scanned.

## 10. Spatial implementation and real-use evidence

`visual_authoring_spatial::SpatialIndex` declares insert, remove, update, point query,
rectangle query, clear, rebuild, and entry count. `UniformGridIndex` is the actual
implementation on the production Scene query path.

- Default logical cell: 256 units.
- Deterministic ordered maps/sets and NodeId identity.
- Only finite AABBs accepted.
- Hidden, detached, invalid-derived, singular-collapsed/zero-area geometry excluded by Scene.
- Large entries use a correctness-preserving overflow bucket.
- Excessively broad rectangle query returns typed `QueryTooLarge`; it never falls back to a
  full entry scan.
- Index order is ignored; Scene applies document structural z-order.

Production `hit_test_world_point` calls `spatial.query_point`; production rectangle candidate
query calls `spatial.query_rect`. The only brute-force spatial oracle is under `#[cfg(test)]`.
Final BENCH-C proved 1 candidate and 1 exact geometry test from 100,001 total Scene records.

## 11. Geometry-aware hit testing and z-order

Point pipeline:

```text
ViewportPoint -> Camera::viewport_to_world -> SpatialIndex point candidates
-> inverse world transform -> exact local geometry -> structural z-order
```

- Rectangle/Frame: local `[0,width] x [0,height]` containment.
- Ellipse: normalized center/radius equation; AABB corner miss is tested.
- Supports translation, rotation, non-uniform scale, negative/reflection scale, nested
  transforms, and camera pan/zoom.
- Locked nodes remain hittable/selectable.
- Hidden, detached, invalid-derived, singular, and zero-size nodes are non-hittable.
- Results expose topmost, all hits top-to-bottom, candidate count, and exact-test count.

Z-order is derived from persistent child-index paths only for returned candidates. Later
siblings/subtrees sort above earlier siblings/subtrees. Reorder/reparent/undo/redo immediately
change results without writing a dense global z-index to unrelated nodes.

`query_rect_candidates` intentionally claims AABB candidates only; exact marquee selection is
not implemented.

## 12. Camera convention

- `WorldPoint`, `ViewportPoint`, and `DevicePoint` are distinct newtypes.
- Document units are logical pixels; +X right and +Y down.
- Camera stores world center, positive zoom, logical viewport size, and DPR.
- Viewport center maps to Camera world center.
- Pan moves center opposite the logical viewport delta divided by zoom.
- Zoom-around-pointer preserves the world point under the pointer.
- DPR is applied only by explicit viewport-to-device conversion.
- Fit bounds uses logical padding and rejects empty/inverted bounds.
- Invalid/non-finite input and non-finite results return typed `CameraError`.

Camera methods dispatch no command and change no Document snapshot, history depth, node
transform, serialization, or Scene revision.

## 13. Instrumentation definitions

Actual update/query locations increment:

- dirty and visited Scene nodes;
- world transforms and bounds recomputed;
- ancestor aggregate contributions updated;
- spatial entries inserted/removed/updated;
- spatial candidates and exact geometry tests;
- full and fallback rebuilds;
- document and Scene revisions.

Current total/attached/effectively-visible/invalid/indexed counts are maintained without a
post-edit all-node scan. `SceneUpdateStats` is per operation; `SceneMetrics::cumulative` is
cumulative. Metrics are runtime cache state and are not serialized.

## 14. Actual release proof results

Source: generated `docs/PHASE_0C_METRICS.json` and raw
`docs/verification/PHASE_0C_VERIFICATION.txt`.

| Fixture | Total records | Build/rebuild evidence | Actual result |
|---|---:|---|---|
| BENCH-A | 1,001 (1,000 rectangles + root) | initial full rebuild 1 | visited/world/bounds 1,001; indexed 1,000 |
| BENCH-B | 10,001 (10,000 rectangles + root) | incremental full rebuild 0 | visited 2; world 1; bounds 1; ancestor aggregate 1; spatial update 1 |
| BENCH-C | 100,001 (100,000 rectangles + root) | actual indexed hit path | candidates 1; exact tests 1; indexed 100,000 |
| BENCH-D | 10,001 (root + 10,000 nested groups) | iterative initial rebuild 1 | visited/world/bounds 10,001; stack overflow 0 |

Informational release timings from the final run (not pass/fail thresholds):

- BENCH-A fixture 5.1290 ms, Scene 9.1869 ms.
- BENCH-B fixture 23.1986 ms, Scene 89.4997 ms, isolated update 0.1690 ms.
- BENCH-C fixture 268.4232 ms, Scene 918.0099 ms, point query 0.0203 ms.
- BENCH-D fixture 46.8603 ms, Scene 56.1846 ms.

BENCH-B irrelevant name change measured dirty/visited/world/bounds/spatial/full rebuild all 0.
BENCH-B document/Scene revisions both ended at 2. All nine proof acceptance booleans were true.
Peak memory was not claimed or measured.

## 15. Incremental/fresh rebuild oracle evidence

Automated tests compare semantic Scene snapshots with a fresh rebuild after:

- isolated incremental transform;
- a deterministic pseudo-random 128-command transform/geometry/visibility/name sequence;
- undo and redo;
- transaction preview/commit and rollback;
- delete/undo/redo;
- attach/detach/reparent and world-preserving reparent;
- document replacement;
- invalid-derived overflow.

All comparisons passed. Full replacement builds from the replacement Document and synchronizes
revisions. Transaction commit preserved the preview Scene without a second recompute.

## 16. Automated test inventory and results

Final required test command breakdown:

- `visual_authoring_core_math`: 12 passed.
- `visual_authoring_document`: 47 passed (42 Gate 0B + 5 change-set tests).
- `visual_authoring_runtime`: 46 passed.
- `visual_authoring_scene`: 0 standalone tests; exercised through runtime tests.
- `visual_authoring_serialization`: 11 passed.
- `visual_authoring_spatial`: 5 passed.
- proof binary test target: 0 tests.
- Total unit/integration/property tests: **121 passed, 0 failed, 0 ignored**.
- Compile-fail doctest: **1 passed, 0 failed, 0 ignored**.
- Existing Gate 0B baseline retained: 65 tests + 1 doctest.
- New Phase 0C tests: 56.

The required 37 evidence areas are covered, including rebuild/oracle sequences,
undo/redo/transaction/replacement, exact dirty categories, hierarchy/delete restoration,
incremental aggregate bounds, spatial CRUD/rectangle/random oracle, actual indexed hit path,
ellipse corner miss, transformed shapes, visibility/detachment/lock, z-order history, all
Camera operations/errors/session exclusion, degenerate/extreme numeric policy, BENCH-B/C/D,
serialization exclusion, and the retained compile-fail mutation proof.

## 17. Final mandated verification

Executed on Windows through the pinned Rust/Cargo 1.89.0 wrapper. Complete actual output and
exit codes are in `docs/verification/PHASE_0C_VERIFICATION.txt`.

| Command | Actual result |
|---|---|
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check` | Passed, exit 0 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings` | Passed, exit 0, warnings denied |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets` | Passed, exit 0 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets` | Passed, exit 0, 121 tests |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace` | Passed, exit 0, 1 compile-fail doctest |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets` | Passed, exit 0 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace` | Passed, exit 0 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0c-proof.ps1` | Passed, exit 0, `all_passed=true` |

Failed/ignored/skipped: 0. No unexecuted check is recorded as passed.

## 18. Important source map

- Workspace: `Cargo.toml`, `Cargo.lock`.
- Change descriptions: `crates/document/src/change.rs`.
- Private effect-to-change derivation: `crates/document/src/command.rs`.
- Headless core/revisions/all mutation paths: `crates/document/src/editor.rs`.
- Linear invariant validation: `crates/document/src/lib.rs`.
- Spatial abstraction/implementation: `crates/spatial/src/lib.rs`.
- Scene/dirty/hit/metrics: `crates/scene/src/lib.rs`.
- Incremental bounds aggregate: `crates/scene/src/bounds.rs`.
- Camera: `crates/runtime/src/camera.rs`.
- Runtime coordinator: `crates/runtime/src/lib.rs`.
- Deterministic fixtures: `crates/runtime/src/fixtures.rs`.
- Phase 0C tests: `crates/runtime/src/tests.rs`, spatial/document test modules.
- Release proof binary: `crates/runtime/src/bin/phase0c_proof.rs`.
- Proof/verification runners: `tools/phase0c-proof.ps1`, `tools/phase0c-verify.ps1`.

## 19. ADRs

- ADR-010: revisioned semantic document change sets.
- ADR-011: scene-aware runtime ownership and synchronization.
- ADR-012: Computed Scene and invalid-derived policy.
- ADR-013: dirty categories and propagation.
- ADR-014: incremental subtree/container bounds.
- ADR-015: replaceable uniform-grid Spatial Index and dependency decision.
- ADR-016: geometry-aware hit testing and degenerate policy.
- ADR-017: structural z-order.
- ADR-018: Camera conventions and DPR.
- ADR-019: instrumentation and proof fixtures.

## 20. Dependencies and licenses

No new third-party spatial dependency was added. The custom index uses Rust standard-library
ordered collections and local NodeId/Rect types behind a replaceable trait.

Locked production dependency versions reported by final Cargo resolution include:

- `serde` 1.0.229 - MIT OR Apache-2.0.
- `serde_json` 1.0.151 - MIT OR Apache-2.0.
- `thiserror` 2.0.19 - MIT OR Apache-2.0.
- `uuid` 1.24.0 - Apache-2.0 OR MIT.

`proptest` 1.11.0 and its graph are test-only. Local crates are `MIT OR Apache-2.0`.
Dependency purpose, boundary data, replaceability, and spatial licensing are documented in
ADR-001 through ADR-004 and ADR-015.

## 21. Deviations

The instruction recommended evaluating a mature R-tree/BVH. A project-owned uniform grid was
selected instead, as explicitly allowed when justified. ADR-015 records why: deterministic
per-ID updates, no new network-fetched critical dependency, a narrow replaceable trait, typed
large-query policy, and randomized brute-force correctness evidence. No functional spatial
requirement was omitted.

An additional `tools/phase0c-verify.ps1` orchestrator was added to capture raw output from all
eight mandated commands in one reproducible log. It does not replace or weaken any command.

## 22. Known bugs

None known after format, zero-warning Clippy, debug/release builds, 121 tests, doctest, Cargo
tree, and release proof all passed.

## 23. Known limitations and temporary scope

- Singular and zero-size geometry is intentionally non-hittable; no screen-space tolerance
  fallback exists yet.
- Rectangle query is broad-phase AABB candidates, not exact marquee selection.
- Uniform-grid broad queries spanning more than 1,000,000 cells return `QueryTooLarge`.
- Very large entries use an overflow bucket and may increase candidates if many such entries
  coexist, while preserving correctness.
- Structural z-order constructs candidate paths on query; no order-maintenance key exists.
- Visual/effect bounds, opacity-dependent visibility, layout, text, and effects are absent.
- `HeadlessEditorCore` remains a public explicitly lower-level persistence/test boundary; the
  normal scene-aware application boundary is `EngineRuntime`.
- Metrics do not include peak memory; no result claims it.
- Fixture constructors are deterministic test/proof utilities at a validated load boundary,
  not normal edit paths.

## 24. Architecture risks

- Future effect/visual bounds must extend bound categories without redefining geometry bounds.
- Future layout/text/effect dirty categories must preserve targeted propagation.
- A later R-tree/BVH replacement must preserve NodeId identity, finite-bound policy,
  deterministic post-query z-order, and instrumentation.
- Cross-process command protocol/revision reconciliation needs a Phase 0D+ ADR without exposing
  raw mutation or private effects.
- A future order-maintenance optimization must preserve persistent child order semantics.

## 25. Packaging verification

- Final archive path:
  `C:\Users\thdwl\Documents\Codex\visual_authoring_engine_phase0c_review_2026-08-08.zip`.
- Entry count: 75.
- Archive size: 46,157,304 bytes.
- SHA-256: `0afc84b2e0eac0173cc1346e76fd78ce7b600119b33fa4d386c579633f1b6264`.
- `target`, `.tools`, prior/nested ZIPs, IDE caches, temporary files, and unnecessary compiled
  binaries: 0 matching entries after reopening the archive.
- Authoritative package: 18/18 embedded entries matched `SHA256SUMS.txt` after reopening.
- Wanted Design System original: 46,381,631 bytes, SHA-256
  `d6f87f906ef2cbf211cae4b7bfe92429c064a4d783529ff14386b0a809fe7887`.

The archive hash is necessarily computed after archive bytes are finalized. The final external
handoff reports that archive's actual hash; this packet is finalized immediately before the
archive is created and records the verified entry/size/hash values in the workspace copy.

## 26. Gate 0C checklist

### Ownership and synchronization

- [x] Computed Scene is completely rebuildable from Document.
- [x] Normal app mutations synchronize Scene automatically.
- [x] Undo/redo/preview/rollback/replacement cannot return successful stale Scene state.
- [x] Document/Scene revisions are checked and reported.
- [x] Private effects and raw mutation remain sealed.

### Dirty and bounds

- [x] Transform, Geometry, Appearance, Hierarchy, and Visibility semantics are explicit.
- [x] Irrelevant properties cause zero recompute/spatial work.
- [x] Own and subtree bounds are distinct.
- [x] One leaf change avoids sibling scans and full rebuild.
- [x] Invalid-derived nodes are typed and excluded from Spatial Index.

### Spatial, hit, z-order, camera

- [x] Real Spatial Index is used by production query/hit paths.
- [x] BENCH-C does not full-scan or geometry-test 100,000 nodes.
- [x] Ellipse exact hit differs from AABB.
- [x] Persistent child order controls topmost results through history changes.
- [x] Camera spaces, DPR, pan, zoom-around-pointer, fit, and errors are tested.
- [x] Camera does not mutate or serialize Document/session history.

### Proof and scope

- [x] BENCH-A/B/C/D actual release metrics were generated.
- [x] 10,000-depth build is iterative and succeeds.
- [x] 121 tests and 1 doctest pass; failed/ignored/skipped 0.
- [x] Format, Clippy, debug/release build, Cargo tree, and proof pass.
- [x] Metrics JSON and raw verification log are generated by execution.
- [x] Phase 0D/WASM/Worker/WebGPU/renderer/UI were not started.
- [x] Final ZIP metadata and protected-input checksums recorded after packaging.

## 27. Stop confirmation

Phase 0C implementation and clean review packaging are complete. Work stops at Gate 0C.
Phase 0D is not authorized and was not started.
