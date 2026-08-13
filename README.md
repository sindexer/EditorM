# EditorM Visual Authoring Engine

Phase 0E-R3 is complete and preserved as the approved GitHub baseline. Phase 1A is in development on feat/phase1a-visible-frame-primitives: the editor now opens on a real 1920×1080 Frame and uses shared analytic anti-aliasing for rectangle, rounded rectangle, Frame, and ellipse rendering.

The live product path is not mocked. A Dedicated Worker owns the Rust/WASM EngineRuntime; versioned projection and render deltas cross the Worker boundary; the main thread submits them to actual WebGPU. React owns only disposable UI projection and interaction state.

## Run the editor on Windows

    powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-editor.ps1

The script checks the required toolchain, builds the WASM bridge, installs locked Editor dependencies when absent, builds the production UI, selects an available localhost port, health-checks the server and assets, prints the URL, and opens the default browser. Stop it with Ctrl+C.

Actual WebGPU is required. WebGPU absence, adapter/device failure, Worker boot failure, WASM failure, or asset/MIME failure is a blocking typed error; there is no Canvas2D, mock-renderer, or static-image success fallback.

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

- crates/document: persistent Frame and primitive appearance truth, typed commands, transactions, history, and atomic validation.
- crates/serialization: version 2 appearance persistence and version 1 default migration.
- crates/scene: stroke-aware bounds, culling, spatial index, and exact hit testing.
- crates/render_model: stable slots, sRGB-to-linear conversion, and bounded per-item deltas.
- crates/renderer_wgpu: native WebGPU backend using the shared WGSL and binary schema.
- crates/wasm_bridge: Worker-owned EngineHost, projection, and render-binary protocol.
- shared: render binary schema v2 and analytic-AA WGSL used by native and browser renderers.
- web/editor: Wanted Design System-adapted React editor shell and actual WebGPU renderer.
- web/phase0d-preview: retained Phase 0 diagnostic preview.
- docs/adr: immutable ADR-001–041 plus Phase 1A ADR-042–044.

## Verification

    $env:VAE_TOOL_ROOT='C:\path\to\WebEditor\.tools'
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
    cd web/editor
    npm test
    npm run build

Browser proof commands and new evidence paths are recorded in the Phase 1A review packet. Keep target, node_modules, dist, credentials, ZIP files, and temporary browser profiles out of commits.

## Phase boundary

Phase 1A includes visible Frame creation, solid primitive appearance, direct single-selection manipulation, undo/redo, analytic AA, and real WebGPU proof. It intentionally excludes snapping, multi-selection transforms, Pen/Bezier, text, gradients, images, shadows, auto layout, components, motion, AI integration, export, and transform-aware Frame clipping.
