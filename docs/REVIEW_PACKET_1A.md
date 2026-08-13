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
- Shared native/browser WGSL uses derivative-based analytic coverage (`fwidth`/`smoothstep`) with no fragment `discard`, sRGB-to-linear conversion, and a premultiplied-linear alpha/blend contract.
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
| `tools/cargo.ps1 test --workspace --all-targets` | PASS: 182 passed, 0 failed, 0 ignored |
| `tools/build-phase0e-wasm.ps1` | PASS |
| generated shared WGSL contract `--check` | PASS |
| `npm.cmd test` | PASS: 3 files, 15 tests |
| `npm.cmd run build` | PASS |
| `npm.cmd run test:browser:phase1a` | PASS: fresh actual hardware run plus 4 artifact assertions |

The complete command/result record is [PHASE_1A_VERIFICATION.txt](verification/PHASE_1A_VERIFICATION.txt).

## Actual Chrome, Worker, WASM, and WebGPU proof

Fresh capture: `2026-08-13T17:27:09.913Z`.

- Chrome `151.0.7922.110`, new process, new temporary profile.
- NVIDIA GeForce GTX 970 actual WebGPU device.
- Dedicated Worker owns an initialized WASM EngineHost; heartbeat is at least 1.
- Server health and HTML/Worker/JS/WASM assets including WASM MIME pass.
- Default 1080p Frame, 4K create/select/undo/redo, Inspector appearance, direct move/resize/undo/redo, stable single-slot updates, and engine/GPU/overlay sequence agreement pass.
- Checks: 18/18; console errors: 0; GPU validation errors: 0; fallback rebuilds: 0.
- The harness owns port allocation, preview start/health, `PHASE0D_URL` equivalent routing, Chrome process/profile, proof capture, and cleanup. It does not require a pre-existing server.

## Pixel evidence

[PHASE_1A_PIXEL_READBACK.json](verification/PHASE_1A_PIXEL_READBACK.json) records 17 passing actual WebGPU readback cases. It covers every combination of DPR 1/1.25/1.5/2 and zoom 25%/100%/400%, plus a circle, a rotated nonuniform ellipse, a rounded rectangle, black/white backgrounds, opacity, fill, and centered stroke. Every required edge has partial analytic coverage; binary-only edges and halo conditions are rejected.

Screenshots:

- [Default 1080p Frame](verification/phase1a-editor-default.png)
- [Open 4K Frame preset menu](verification/phase1a-frame-4k.png)
- [Appearance and analytic AA](verification/phase1a-appearance-aa.png)
- [DPR/zoom matrix state](verification/phase1a-dpr-zoom-matrix.png)

## Actual GTX 970 performance

[PHASE_1A_METRICS.json](PHASE_1A_METRICS.json) is actual native wgpu/Vulkan evidence and retains every raw frame sample.

| Fixture | Visible | Warmup / measured | Median | p95 | Max | Interpretation |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| BENCH-A | 1,000 | 30 / 300 | 0.3838 ms | 0.6052 ms | 1.0244 ms | Gate threshold `p95 <= 16.7 ms`: PASS |
| BENCH-B | 10,000 | 10 / 60 | 0.5000 ms | 0.6535 ms | 0.7359 ms | Comparative evidence only |

Both use a 1920×1080 viewport, DPR 1, one batch, one draw call, and actual NVIDIA GeForce GTX 970 hardware. Warmup and evidence serialization are outside measured frame samples.

## Preserved failures and corrections

[PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json](verification/PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json) preserves a pre-fix `condition_timeout` waiting for the Resizing FSM; it is not presented as a pass. Fresh hardware proof also exposed a nonuniform ellipse centered-stroke quad clipping defect and a transposed affine selection-overlay mapping. The quad expansion, overlay mapping, and direct resize hit path were corrected before the final proof.

A first `npm.ps1` validation entry did not execute because of Windows execution policy and was not counted as a pass; final commands use `npm.cmd`. Two failures while authoring the new artifact test (one TypeScript type inference error and one temporary-profile expectation mismatch) were corrected before the final 15/15 and 4/4 passing runs.

## Evidence index and hashes

- `docs/verification/PHASE_1A_BROWSER_PROOF.json` — `8caa83cb07b8a51c6f7e3827e97d6cb862194d5fa73f34bc9e7b1084c57c792d`
- `docs/verification/PHASE_1A_PIXEL_READBACK.json` — `4deb37d439c412303aa56077edef16129be7d2a03baceb99758729737c280b86`
- `docs/PHASE_1A_METRICS.json` — `b74162be5fc66ca84c363c5decf53bd30fba1bd6387a0da684738986c1cbb777`
- `docs/verification/PHASE_1A_BROWSER_INCIDENT_PRE_FIX.json` — `6353bb842f7f474b38012a052969df2962e1108734bdb0ac26b73c10509e1668`
- `docs/verification/phase1a-editor-default.png` — `5ceec717d942e439dbb0d9ed630283869ff01bb4d0bd666a8651d2c1c5686859`
- `docs/verification/phase1a-frame-4k.png` — `19151523b81ca6e899cd28e08d79a3f3a74453b4df40c9309c83d9895cabbd7d`
- `docs/verification/phase1a-appearance-aa.png` — `10043df2cc842e7f8508ec62148ddb3f296602e06e3311caa336167956cfb1cc`
- `docs/verification/phase1a-dpr-zoom-matrix.png` — `3b6585743a733dc31b04db910f5a18c75c2ec5d36ef4a9ebffe172c8f761fe02`

## Run locally

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-editor.ps1
```

The script checks dependencies, builds WASM when needed, installs Editor dependencies when needed, starts the localhost server, checks health, and opens the default browser. The running editor uses the real Dedicated Worker/WASM/WebGPU path; it has no mock or Canvas2D product fallback.