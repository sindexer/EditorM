# Visual Authoring Engine

Phase 0D-R1 corrects the Gate 0D atomicity, bounded-work, ordering, and Worker/GPU sequencing defects while preserving the Gate 0A-0C Document, command/history/selection, serialization, and computed-scene semantics.

The current path is real, not mocked: a Dedicated Worker owns a wasm32 `EngineRuntime`; a versioned EngineHost protocol returns transferable render data; and a main-thread WebGPU renderer draws rectangle and ellipse instances. A native `wgpu` renderer provides a separate hardware proof. Phase 0E product editor work has not started.

## Workspace

- `crates/core_math`: finite-aware f64 vector, Rect, and Affine2 primitives.
- `crates/document`: persistent Document truth, typed commands, transactions, history, selection, revisioned semantic changes, and atomic mutation.
- `crates/serialization`: version 1 persistent JSON; runtime/session state is excluded.
- `crates/spatial`: replaceable `SpatialIndex` and production `RTreeIndex`.
- `crates/scene`: rebuildable ComputedScene, invalid-derived policy, hierarchy/bounds, R*-tree queries, exact hit testing, cached sibling order, and structural work metrics.
- `crates/render_model`: backend-neutral RenderItems, stable slots, bounded prepared patches, O(1) counters, typed f32 omission diagnostics, and indexed culling.
- `crates/renderer_wgpu`: native wgpu device/pipeline/buffers, instanced rendering, shared WGSL, GPU-boundary handling, metrics, and offscreen proof.
- `crates/runtime`: EngineRuntime, Camera, deterministic fixtures, and Phase 0C/0D/0D-R1 proof binaries.
- `crates/wasm_bridge`: wasm-bindgen EngineHost, render schema versioning, sequencing, and Worker protocol boundary.
- `shared`: common WGSL and checked render binary schema used by native and browser renderers.
- `web/phase0d-preview`: dependency-free diagnostic preview, Worker host, browser WebGPU, self-contained hardware automation, and pixel readback proof.
- `docs/adr`: ADR-001 through ADR-028.
- `visual_authoring_engine_codex_package`: immutable authoritative project reference.

## Ownership and synchronization

```text
Document (persistent f64 truth)
  -> typed Command / transaction / history
  -> revisioned DocumentChangeSet
  -> ComputedScene + RTreeIndex
  -> bounded RenderModel patch + stable slot
  -> native wgpu renderer
       or
     wasm EngineHost in Dedicated Worker
       -> sequenced transferable instance/visibility buffers
       -> serialized main-thread WebGPU frame
```

JavaScript has no Document mirror or mutation API. Scene, spatial data, RenderModel, GPU buffers, and visible sets are disposable derived state. Camera is session state and changes no Document, Scene, or Render revision. A valid f64 item outside f32 range remains in the Document, is zeroed/omitted from GPU visibility with a typed diagnostic, and recovers its stable slot after correction.

## Run the diagnostic preview on Windows

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-phase0d-preview.ps1
```

The script installs/builds the static diagnostic, starts a local server, prints its URL/PID, and opens the default browser. Add `-NoBrowser` to suppress opening.

The automated browser proof needs no pre-started server:

```powershell
cd web/phase0d-preview
npm run test:browser
```

It selects a localhost port, starts and health-checks the preview, validates asset status and WASM MIME, launches a new Chrome process with a temporary profile, uses actual WebGPU, writes proof/screenshot/pixel artifacts, and cleans up its server, browser, and profile. WebGPU absence is blocking; there is no Canvas2D, mock, or static-image success fallback.

## Rebuild the WASM bridge

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0d-wasm.ps1
```

This requires target `wasm32-unknown-unknown` and `wasm-bindgen` CLI 0.2.126. Set `VAE_WASM_BINDGEN` if the verified executable is not on PATH or under `VAE_TOOL_ROOT`.

## Verification

Run the complete recorded verification:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-r1-verify.ps1
```

Or run only the native and self-contained browser hardware proofs:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-r1-proof.ps1
```

The final Windows run passed format, Clippy with warnings denied, debug and release workspace builds, all-target tests, doctest, dependency tree, wasm32 release build, generated WASM package, Phase 0C and Phase 0D regression proofs, clean web install/build/unit tests, and actual Chrome WebGPU proof.

- Rust unit/integration/property tests: 154 passed, 0 failed, 0 ignored.
- Compile-fail doctest: 1 passed, 0 failed, 0 ignored.
- Web unit tests: 6 passed, 0 failed, 0 skipped.
- Browser proof artifact test: 1 passed, 0 failed, 0 skipped.
- Native R1 structural checks: 18/18 true.
- Browser R1 checks: 24/24 true.

Evidence:

- `docs/PHASE_0D_R1_METRICS.json`
- `docs/verification/PHASE_0D_R1_VERIFICATION.txt`
- `docs/verification/PHASE_0D_R1_BROWSER_PROOF.json`
- `docs/verification/PHASE_0D_R1_PIXEL_READBACK.json`
- `docs/verification/phase0d-r1-preview.png`
- `docs/REVIEW_PACKET_0D_R1.md`

## Gate boundary

Work stops at the Gate 0D-R1 review package. The preview is a technical diagnostic, not a Wanted Design System product UI. Phase 0E, React product UI, and Wanted Design System integration have not started and require separate external authorization.