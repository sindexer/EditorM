# Quality and Efficiency Policy

## Non-negotiable correctness

Immediately address data loss, state-machine violations, structural performance regressions, renderer-integrity failures, secret exposure, and tests that falsely report success. Public operations must be typed, recoverable, and failure-atomic.

## Verification tiers

### Fast pull-request checks

Run on every PR:

- Rust formatting, Clippy with warnings denied, and workspace/all-target tests;
- pinned WASM rebuild plus checked-in binding, ABI, protocol, and initialization consistency;
- actual WASM module initialization smoke test;
- `npm ci`, React/Web unit tests, and production builds;
- approved-baseline integrity, Git LFS, large-file, forbidden-artifact, and tracked-secret checks.

A check not executed must be reported as not executed. Stored JSON is never treated as a new execution.

### Targeted checks

Run when the affected subsystem changes: focused structural matrices, serialization round trips, Worker sequencing and restart tests, DOM input proof, renderer/WGSL validation, accessibility, and visual regression checks.

### Full gate or release checks

Run before a Phase Gate or release candidate and when explicitly requested: 10k/100k structural matrices, actual WASM, actual browser Worker/WASM integration, actual WebGPU hardware proof, pixel readback, DOM input proof, and long-running sequencing/restart/stale-frame checks.

GitHub-hosted runners cannot substitute for real hardware WebGPU evidence. Renderer, WGSL, render-binary schema, or GPU-application changes require fresh local hardware proof attached to the PR and repeated before the gate.

## Cost control

Prefer focused checks during iteration and avoid regenerating unchanged evidence. Full matrices, browser hardware runs, and checksummed review packages are reserved for subsystem-impacting changes, Phase Gates, release candidates, or explicit reviewer requests. Do not weaken thresholds, skip assertions, reuse stale output, or introduce mocks merely to reduce runtime.

## Evidence integrity

Evidence must name the command, start and finish times, environment, exit code, pass/fail/ignored counts, and raw samples when performance is measured. Preserve prior-phase evidence byte-for-byte. Record failed attempts when they explain a correction; do not overwrite them with successful output under the same historical path.
