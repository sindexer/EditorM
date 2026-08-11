# Phase 0D Manual Checklist

The automated evidence was completed on 2026-08-09. This checklist lets a reviewer repeat the
observable diagnostic flow without implying Phase 0E product UI.

## Start

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-phase0d-preview.ps1
```

The script performs `npm ci --ignore-scripts`, builds the static preview, starts a hidden local
server, prints its PID and `http://127.0.0.1:4173/`, and opens the default browser unless
`-NoBrowser` is supplied.

## Automated evidence already completed

- [x] WASM initialized inside a Dedicated Worker.
- [x] Worker heartbeat, protocol version 1, and synchronized revisions were visible.
- [x] Main thread exposed no Document mutation API.
- [x] Actual Chrome 151 created a WebGPU adapter/device on NVIDIA GeForce GTX 970.
- [x] Rectangle and ellipse were visibly distinct in the captured preview.
- [x] DPR resize, drag-pan, pointer-centered zoom, and exact hit-test worked.
- [x] One-node command, undo, and redo crossed the Worker boundary.
- [x] BENCH-B rendered 10,000 rectangles with 1 batch and 1 instanced draw.
- [x] Offscreen BENCH-B submitted 0 instances.
- [x] Narrow BENCH-C queried fewer candidates than its 100,000 objects.
- [x] Worker restart reinitialized WASM and resumed heartbeat.
- [x] Browser console errors, GPU validation errors, and fallback rebuilds were 0.

Evidence: `docs/verification/PHASE_0D_BROWSER_PROOF.json` and
`docs/verification/phase0d-preview.png`.

## Reviewer repeat

- [ ] Confirm the HUD says `Worker core / main-thread WebGPU` rather than claiming Worker GPU.
- [ ] Drag the canvas and verify the Document/Scene/Render revisions do not change.
- [ ] Wheel over a visible shape and verify zoom remains centered under the pointer.
- [ ] Click the rectangle and ellipse and confirm a NodeId appears under hit-test.
- [ ] Click `Move one node`, `Undo`, then `Redo`; inspect dirty/upload counters.
- [ ] Select BENCH-B, click `Fit`, and confirm 10,000 submitted instances, 1 draw, 1 batch.
- [ ] Select BENCH-C and confirm narrow spatial candidates are far below 100,000.
- [ ] Click `Restart worker` and confirm heartbeat returns.
- [ ] Confirm the latest typed error says `none` and validation/fallback counts are 0.

## Expected failure mode

If WebGPU is unavailable, the preview displays `webgpu_unavailable` and Gate 0D is not treated
as passed. There is intentionally no Canvas2D or static-image success fallback.