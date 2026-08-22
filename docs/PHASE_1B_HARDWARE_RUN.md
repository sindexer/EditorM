# Phase 1B Hardware Verification Run

Gate 1B needs evidence that can only be produced on a machine with a real GPU. Everything in this
document is written to be run as-is on the Windows workstation that produced the Phase 1A
hardware evidence. Nothing here has been executed in the container that prepared this branch; see
[PHASE_1B_GATE_STATUS.json](verification/PHASE_1B_GATE_STATUS.json) for what is still UNVERIFIED.

## Prerequisites

- Google Chrome with working WebGPU (the Phase 1A run used Chrome 151 on an NVIDIA GeForce GTX 970).
- The pinned Rust toolchain and the verified `wasm-bindgen` 0.2.126 CLI, as in Phase 1A.
- No other Chrome instance is required; each harness starts its own process with a throwaway profile.

    $env:VAE_TOOL_ROOT='C:\path\to\WebEditor\.tools'
    # Only if Chrome is not at the default location the harnesses assume:
    $env:PHASE0E_CHROME='C:\Program Files\Google\Chrome\Application\chrome.exe'

## 1. Rebuild the engine and the pinned WASM package

    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0e-wasm.ps1

## 2. Phase 1A regression re-run

    cd web/editor
    npm.cmd ci
    npm.cmd run test:browser:phase1a

Evidence written:

- `docs/verification/PHASE_1A_BROWSER_PROOF.json`
- `docs/verification/PHASE_1A_PIXEL_READBACK.json`
- `docs/verification/phase1a-*.png`

A failing run writes `docs/verification/PHASE_1A_BROWSER_FAILURE.json` instead. Preserve the
failure file; do not overwrite the previously committed Phase 1A evidence with a passing rerun
unless the gate reviewer asks for a refreshed capture.

## 3. Phase 1B browser and WebGPU proof

    cd web/editor
    npm.cmd run test:browser:phase1b

This builds the editor, starts the preview server on a free port, launches a new Chrome process
with a throwaway profile, and drives the real Editor through pointer and keyboard input only. It
verifies, in the browser: shift multi-selection, rubber-band selection, `Ctrl+A`, multi-object
drag, drift-free coalesced drag over 64 pointer frames, edge/center snapping, Alt snap suspend,
the snap toggle, snap-guide rendering, all six align operations, both distribute operations,
arrange-then-undo, and multi-drag-then-undo. It then runs the multi-selection drag benchmark for
10, 100, and 1,000 objects with snapping off and on.

Evidence written:

- `docs/verification/PHASE_1B_BROWSER_PROOF.json` — the run, its checks, and the GPU adapter.
- `docs/verification/PHASE_1B_PIXEL_READBACK.json` — pixel evidence for the selection outline,
  union bounds, rubber band, snap guide, and the dragged rectangle on the WebGPU surface.
- `docs/PHASE_1B_METRICS.json` — the browser-side multi-drag benchmark.
- `docs/verification/phase1b-multi-selection.png`, `phase1b-marquee.png`, `phase1b-snap-guide.png`,
  `phase1b-aligned.png`, `phase1b-distributed.png`.

A failing run writes `docs/verification/PHASE_1B_BROWSER_FAILURE.json` and exits non-zero. The
harness fails closed: no WebGPU, a software renderer, a Worker or WASM failure, a fallback
rebuild, or a console error all stop the run instead of degrading it.

The harness rejects software renderers by design. `PHASE1B_ALLOW_SOFTWARE_GPU=1` runs it anyway
for diagnostics and writes to `*_SOFTWARE_RUN.json` paths that are explicitly not gate evidence.
`PHASE1B_NO_SANDBOX=1` exists only for root containers and is never needed on Windows.

## 4. Engine-side checks that do not need a GPU

    cd web/editor
    npm.cmd run test:direct-wasm:phase1b
    npm.cmd run bench:multi-drag:phase1b
    npm.cmd test
    npm.cmd run build

Evidence written: `docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json` and
`docs/PHASE_1B_METRICS_ENGINE_ONLY.json`. These already passed in the preparing container and are
committed; re-running them on Windows confirms the same result on the gate machine.

## 5. Update the gate record

After the hardware runs, update `docs/verification/PHASE_1B_GATE_STATUS.json` so every item that
now has evidence moves from `UNVERIFIED` to `PASS` or `FAIL`, set `browser_proof_present` to true,
and set `gate_conclusion`. `web/editor/tests/phase1b-gate-status.test.ts` enforces that a `PASS`
item names artifacts that exist and that no GPU-dependent item claims `PASS` while the browser
proof artifact is missing, so the record cannot drift from the evidence.
