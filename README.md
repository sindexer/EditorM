# EditorM Visual Authoring Engine

Phase 0E-R3 is complete and preserved as the approved GitHub baseline. Phase 1A and the Phase 1B implementation have merged to `main`. The editor opens on a real 1920×1080 Frame and supports engine-owned multiple selection, rubber-band selection, alignment, distribution, and object snapping, all undoable in one step. Gate 1B remains not passed until the merged source is verified on the required Windows hardware path.

The live product path is not mocked. A Dedicated Worker owns the Rust/WASM EngineRuntime; versioned projection and render deltas cross the Worker boundary; the main thread submits them to actual WebGPU. React owns only disposable UI projection and interaction state.

## Run the editor on Windows

    powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-editor.ps1

The script checks the required toolchain, builds the WASM bridge, installs locked Editor dependencies when absent, builds the production UI, selects an available localhost port, health-checks the server and assets, prints the URL, and opens the default browser. Stop it with Ctrl+C.

Actual WebGPU is required. WebGPU absence, adapter/device failure, Worker boot failure, WASM failure, or asset/MIME failure is a blocking typed error; there is no Canvas2D, mock-renderer, or static-image success fallback.

## Phase 1B user path

1. Shift-click, or drag a rubber band on empty canvas, to select several objects. `Ctrl/Cmd+A` selects everything at the top level of the current root.
2. Drag any selected object to move the whole selection; edges and centers snap to nearby objects and a guide shows the match.
3. Toggle snapping in the canvas toolbar, or hold `Alt` to suspend it for part of a drag.
4. Use the align buttons with two or more objects selected, and the distribute buttons with three or more.
5. Undo once to reverse a whole alignment, distribution, or multi-object drag.

Alignment, distribution, and snapping are planned inside the Rust runtime and applied as typed commands in one transaction. The React editor never computes a transform; it sends a request and draws what the engine reports. Transform handles stay on a single selection in this phase.

## Phase 1A user path

1. Confirm the default Frame 1920×1080 in Layers and on canvas.
2. Press F and create Full HD, 4K UHD, DCI 4K, or a custom Frame.
3. Use R or O to draw a Rectangle or Ellipse.
4. Select a primitive and edit fill, opacity, uniform corner radius, stroke color, and centered stroke width in Inspector.
5. Move or resize the selection directly on canvas.
6. Use Ctrl+Z / Ctrl+Y or the toolbar for undo/redo.
7. Use Fit selection for the current Frame.

Fill/stroke colors are stored as sRGB and converted to linear space at the RenderModel boundary. The shared WGSL returns premultiplied linear output and uses derivative-based analytic coverage without fragment discard.

## Workspace

- crates/document: persistent Frame and primitive appearance truth, typed commands, transactions, history, atomic validation, and multiple-selection state.
- crates/serialization: version 2 appearance persistence and version 1 default migration.
- crates/scene: stroke-aware bounds, culling, spatial index, and exact hit testing.
- crates/runtime: scene-aware runtime plus `arrange` (alignment and distribution planning) and `snap` (bounded band snapping and alignment guides).
- crates/render_model: stable slots, sRGB-to-linear conversion, and bounded per-item deltas.
- crates/renderer_wgpu: native WebGPU backend using the shared WGSL and binary schema.
- crates/wasm_bridge: Worker-owned EngineHost, projection, and render-binary protocol.
- shared: render binary schema v2 and analytic-AA WGSL used by native and browser renderers.
- web/editor: Wanted Design System-adapted React editor shell and actual WebGPU renderer.
- web/phase0d-preview: retained Phase 0 diagnostic preview.
- docs/adr: immutable ADR-001–041, Phase 1A ADR-042–044, and Phase 1B ADR-045–047.

## Verification

    $env:VAE_TOOL_ROOT='C:\path\to\WebEditor\.tools'
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
    cd web/editor
    npm test
    npm run build

Phase 1B adds three more checks:

    cd web/editor
    npm run test:direct-wasm:phase1b
    npm run bench:multi-drag:phase1b
    npm run test:browser:phase1b

The first loads the pinned `engine_host` WASM package in Node and exercises multiple selection, rubber-band selection, snapped translation, alignment, distribution, and undo against the actual compiled engine. The second measures one multi-object drag frame for 10, 100, and 1,000 selected objects. Neither produces GPU evidence and neither claims any.

The third is the Gate 1B proof: it builds the editor, launches a new Chrome process, and drives the real Editor through pointer and keyboard input across the Worker, WASM, and actual WebGPU, including pixel evidence for the selection overlay and the snap guide. It requires a real GPU and fails closed without one. See [docs/PHASE_1B_HARDWARE_RUN.md](docs/PHASE_1B_HARDWARE_RUN.md) for the hardware run, and [docs/verification/PHASE_1B_GATE_STATUS.json](docs/verification/PHASE_1B_GATE_STATUS.json) for what is still unverified.

Browser proof commands and new evidence paths are recorded in the Phase 1A and Phase 1B review packets. Keep target, node_modules, dist, credentials, ZIP files, and temporary browser profiles out of commits.

## Phase boundary

Phase 1A includes visible Frame creation, solid primitive appearance, direct single-selection manipulation, undo/redo, analytic AA, and real WebGPU proof.

Phase 1B adds multiple selection, rubber-band selection, multi-object movement, object snapping with guides, alignment, and distribution. It intentionally excludes multi-selection resize and rotate, pixel-grid snapping, spacing measurements, rulers and user guides, Pen/Bezier, text, gradients, images, shadows, auto layout, components, motion, AI integration, export, and transform-aware Frame clipping.

Phase 1B carries no browser, WebGPU, or pixel evidence yet: its proof harness exists and fails closed without a GPU. See [docs/REVIEW_PACKET_1B.md](docs/REVIEW_PACKET_1B.md) for exactly what has and has not been executed.
