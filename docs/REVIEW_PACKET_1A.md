# Phase 1A Review Packet

## Disposition

This packet requests external review of Phase 1A: visible Frame canvas and anti-aliased primitive appearance. Implementation and current evidence are complete, but this packet does not claim Gate 1A approval. Phase 1B has not started, the PR must not be merged by this task, and no review ZIP was created.

Baseline: main merge commit `277bc71361da5b50a2d3168856ce0b3cb83d0463`; approved Phase 0E-R3 payload `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`; approved tag `phase-0e-r3-approved`.

## Delivered scope

- The Editor opens with a real, selected `Frame 1920×1080` and can create 1080p, 4K UHD, DCI 4K, and validated custom Frames through the `F` tool menu.
- Frame creation has stable collision-free naming, Layer/Inspector projection, fit-selection, direct single-selection move/resize, and undo/redo.
- Rectangle, Frame, and Ellipse persist solid sRGB fill RGBA, independent opacity, four corner radii, and one centered solid stroke.
- Typed commands cross the Worker-owned Rust/WASM boundary; invalid finite/range inputs fail atomically without partial Document, History, Selection, Scene, Render, or GPU mutation.
- Document format version 2 stores appearance while loading version 1 with defaults. Request protocol remains version 1. Render binary schema advances independently to version 2 with 112-byte instances and 116-byte dirty records.
- Rust, WGSL, browser proof, selection overlay, and hit-test share `[m11,m12,m21,m22,tx,ty]` semantics: `x=m11*x+m12*y+tx`, `y=m21*x+m22*y+ty`.
- Browser rendering keeps the preferred base format and uses compatible sRGB pipeline, canvas, and readback views; unsupported sRGB views fail closed.
- Ellipse stroke distance uses a gradient-correct local-unit approximation, conservative per-axis half-stroke expansion, and matching bounds/culling/hit-test semantics.
- Fill and stroke contribute mutually exclusive premultiplied coverage, then object opacity is applied exactly once; black/white paired readback rejects halo, dip, overshoot, and undershoot.
- Centered strokes affect bounds/culling and dirty only the edited stable slot. Single appearance and geometry edits show zero full RenderModel clone, zero unrelated item scan, and zero fallback rebuild in current proof.

## Explicit exclusions

This phase does not implement snapping, multi-selection transforms, Pen/Bezier, text, gradients, image fill, shadows, auto layout, components/variants, motion, AI integration, multiple projects, export, or fully transform-correct Frame clipping. It does not start Phase 1B.

## Decisions

- [ADR-042](adr/ADR-042-versioned-primitive-appearance-schema.md): versioned persistent appearance and independent render schema.
- [ADR-043](adr/ADR-043-analytic-primitive-coverage-and-alpha.md): shared analytic coverage, color conversion, and premultiplied alpha.
- [ADR-044](adr/ADR-044-frame-creation-and-preset-semantics.md): Frame presets, naming, validation, selection, and history semantics.

ADR-001 through ADR-041 and all existing Phase 0 historical evidence remain unchanged.

## Required verification

All commands below were run on Windows against the final source state:

| Validation | Result |
| --- | --- |
| `tools/cargo.ps1 fmt --all -- --check` | PASS |
| `tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings` | PASS |
| `tools/cargo.ps1 build --workspace --all-targets` | PASS |
| `tools/cargo.ps1 test --workspace --all-targets` | PASS: 183 passed, 0 failed, 0 ignored |
| `tools/build-phase0e-wasm.ps1` | PASS |
| generated shared WGSL contract `--check` | PASS |
| Preview `npm.cmd test` / `npm.cmd run build` | PASS: 7 tests; 9 verified build assets |
| Editor `npm.cmd test` | PASS: 3 files, 17 tests |
| `npm.cmd run build` | PASS |
| `npm.cmd run test:browser:phase1a` | PASS: fresh actual hardware run plus 5 stored-proof tests |

The complete command/result record is [PHASE_1A_VERIFICATION.txt](verification/PHASE_1A_VERIFICATION.txt). The rebuilt pinned WASM package is 1,284,737 bytes with SHA-256 `aa8444cc8a5120b6d9c97ebeafc401cac2b53152f69e6451f5b4f73b4b45f492`; the fresh browser run instantiates EngineHost with protocol v1 and render schema v2.

## Actual Chrome, Worker, WASM, and WebGPU proof

Fresh capture: `2026-08-20T05:22:10.457Z`.

- Chrome `151.0.7922.138`, new process PID 17468, new temporary profile.
- NVIDIA GeForce GTX 970 actual WebGPU device, driver `32.0.15.8157`.
- Dedicated Worker owns an initialized WASM EngineHost; heartbeat is at least 1.
- Surface base `bgra8unorm`; pipeline and readback views `bgra8unorm-srgb`.
- Server health and HTML/Worker/JS/WASM assets including WASM MIME pass.
- Default 1080p Frame, 4K create/select/undo/redo, affine render/hit/overlay parity, Inspector appearance, direct move/resize/undo/redo, stable single-slot updates, and engine/GPU/overlay sequence agreement pass.
- Browser checks: 26/26; pixel assertions: 16/16; console errors: 0; GPU validation errors: 0; fallback rebuilds: 0.
- The harness owns port allocation, preview start/health, `PHASE0D_URL` equivalent routing, Chrome process/profile, proof capture, and cleanup. It does not require a pre-existing server.

## Pixel evidence

[PHASE_1A_PIXEL_READBACK.json](verification/PHASE_1A_PIXEL_READBACK.json) records 48 passing ellipse stroke measurements across four shapes, DPR 1/1.25/1.5/2, and zoom 25%/100%/400%, plus 17 analytic-AA cases.

