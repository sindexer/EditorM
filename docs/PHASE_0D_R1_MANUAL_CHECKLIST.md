# Phase 0D-R1 Manual Checklist

Date: 2026-08-10 (Asia/Seoul)

This is a Gate 0D-R1 diagnostic checklist. It does not authorize or describe a Phase 0E
product editor.

## Automated evidence completed

- [x] `npm run test:browser` selected its own localhost port, started and health-checked the
  preview server, verified HTML/Worker/JS/WASM status and MIME, and cleaned up the server,
  Chrome process, and temporary Chrome profile.
- [x] Chrome 151 created a real WebGPU device on NVIDIA GeForce GTX 970; no mock, Canvas2D, or
  static-image success fallback was used.
- [x] Worker-owned WASM initialized and heartbeat resumed after a real Worker restart.
- [x] Engine response sequence, GPU frame sequence, and GPU metrics sequence matched.
- [x] A delayed older frame could not replace a newer frame; a restart rejected its pending
  waiter with typed `worker_restarted`.
- [x] A 500-event pointer burst and 120-event wheel burst were coalesced with at most one camera
  request in flight.
- [x] A finite f64 translation of `1e100` succeeded at the Document boundary, emitted a typed
  `translation_outside_f32` diagnostic, zeroed/omitted one GPU slot, and recovered the same slot.
- [x] Actual GPU pixel readback distinguished rectangle fill, ellipse fill, ellipse AABB
  background, reorder, undo, redo, and offscreen background.
- [x] Browser console errors, WebGPU validation errors, and fallback rebuilds were zero.

Evidence:

- `docs/verification/PHASE_0D_R1_BROWSER_PROOF.json`
- `docs/verification/PHASE_0D_R1_PIXEL_READBACK.json`
- `docs/verification/phase0d-r1-preview.png`
- `docs/PHASE_0D_R1_METRICS.json`

## Reviewer repeat

Run the self-contained hardware proof without starting a server first:

```powershell
cd web/phase0d-preview
npm run test:browser
```

Then optionally start the diagnostic preview:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-phase0d-preview.ps1
```

- [ ] Confirm the HUD says `Worker core / main-thread WebGPU`.
- [ ] Confirm response sequence, GPU sequence, and revisions agree.
- [ ] Drag and wheel quickly; confirm coalesced/dropped counters rise and the view reaches the
  final input position.
- [ ] Click `Move one node`, `Undo`, and `Redo`; confirm one dirty instance and no full rebuild.
- [ ] Reorder the overlap fixture through the proof and confirm topmost color changes, then undo
  and redo restore the expected colors.
- [ ] Select BENCH-B, Fit, and confirm 10,000 submitted instances with one draw and one batch.
- [ ] Select BENCH-C and confirm spatial candidates are much smaller than 100,000.
- [ ] Click `Restart worker`; confirm heartbeat returns and no old-generation frame appears.
- [ ] Confirm browser console, GPU validation, and fallback counters remain zero.

## Expected blocking diagnostics

The browser harness reports distinct typed failures for server connection/health, asset HTTP
status, WASM MIME, WASM boot, Worker request, WebGPU initialization, and general application
readiness. Any such error blocks Gate 0D-R1; increasing a timeout or reading an old proof JSON is
not a pass.
