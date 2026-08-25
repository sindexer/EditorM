# Phase 2A — Path Rendering and Hit Testing: Verification Record

Scope: connect the already-merged persistent Path schema to the real scene, hit test, render
model, WASM boundary and WebGPU pipeline. No Pen tool, no anchor editing UI, no caps, joins,
dashes or gradients.

## Verification status

CPU, WASM, and software-GPU diagnostics were followed by a fresh actual-hardware run on Windows.
Run `phase2a-20260825T142800Z-69ae0d0` tested committed source
`69ae0d01e3d231729a022ecbec568bb3593acf6f` on Chrome 151.0.7922.174 and an NVIDIA GeForce GTX
970. The tracked browser proof, pixel readback, screenshot, and metrics below are the executed
evidence for that run; the earlier software run remains diagnostic only.

| Item | Status | Evidence |
| --- | --- | --- |
| Shared segment/bounds/flatten/fill geometry (one Rust module) | PASS | `cargo test -p visual_authoring_core_math` |
| Exact curve bounds from derivative roots | PASS | `crates/core_math/src/path.rs`, `crates/document/src/change_tests.rs` |
| Path hit testing: straight stroke, cubic stroke, closed fill interior, open path never fills | PASS | `crates/runtime/src/tests.rs` |
| Derived tessellation cache; one edit re-tessellates one path | PASS | `crates/runtime/src/tests.rs`, `docs/PHASE_2A_PATH_METRICS.json` |
| Camera-only frame re-tessellates nothing and uploads no triangles | PASS | `crates/wasm_bridge/src/lib.rs` path tests, `docs/PHASE_2A_PATH_METRICS.json` |
| Path items excluded from the instanced primitive draw; ordered draw batches | PASS | `crates/runtime/src/tests.rs` |
| Undo/redo of create-path and set-path-geometry across Document/Scene/RenderModel/hit test | PASS | `crates/runtime/src/tests.rs`, `crates/wasm_bridge/src/lib.rs` |
| Negative cases: duplicate anchor id, non-finite anchor or handle, invalid open/closed counts, wrong node kind, failure atomicity | PASS | `crates/runtime/src/tests.rs`, `crates/wasm_bridge/src/lib.rs`, `crates/serialization/src/lib.rs` |
| Serialization: v1/v2/v3 load, v3 path round trip, save→load→save stability, anchor UUID preservation | PASS | `cargo test -p visual_authoring_serialization` |
| Phase 2A direct-WASM proof against the shipped package (42 checks) | PASS | `docs/verification/PHASE_2A_DIRECT_WASM_PROOF.json` |
| Gate 1B evidence unchanged by Phase 2A verification | PASS | `docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json` unmodified; SHA-256 pinned in `web/editor/tests/phase1b-direct-wasm.test.ts` |
| Render binary schema v3 + WGSL path pipeline published and consumed | PASS | `cargo test -p visual_authoring_renderer_wgpu`, `web/phase0d-preview` unit tests, `npm test` |
| Path CPU performance at 10/100/1000 paths, split into initial tessellation / static frame / single edit / camera-only | PASS | `docs/PHASE_2A_PATH_METRICS.json` |
| Actual Chrome + Dedicated Worker + WASM + actual WebGPU renders paths | PASS (27/27 browser assertions) | `docs/verification/PHASE_2A_BROWSER_PROOF.json` |
| WebGPU surface pixel readback for straight path, Bezier path, closed fill, stroke, background control | PASS (10/10 pixel assertions) | `docs/verification/PHASE_2A_PIXEL_READBACK.json`, `docs/verification/phase2a-paths.png` |
| Actual-GPU frame behaviour (draw calls, no triangle re-upload on camera frames) | PASS | `docs/PHASE_2A_BROWSER_METRICS.json` |

## Commands