- Stroke width is measured by integrated connected linear coverage on an isolated black backdrop with a 1 physical pixel tolerance; maximum observed error is 0.7144 px.
- Three fill-only/stroke-only/fill+stroke cases use paired black/white samples; maximum linear channel error is 0.00554 against tolerance 0.055.
- The non-symmetric affine case, sRGB view linkage, halo rejection, boundary alpha continuity, clipping, and render/hit/overlay parity all fail closed.

Screenshots:

- [Default 1080p Frame](verification/phase1a-editor-default.png)
- [Open 4K Frame preset menu](verification/phase1a-frame-4k.png)
- [Appearance and analytic AA](verification/phase1a-appearance-aa.png)
- [DPR/zoom matrix state](verification/phase1a-dpr-zoom-matrix.png)
- [sRGB color contract](verification/phase1a-color-contract.png)
- [Affine render/hit/overlay parity](verification/phase1a-affine-parity.png)

## Actual GTX 970 performance

[PHASE_1A_METRICS.json](PHASE_1A_METRICS.json) is actual native wgpu/Vulkan evidence and retains every raw frame sample.

| Fixture | Visible | Warmup / measured | Median | p95 | Max | Interpretation |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| BENCH-A | 1,000 | 30 / 300 | 0.2962 ms | 0.7028 ms | 1.8826 ms | Gate threshold `p95 <= 16.7 ms`: PASS |
| BENCH-B | 10,000 | 10 / 60 | 0.6036 ms | 0.7990 ms | 1.6970 ms | Comparative evidence only |

Both use a 1920×1080 viewport, DPR 1, one batch, one draw call, and actual NVIDIA GeForce GTX 970 hardware. Warmup and evidence serialization are outside measured frame samples.

## Preserved failures and corrections

[PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json](verification/PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json) preserves the original pre-fix resize timeout and is not presented as a pass. `PHASE_1A_BROWSER_FAILURE*.json`, `PHASE_1A_BROWSER_PROOF_FAILED_*.json`, and `PHASE_1A_PIXEL_READBACK_FAILED_*.json` preserve subsequent harness/readback failures rather than being deleted or rewritten as passes.

Those runs exposed an out-of-viewport affine sample, asynchronous pointer-up races, non-isolated color backdrops, threshold-based subpixel width overcounting, opposite-edge contamination, and inward-AA misclassification. Each failure remains recorded; the final proof uses bounded discriminating points, press/transaction synchronization, isolated black/white backdrops, integrated connected coverage, and outward-only clipping validation.

An initial targeted Rust compile failed because `Vec2::length` did not exist; it was corrected to `f64::hypot` and was not represented as a pass. A first `npm.ps1` validation entry did not execute because Windows execution policy blocked it. Final passing counts are Rust 183, Editor 17, Preview 7, and browser stored-proof 5.

## CI first-run incident

The first PR Actions run, [31726409602](https://github.com/sindexer/EditorM/actions/runs/31726409602), is preserved as a failure. Rust (9m04s), Editor, integrity, and classification passed. WASM smoke still hard-coded the approved Phase 0 binary hash/schema v1; Preview tests still expected schema v1 and `discard`; and the hardware validator only recognized the Phase 0E-R3 proof shape. These were validation-contract gaps exposed by the authorized Phase 1 schema change, not retries hidden as passes. The follow-up keeps the legacy Phase 0 proof branch, validates current checked-in and fresh pinned schema v2 WASM initialization, asserts analytic AA/no-discard, and requires explicit Phase 1A execution/no-mock/no-Canvas2D fields. The legacy Phase 0E-R3 proof was structurally revalidated after the compatibility change.

## Evidence index and hashes

- `docs/verification/PHASE_1A_BROWSER_PROOF.json` — `86f795f2b95911d7f0a0d91bc13d66630886b7b45f6928b34b4fed9baa6f9bc6`
- `docs/verification/PHASE_1A_PIXEL_READBACK.json` — `0379f046e97495cbb0199b41fc8371815c7f325ff13cfa2d6f01e26a23708894`
- `docs/PHASE_1A_METRICS.json` — `9a4882a67a53f099776b3e20ea0b06f9882d17cbaffb32db9f3f5ae42b556a66`
- `docs/verification/PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json` — `6353bb842f7f474b38012a052969df2962e1108734bdb0ac26b73c10509e1668`
- `docs/verification/phase1a-editor-default.png` — `a7c4923378db301d567220135c884e69d0347bd533f9c2281bedb7298301c816`
- `docs/verification/phase1a-frame-4k.png` — `8b89c7ca12a90652caf08e66b8c81f0df4466e7aec896f4e9cc772aa07bc8dfc`
- `docs/verification/phase1a-appearance-aa.png` — `637130bb0e62894fc29463ccf69d9ab61b635f1fbd3b608e2539281d2a23e0b1`
- `docs/verification/phase1a-dpr-zoom-matrix.png` — `740bd2655a0f72ec31cba540fcbe023864525b707fb08d9212ec0ddf2f71d609`
- `docs/verification/phase1a-color-contract.png` — `d74d6e9139b6c574f10b9414764727e747f29ae07c7416977034776dbad70610`
- `docs/verification/phase1a-affine-parity.png` — `9c69a6b2f530d84e0e3b28c270f969ecfffb1c6129b093a9feba2b7023942f9e`

## Run locally

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-editor.ps1
```

The script checks dependencies, builds WASM when needed, installs Editor dependencies when needed, starts the localhost server, checks health, and opens the default browser. The running editor uses the real Dedicated Worker/WASM/WebGPU path; it has no mock or Canvas2D product fallback.