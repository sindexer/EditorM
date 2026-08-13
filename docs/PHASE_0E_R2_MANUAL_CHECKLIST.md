# Phase 0E-R2 Manual Checklist

## Automated evidence already exercised

- [x] Verified the R1 baseline ZIP SHA-256 before R2 work.
- [x] Exercised 10K and 100K first/middle/last k=3 Group, Undo, Redo, Ungroup, save/load/Ungroup, and sibling insert/delete/reorder/Ungroup workflows.
- [x] Used 10 warm-up and 30 measured samples at Native Release, direct EngineHost, React ProjectionStore, and actual Chrome layers; Native Release structural samples contain eight actual executions per timing sample.
- [x] Confirmed 100K/10K structural work ratios at or below 1.35 and warm median time ratios at or below 3.0.
- [x] Confirmed Document/Scene/UI full sequence scans, full copies, dense rewrites, structural fallback/full rebuild, RenderModel clone/full scan, and unchanged-visual GPU dirty/upload counts are zero.
- [x] Verified eight overlapping siblings across Document order, Scene rank, topmost hit-test, persistence, undo/redo, edited-group restoration, and React Layers order.
- [x] Confirmed the legacy group-position metadata key is absent from product state and the versioned internal restoration record survives save/load.
- [x] Opened a self-contained preview with Chrome/151.0.7922.76, a new temporary profile, Dedicated Worker/WASM, and actual NVIDIA GeForce GTX 970 WebGPU.
- [x] Re-ran the R1 single-leaf, 48-byte instance update, generation, pointercancel, numeric validation, dialog/accessibility, pixel readback, and error-zero regressions.

## Optional visual review

- [ ] Inspect the five R2 screenshots at 100% scale for accidental clipping or layout regressions.
- [ ] Launch `tools/start-phase0e-editor.ps1` and repeat any desired interaction manually.

The optional visual review is not recorded as executed. Automated evidence is referenced from the R2 review packet.
