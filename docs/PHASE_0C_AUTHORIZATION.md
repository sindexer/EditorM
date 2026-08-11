# PHASE 0C AUTHORIZATION

Status: Authorized by external review instruction  
Date: 2026-08-08 (Asia/Seoul)

## Authorization

The external Gate 0B review approved the Phase 0B command-only mutation boundary,
transactions, undo/redo and branch invalidation, local-effect history, and non-persistent
selection state. The approved baseline was reverified before Phase 0C work began:

- 65 unit/integration/property tests passed, with 0 failed and 0 ignored.
- 1 compile-fail doctest passed, with 0 failed and 0 ignored.
- The Phase 0B review archive reference is 46,100,878 bytes with SHA-256
  `4df5259941db731d6df1b091c7b866e66b420460d899cf4c3d6b5db54450eaff`.

This authorization permits only Phase 0C: Computed Scene, semantic mutation change
reporting, incremental dirty propagation, spatial indexing, geometry-aware hit testing,
camera coordinate conversion, instrumentation, and BENCH-A through BENCH-D proof fixtures.

## Explicitly not authorized

Phase 0D and later work is not authorized. This includes WASM, Dedicated Worker, WebGPU or
wgpu, renderer/render items, GPU resources, batching or renderer culling, React/browser UI,
Wanted Design System UI, direct-manipulation UI, snapping, vector/path/text engines, AI
command interpretation, and PSD or After Effects integration.

## Protected inputs

The authoritative `visual_authoring_engine_codex_package` and its Wanted Design System
reference originals remain immutable. Their supplied checksums must be reverified at Gate
0C packaging.

Implementation must stop at Gate 0C and await a new external approval before Phase 0D.
