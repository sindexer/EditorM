# Phase 0E-R3 Manual Checklist

This checklist records what was actually verified for the R3 review package. It does not claim external Gate approval.

- [x] Direct R2 archive SHA-256 equals `2292fb079d4a5b6da4bedcd3f026277a6b2c6ae372193616112e1b2766ff2bb1`.
- [x] Prior Phase 0D/0E/R1/R2 documents and evidence changed: 0.
- [x] Authoritative package files byte-identical: 19/19.
- [x] Authoritative `SHA256SUMS.txt`: 18/18.
- [x] Document, Scene, and UI ranked sequences have worst-case logarithmic balancing independent of Node IDs.
- [x] Monotonic, reverse, edge, and collision-shaped sequence tests pass.
- [x] Restoration schema v2, version-1 migration, missing-anchor fallback, group reparent, and internal reorder policies pass.
- [x] Failed public requests leave Document, history, revision, selection, Scene, Render, GPU delta, and diagnostics unchanged.
- [x] Native runtime matrix: 70/70 entries, 55/55 thresholds.
- [x] Native EngineHost matrix: 70/70 entries, 55/55 thresholds.
- [x] Actual direct WASM matrix: 70/70 entries, 57/57 thresholds.
- [x] React ProjectionStore matrix: 70/70 entries, 55/55 thresholds.
- [x] Actual Chrome Worker/WASM/UI matrix: 70/70 entries, 55/55 thresholds.
- [x] Actual WASM module, instance, glue instance, protocol, schema, hash, and byte length recorded.
- [x] Actual Chrome uses a fresh profile, Dedicated Worker, actual WebGPU adapter/device, and NVIDIA GeForce GTX 970.
- [x] Browser heartbeat >= 1, actual value 639.
- [x] Pixel readback passes.
- [x] Console errors: 0.
- [x] GPU validation errors: 0.
- [x] Maximum fallback/rebuild count: 0.
- [x] Five current R3 screenshots captured.
- [x] Format, Clippy, Build, Rust all-target tests, React tests, production build, release matrices, direct WASM, and actual browser proof pass.
- [x] Pre-fix and invocation failure outputs are preserved and not presented as passes.
- [x] Package excludes target, node_modules, .tools, .git, IDE/cache, dist, TypeScript cache, codemods, credentials, temporary logs, and nested ZIPs.
- [x] Phase 1 was not started.
