# Review Packet - Phase 0D

## 1. Revision and authorization

- Date: 2026-08-09 (Asia/Seoul).
- Commit/branch: N/A; the supplied workspace is not a Git repository.
- Authorized baseline: externally approved Gate 0C ZIP, 46,157,304 bytes, 75 entries,
  SHA-256 `0afc84b2e0eac0173cc1346e76fd78ce7b600119b33fa4d386c579633f1b6264`.
- Baseline evidence: 121 Rust tests, 1 compile-fail doctest, and 18/18 authoritative source
  checks passed.
- Authorization record: `docs/PHASE_0D_AUTHORIZATION.md`.
- External decision record: `docs/PHASE_0C_EXTERNAL_REVIEW.md`.

## 2. Implemented scope

- Checked, atomic revision exhaustion across dispatch, transaction update/rollback, undo,
  redo, and document replacement.
- Production R*-tree spatial index with finite NodeId/Rect boundary and brute-force oracle
  regression tests.
- Backend-neutral RenderModel with stable slots, free-list reuse, structural order, dirty
  slots/ranges, and incremental updates.
- Camera viewport culling through the Scene R*-tree; no full Scene scan on the culling path.
- Native `wgpu` renderer with actual adapter/device creation, storage buffers, instancing,
  rectangle/ellipse WGSL, offscreen render proof, metrics, and validation scopes.
- Validated f64-to-f32 boundary with camera-relative high/low translation splitting.
- `wasm32-unknown-unknown` EngineHost bridge and generated browser module.
- Versioned protocol with correlation ID, revisions, commands, transactions, undo/redo,
  camera/DPR, hit-test, render delta, metrics, heartbeat, and typed errors.
- Dedicated Worker ownership of WASM `EngineRuntime`; main thread owns only WebGPU rendering.
- Transferable full/dirty instance, removed-slot, and visible-slot ArrayBuffers.
- Minimal diagnostic WebGPU preview and actual Chrome hardware automation.

## 3. Explicitly unimplemented

Phase 0E and later were not started. There is no React product editor, Wanted Design System
implementation, Layers/Inspector/Toolbar, selection overlay, resize/rotate handles, snapping,
text/path authoring, clip/effect stack, PSD/AE bridge, AI interpreter, collaboration system,
or production file UI. The controls in `web/phase0d-preview` exist only to exercise Gate 0D.

## 4. Dependency graph

```text
core_math -> document -> serialization
                  |
                  v
               spatial (RTreeIndex)
                  |
                  v
                scene (ComputedScene)
                  |
                  v
             render_model
              /        \
             v          v
     renderer_wgpu    runtime
                         |
                         v
                    wasm_bridge
                         |
                         v
        Dedicated Worker EngineHost protocol
                         |
        transferable binary render payloads
                         |
                         v
            main-thread browser WebGPU
```

`runtime` gates native proof dependencies behind `native-wgpu`. The WASM bridge disables that
feature, so the exact root `cargo build --release --target wasm32-unknown-unknown` builds the
WASM-safe dependency path. `cargo tree --workspace` passed with exit code 0.

## 5. Persistent and derived ownership

- Document is the only persistent truth.
- Every persistent change still enters through typed Command/transaction/history.
- ComputedScene, RTreeIndex, RenderModel, stable slots, GPU buffers, and visible sets are
  rebuildable derived state.
- JavaScript does not hold a Document mirror.
- `EngineRuntime::document()` remains immutable and no mutable Document crosses WASM.
- Camera remains session state and changes no persistent or derived revision.
- Renderer replacement cannot alter command, history, selection, or serialization semantics.

## 6. Revision overflow correction

`DocumentChangeSet::changed` now receives explicit before/after revisions. The editor computes
the next revision once with `checked_add`; exhaustion returns
`EditorError::RevisionExhausted { revision }` before any mutation. Test-only initialization at
`u64::MAX` verifies Document, history, transaction preview, selection, Scene, and RenderModel
remain unchanged for every mutation direction. There is no wrap to Scene revision 0.

