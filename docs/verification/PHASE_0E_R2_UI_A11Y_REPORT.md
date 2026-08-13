# Phase 0E-R2 UI and Accessibility Regression Report

- Captured: 2026-08-11T03:24:55.669Z
- Browser: Chrome/151.0.7922.76, new temporary profile
- GPU: NVIDIA GeForce GTX 970, driver 32.0.15.8157
- Runtime: actual WebGPU in Dedicated Worker/WASM; no mock and no Canvas2D fallback

## Automated DOM and accessibility results

- Component showcase label: true; initial focus: `Close showcase`.
- Tabs: 3; segmented radios: 3; roving radio keyboard behavior: true.
- Select, menu trigger, switch, checkbox, badge: all present and semantically exercised.
- Dialog focus containment and Escape close/focus restoration: passed.
- Invalid numeric input returned `numeric_non_finite`, set `aria-invalid=true`, announced `numeric_non_finite: Enter a finite number.`, and did not mutate Worker revisions or history.
- Serialized pointercancel rollback, stale-intent rejection, and Worker restart generation checks: passed.
- 100K Layers projection nodes: 100001; mounted rows: 25.
- Actual WebGPU pixel readback: actual-webgpu-texture-readback; all 4 assertions passed.
- Console errors: 0; GPU validation errors: 0; run-wide fallback maximum: 0.

All 37 browser proof checks are true. This report describes automated evidence; optional human screenshot inspection was not recorded as executed.
