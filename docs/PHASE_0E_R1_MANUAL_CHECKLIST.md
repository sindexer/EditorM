# Phase 0E-R1 Manual Checklist

## Automated evidence already exercised

- [x] Opened a self-contained preview with a new Chrome 151 process and temporary profile.
- [x] Created an actual NVIDIA GeForce GTX 970 WebGPU adapter/device in a Dedicated Worker/WASM runtime.
- [x] Exercised Group/Ungroup on first, middle, and last sibling targets in 10K and 100K fixtures.
- [x] Confirmed Document clone 0, fallback/full rebuild 0, projection full snapshot 0, bounded structural payload, React hierarchy full rebuild delta 0, and mounted Layers rows at most 30.
- [x] Entered `oldX + 17` through the 10K and 100K Inspector and confirmed revision/history +1, one UI node delta, one dirty instance, 48-byte instance payload, and 52-byte versioned dirty record.
- [x] Dispatched an actual DOM `pointercancel` while one update was in flight and another was scheduled; confirmed rollback settlement, no queued execution, unchanged history, and matching response/GPU/overlay sequences.
- [x] Exercised dialog label, initial focus, containment, Escape close, focus restoration, segmented radio semantics/roving focus, and invalid numeric typed error without Worker/history mutation.
- [x] Performed actual WebGPU texture readback for rectangle, ellipse, and ellipse AABB-corner background pixels.
- [x] Confirmed run-wide maximum fallback 0, console errors 0, GPU validation errors 0, and `all_passed=true`.

## Optional visual review

- [ ] Inspect the five R1 screenshots at 100% scale for accidental clipping or layout regressions.
- [ ] Launch `tools/start-phase0e-editor.ps1` and repeat any desired mouse/keyboard interaction manually.

The optional visual review is not recorded as executed. Automated checks above are backed by the R1 browser proof.