## 7. Spatial replacement

ADR-021 supersedes ADR-015 on the production Scene path. `RTreeIndex` stores finite bounds in
an `rstar::RTree` plus an authoritative NodeId map. It supports insert/remove/update/query/
clear/rebuild. Result z-order still comes from persistent child order. Mixed size, dense,
sparse 100,000, huge geometry, extreme zoom-out, and repeated mutation cases match the
brute-force test oracle.

Final native proof:

- narrow BENCH-C: 2 candidates, 2 visible, 99,998 culled;
- extreme zoom-out: 100,000 visible and submitted;
- `QueryTooLarge`: false.

Actual Chrome narrow BENCH-C returned 1 candidate from 100,000.

## 8. Render representation and resource model

A RenderItem is keyed by NodeId and contains primitive, size, optional f64 world transform and
bounds, opacity, renderability, structural order key, and stable slot. It is not persistent
truth. Deletion frees a slot; restoration/insertion may reuse it without duplication.

One transform update in final native proof:

- Scene full rebuilds: 0;
- RenderModel full rebuilds: 0;
- dirty slots/ranges: 1/1;
- instance upload calls/bytes: 1/48;
- full instance-buffer uploads: 0.

Fixture replacement is an explicit full reset; stale buffers are replaced rather than merged.

## 9. Batching, shader, and precision

Both native and browser paths use one 48-byte storage instance containing linear transform,
high/low translation, size, opacity, and primitive flag. Visible slots provide deterministic
bottom-to-top instance order. One instanced triangle-list draw emits six vertices per item.
Ellipse fragments outside the normalized radius are discarded; rectangle and ellipse output
is therefore geometry-aware.

Native BENCH-B proof:

- total/visible/submitted: 10,000 / 10,000 / 10,000;
- batches/draw calls: 1 / 1;
- GPU buffers/bytes: 3 / 852,000;
- validation errors: 0.

World/Scene values stay f64. The renderer validates finite f32 representability and uses
camera-relative high/low translation splitting. Non-representable values return typed errors.

## 10. Viewport culling

Camera computes one finite world viewport Rect. RenderModel queries the Scene R*-tree, applies
exact AABB visibility and renderability checks, then emits stable visible slots in structural
order. Hidden, detached, invalid-derived, and offscreen items do not reach submitted instances.
Camera movement changes only the visible payload and camera uniform; Document/Scene/Render
revisions remain stable.

The actual Chrome offscreen BENCH-B case submitted 0 instances. The fitted 10,000 case used
1 batch and 1 draw. Extreme native zoom-out returned all 100,000 without a broad-query error.

## 11. WASM, Worker, and EngineHost path

```text
main-thread typed request
-> Dedicated Worker
-> EngineHost::handleJson in generated WASM
-> EngineRuntime / Command / transaction / history / Camera
-> synchronized Scene + RTreeIndex + RenderModel
-> JSON response with protocol/revisions/metrics/typed error
-> transferable instance/visibility ArrayBuffers
-> main-thread WebGPU buffer update and instanced draw
```

Protocol version is 1. Each request and response carries a correlation ID. Large instance data
is not copied as per-frame JSON. Heartbeat comes from the Worker-owned host. Worker restart
constructs a new WASM host and full render payload. The HUD truthfully reports `Worker core /
main-thread WebGPU`; no OffscreenCanvas Worker GPU claim is made.

## 12. Actual hardware proof

Native proof (`docs/PHASE_0D_METRICS.json`):

- NVIDIA GeForce GTX 970, DiscreteGpu, Vulkan, NVIDIA driver 581.57;
- actual adapter/device and offscreen command submission;
- all 10 structural checks passed;
- fallback rebuilds and GPU validation errors: 0.

Browser proof (`docs/verification/PHASE_0D_BROWSER_PROOF.json`):