CPU and contract verification (runs anywhere, no GPU needed):

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run --release -p visual_authoring_runtime --bin phase2a_path_bench
cd web\editor
npm.cmd test
npm.cmd run test:direct-wasm:phase2a
npm.cmd run build
cd ..\phase0d-preview
npm.cmd test
```

On Windows PowerShell use `npm.cmd`, not `npm`: the execution policy blocks `npm.ps1`.

### Before running the hardware proof

The proof must describe committed source, not a dirty tree. On the GPU machine:

```
git switch feat/phase2a-path-rendering
git pull --ff-only
git status --short
```

Do **not** run the hardware proof if any of these is modified:

```
crates/**
shared/**
web/editor/src/**
web/editor/scripts/**
web/editor/public/worker.js
```

`web/editor/public/pkg/**` is the generated WASM package this repository tracks. It is expected to
match the committed build; if it differs, rebuild it from the committed source
(`tools/build-phase0e-wasm.ps1`) and confirm `node tools/ci-wasm-smoke.mjs` reports
`render_binary_schema_version: 3` before proceeding, so the proof describes the shipped package.

### Hardware verification (Windows machine with a real GPU)

```
cd D:\Codex\EditorM\web\editor
npm.cmd run test:browser:phase2a:path-render
```

Optional environment variables, matching the Phase 1B harness:

- `PHASE0E_CHROME` — path to `chrome.exe` when it is not in the default location.
- `PHASE2A_NO_SANDBOX=1` — only when Chrome refuses to start sandboxed (container/root runs).
- `PHASE2A_ALLOW_SOFTWARE_GPU=1` — diagnostic only. It writes to separate `*_SOFTWARE_RUN.json`
  paths and can never be hardware evidence. A software renderer is never a PASS.

The run must exercise the whole real path: Chrome → React editor → Dedicated Worker → WASM →
RenderModel → WebGPU → hardware GPU.

Artifacts produced by a hardware run:

- `docs/verification/PHASE_2A_BROWSER_PROOF.json`
- `docs/verification/PHASE_2A_PIXEL_READBACK.json`
- `docs/verification/phase2a-paths.png`
- `docs/PHASE_2A_BROWSER_METRICS.json`

### What the hardware run must show

Environment:

```
actual_webgpu            = true
software_renderer        = false
worker_runtime_owner     = dedicated-worker
wasm_initialized         = true
render_binary_schema_version = 3
gpu validation errors    = 0
fallback rebuilds        = 0
```

Path behaviour, each asserted from a WebGPU surface readback rather than a page screenshot:

```
open straight path visible
open cubic path visible
closed filled path interior visible
stroked closed cubic visible
background control reads as background

stroke hit test          PASS
filled interior hit test PASS
outside-the-outline miss PASS

undo restores geometry   PASS
redo reapplies geometry  PASS
create/edit round trip   PASS
```

Frame behaviour recorded in `docs/PHASE_2A_BROWSER_METRICS.json`:

```
camera-only frame : path triangle upload = 0, retessellation = 0
static frame      : retessellation = 0
single path edit  : retessellated paths = 1
fallback rebuilds = 0
draw calls and uploaded vertex bytes recorded
```

## First Windows hardware run: environment good, harness race found

The first run on real hardware (GeForce, Maxwell, `browser-webgpu`) established that the
environment and the product boundary are sound:

```
actual_webgpu = true          Dedicated Worker = true
software_renderer = false     WASM initialized  = true
GPU validation errors = 0     schema version    = 3
fallback rebuilds = 0
```

It then stopped at the first hit-test click with:

```
EngineFailure: operation dispatch is not allowed while a transaction is active
fsm = "Moving"   history.transaction_active = true   interaction_active.kind = "move"
```

**Classification: HARNESS.** The engine guard is correct and was not weakened. A pointer press on
the canvas opens an interaction transaction, and `finishInteraction` in `web/editor/src/App.tsx`
closes it *asynchronously* after the pointer is released: it drains the drag queue, awaits
`commit_transaction`, and only then returns the FSM to Idle. CDP resolving `mouseReleased` says
nothing about that tail, so the harness's next request landed while the transaction was still
open. The editor does return to Idle; the harness simply did not wait for it.

The fix is state-based synchronisation, never a fixed delay:

- `waitForInteractionIdle(label)` polls the live proof object until `fsm` is Idle (or
  NestedEditing), `history.transaction_active` is false, `interaction_active` is null, and the
  interaction queue has nothing in flight or scheduled. An already-idle editor costs one poll.
- `clickPoint` ends with that wait, so a click is complete when it returns.
- Every crossing from real pointer input into direct command dispatch — `create_path`,
  `set_path_geometry`, `undo`, `redo`, `save_document`, `load_document` — asserts quiescence first.
- A timeout raises `interaction_quiescence_timeout` and records `fsm`, `transaction_active`,
  `interaction_active`, `interaction_queue`, `engine_sequence`, `gpu_frame_sequence` and
  `selection`.

That last point is what makes the next failure classifiable without guessing: **if the editor
returns to Idle and the harness moved too early, the harness is at fault; if the editor never
returns to Idle, the timeout evidence shows a product interaction-completion bug** — a transaction
leak — and the classification changes to PRODUCT.

`web/editor/tests/phase2a-path-contract.test.ts` pins this contract: the helper must exist and
consider every signal, `clickPoint` must use it, each command boundary must assert it, the
dedicated failure code and its evidence fields must be present, and no `await sleep` in the proof
body may exceed a polling interval.

The Phase 1A and 1B harnesses were inspected for the same pattern. They are theoretically exposed
to it, but every canvas click there is followed by a state condition (`selection_count === N`,
`engine_sequence > n`, an FSM assertion) rather than an immediate dispatch, and both passed on real
hardware. They are left untouched: changing verified Gate evidence harnesses without the hardware
to re-verify them would trade a real asset for a theoretical improvement.

## Shipped WASM matches the committed source

The tracked package under `web/editor/public/pkg/` is what a hardware run actually serves, so it
must be built from the commit being proved. It is verified by rebuilding and comparing:

```
cargo build --release --target wasm32-unknown-unknown -p visual_authoring_wasm_bridge
wasm-bindgen --target web --out-dir <scratch> --out-name engine_host \
  target/wasm32-unknown-unknown/release/visual_authoring_wasm_bridge.wasm
```

Two `wasm-bindgen` runs of the same build produce byte-identical output, so a mismatch is a real
signal rather than build noise. The package committed with the first Phase 2A commit did **not**
match: it differed in 132 bytes, all inside panic-location metadata, because it was built before a
later `clippy` fix moved `union_rect` in `crates/document/src/lib.rs` and shifted line numbers by
six. The behaviour was identical, but the artifact no longer described the committed source, so it
was rebuilt and `PHASE_2A_DIRECT_WASM_PROOF.json` regenerated against it
(`sha256 673050e5…`, 42/42 checks).

## Pre-flight for the hardware run

Before the GPU run, the requests the browser harness issues were exercised against the shipped
WASM in Node, so a harness-shaped defect cannot waste a hardware run. All 23 checks passed:

- the fitted camera keeps the 6-unit strokes about 9 device pixels wide, so pixel sampling is not
  measuring a sub-pixel line;
- every one of the nine sample points — straight stroke, its background control, the Bezier curve
  point, the chord control, the fill interior, the bounding-box corner outside the outline, the
  ring stroke, the ring fill and the far background — lands inside the canvas after `fit`;
- hit testing at those exact viewport coordinates picks the expected node: fill interior picks the
  filled path, the bbox corner picks nothing, the straight stroke picks the straight path, the
  curve point picks the cubic path, the open path's interior and the background are misses;
- `save_document` returns JSON whose shape the harness parses (four `kind.type == "path"` nodes
  with string anchor ids, `version: 3`), bounds survive `load_document`, and save → load → save is
  byte stable.

At that checkpoint the GPU half remained unproven. The later Windows hardware run identified at
the top of this record supplied that proof without rewriting the direct-WASM execution.

## Harness dry run in this container (diagnostic only, never evidence)

The Phase 2A browser harness was executed here once in software-GPU diagnostic mode
(`PHASE2A_ALLOW_SOFTWARE_GPU=1`, bundled Chromium, `--no-sandbox`) purely to check that the
harness itself starts the preview server, launches Chrome, attaches CDP and reaches the editor.

It stopped at `application_readiness_timeout` with `actual_webgpu: false` and
`wasm_initialized: false`: Chromium's SwiftShader path in this container cannot create the WebGPU
swap chain (`Could not find SharedImageBackingFactory ... WebGPUSwapChainTexture`), so the editor
never finishes booting. The Phase 1B harness fails at exactly the same point in this container —
see `PHASE_1B_BROWSER_SOFTWARE_RUN_FAILURE.json` — so this is an environment limit, not a defect
in the Phase 2A harness. The diagnostic output is kept at
`PHASE_2A_BROWSER_SOFTWARE_RUN_FAILURE.json` and is **not** evidence of anything about paths. It
did not change the status; the separate later actual-hardware run is the execution that produced
the PASS evidence.

## Evidence separation

Phase 1B evidence is an immutable historical artifact of the Gate 1B run on commit `330476b5`. No
Phase 2A command regenerates it:

- `scripts/direct-wasm-phase1b.mjs` now writes nothing unless an explicit `--output=<path>` is
  given, so running it as a regression check cannot overwrite history. Only
  `npm.cmd run test:direct-wasm:phase1b:gate`, which the Gate 1B runner invokes, passes that path.
- `tests/phase1b-direct-wasm.test.ts` pins the artifact's SHA-256, so any stray regeneration fails
  the default suite instead of silently replacing Gate 1B evidence.
- Phase 2A has its own direct-WASM artifact, `docs/verification/PHASE_2A_DIRECT_WASM_PROOF.json`,
  produced by `scripts/direct-wasm-phase2a.mjs` against the WASM package this repository currently
  ships (42 checks: PATH-A buffers and batches, camera-only upload contract, create/edit/undo/redo,
  and the negative cases).

## Measured CPU path performance

From `docs/PHASE_2A_PATH_METRICS.json`, on the container CPU (release build, closed cubic paths
with four curved segments each; the most expensive path kind Phase 2A draws):

| Paths | Initial tessellation | Static frame | Single path edit | Camera-only frame |
| --- | --- | --- | --- | --- |
| 10 | 1.0 ms | 0.015 ms | 0.09 ms | 0.007 ms |
| 100 | 8.8 ms | 0.083 ms | 0.13 ms | 0.028 ms |
| 1000 | 89.9 ms | 0.348 ms | 0.39 ms | 0.044 ms |

Structural results, which matter more than the timings: a single path geometry edit
re-tessellates exactly **1** path at every count, and a camera-only frame re-tessellates **0**
and uploads **0** triangles at every count. Full render rebuilds and fallback rebuilds are 0.

## Known limitations, recorded rather than hidden

- **Initial tessellation cost.** Building 1,000 curved closed paths costs ~90 ms of CPU
  tessellation once. It is not per frame and not per edit, but it is real, and a later phase that
  wants instant load of path-heavy documents will need incremental or parallel tessellation.
- **Stroke geometry is minimal by scope.** Each segment is stroked as a quad with a feather; there
  are no caps, joins, miter limits or dashes. Phase 2B owns those.
- **Self-intersecting closed paths are not filled.** They produce stroke triangles only, and their
  interior is not pickable. This is deliberate: the fill rule and the triangulator agree, so the
  hit test never claims a region the renderer does not draw.
- **Feather width is one logical pixel.** On a high-DPR display the feather is one CSS pixel wide
  rather than one device pixel, so path edges are marginally softer than the analytic primitives'
  fragment-derivative antialiasing.
- **Paths are not part of the analytic primitive batch.** A frame that interleaves many paths and
  many primitives in z-order issues one draw call per contiguous run. Documents that alternate
  path/primitive/path/primitive at every z-level pay one draw call per item.