- Chrome 151.0.7922.76, actual NVIDIA GeForce GTX 970;
- WebGPU adapter architecture `maxwell`;
- browser WebGPU implementation backend is not exposed by the Web API and is reported as such;
- no WebGPU mock, Canvas2D fallback, or static-image substitution;
- Worker WASM, heartbeat/restart, resize/DPR, pan/zoom, hit-test, command, undo/redo,
  offscreen culling, 10k instancing, 100k pruning, metrics agreement all passed;
- browser console errors, GPU validation errors, fallback rebuilds: 0;
- screenshot: `docs/verification/phase0d-preview.png`.

## 13. Verification results

Every command below ran on Windows and returned exit code 0. Raw stdout/stderr and timestamps
are in `docs/verification/PHASE_0D_VERIFICATION.txt`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0c-proof.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-proof.ps1
```

Web commands also returned exit code 0:

```powershell
npm ci --ignore-scripts
npm run build
npm test
npm run test:browser
```

Test accounting is kept by command rather than double-counting proof assertions:

- Rust unit/integration/property tests: 145 passed, 0 failed, 0 ignored;
- compile-fail doctest: 1 passed, 0 failed, 0 ignored;
- web unit tests: 4 passed, 0 failed, 0 skipped;
- browser proof artifact test: 1 passed, 0 failed, 0 skipped;
- browser automation structural checks inside the proof: 16/16 true;
- Phase 0C regression proof: all structural checks true;
- Phase 0D native proof: 10/10 structural checks true.

## 14. Preview procedure

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-phase0d-preview.ps1
```

Use `-NoBrowser` to start without opening the default browser. The script prints the local URL
and server PID. Detailed observable steps are in `docs/PHASE_0D_MANUAL_CHECKLIST.md`.

## 15. Dependencies and licenses

Important resolved production dependencies:

- `rstar 0.12.2` - MIT OR Apache-2.0;
- `wgpu 0.20.1` - MIT OR Apache-2.0;
- `bytemuck 1.25.2` - Zlib OR Apache-2.0 OR MIT;
- `pollster 0.3.0` - Apache-2.0 OR MIT;
- `wasm-bindgen 0.2.126` and `js-sys 0.3.103` - MIT OR Apache-2.0;
- `uuid 1.24.0` - Apache-2.0 OR MIT;
- `serde 1.0.229`, `serde_json 1.0.151`, `thiserror 2.0.19` - permissive
  MIT/Apache-family terms as declared upstream.

The preview has no npm runtime or development dependency; `npm ci` audited one local package
and reported 0 vulnerabilities. Google Chrome is an external verification environment and is
not redistributed. `cargo tree --workspace` in the raw log is the authoritative resolved tree.

## 16. Known limitations

- Browser WebGPU does not expose the implementation backend string; native wgpu proves Vulkan
  separately. The browser proof records this as undisclosed rather than guessing.
- Rendering is intentionally limited to rectangle and ellipse primitives without clips,
  effects, paths, or text.
- GPU runs on the main thread; core, Scene, R-tree, RenderModel, commands, and history remain
  in the Worker. OffscreenCanvas Worker GPU was not assumed.
- Fixture replacement sends a full instance payload. Normal commands send dirty records only.
- Buffer capacity grows and is reused; this foundation does not compact a live buffer every
  frame.
- WebGPU absence is a blocking typed error, not a fallback success.
- The diagnostic surface is not a Phase 0E product UI.

## 17. Package and gate request

The review ZIP is created outside the workspace as
`C:\Users\thdwl\Documents\Codex\visual_authoring_engine_phase0d_review_2026-08-08.zip`.
It excludes `target`, `node_modules`, `.tools`, caches, IDE state, logs, nested ZIPs, and
credentials. `CHECKSUMS.sha256` inside the ZIP covers every other included file. The ZIP's own
digest is stored only in the external `.sha256` sidecar and final report to avoid
self-reference.

Work stops at Gate 0D. External Gate 0D review is requested; Phase 0E must not begin from this
packet without a separate authorization.