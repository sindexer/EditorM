# Phase 0E-R3 Review Packet

## Disposition and scope

This packet requests external review of the authorized Phase 0E-R3 complexity and evidence-integrity corrections. It does not claim Gate approval and does not authorize or begin Phase 1.

Direct baseline: `visual_authoring_engine_phase0e_r2_review_2026-08-11.zip`, 48,176,877 bytes, SHA-256 `2292fb079d4a5b6da4bedcd3f026277a6b2c6ae372193616112e1b2766ff2bb1`.

## Corrections

- Group planning resolves selected ranks once, forms consecutive runs in one pass, and resolves one anchor pair per run.
- Ungroup planning uses restoration v2 run order and ranked lookups without expanding-prefix scans.
- Document and Scene use arena-backed implicit order-statistic AVL sequences; UI uses a ranked AVL sequence. Tree shape is independent of Node IDs.
- All bulk constructors and fragment paths are tracked. Allocation bytes, nodes, rotations/rebalances, maximum depth, comparisons, copies, moves, scans, dense rewrites, and fallbacks are exposed through EngineHost and UI counters.
- Failed public requests preserve Document semantics, history, revision, selection, Scene, Render, GPU delta, and diagnostics.
- Actual direct WASM evidence compiles and instantiates `engine_host_bg.wasm`, creates the glue EngineHost, and invokes `handleJson`; it is not native EngineHost evidence.
- The React batch path uses a pruned ranked extraction, balanced fragment adoption, direct balanced reattachment, and a verified closed-form rank bound for proven interpolated selections.

## Full performance matrix

Each layer ran N={10K,100K}, k={3,30,100,300,1K,3K,10K}, five selection patterns, Group/Ungroup/Undo/Redo, 10 warm-ups, and 30 measured iterations. Fixture creation/load is outside structural timings. Raw samples, median, p95, counters, and exact sibling restoration are in the evidence.

| Layer | Matrix | Max work/k | Max x10 work | Max 100K/10K work | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| Native release runtime | 70 | 1.0206 | 10.5037 | 1.2343 | PASS |
| Native EngineHost | 70 | 1.0206 | 10.5037 | 1.2343 | PASS |
| Actual direct WASM | 70 | 1.0240 | 10.5892 | 1.2769 | PASS |
| React ProjectionStore | 70 | 0.2836 | 9.8293 | 1.3470 | PASS |
| Actual Chrome Worker/WASM/UI | 70 | 0.5259 | 10.3598 | 1.2986 | PASS |

Limits are 2.0, 15.0, and 1.35 respectively. Actual WASM k=1,000 Group median is 12.7018 ms (limit 25 ms); k=3,000/k=300 median ratio is 9.43584 (limit 15).

All layers report structural full scans/copies, dense rewrites, and fallback/full rebuilds as zero. Maximum depths remain within the implementation's documented logarithmic bound.

## Correctness and atomicity

- Noncontiguous multi-run Group and exact immediate Ungroup.
- Sibling insert/delete/reorder while grouped.
- Group reparent and missing-anchor fallback.
- Internal child reorder with documented run policy.
- Undo/Redo and save/load/Ungroup.
- Version-1 restoration migration to v2.
- Exact semantic snapshot round trips.
- Scene rank, hit-test, culling, Layers order, Worker/GPU/overlay sequence, restart generation, pointercancel, numeric failure, accessibility, and pixel readback R2 protections remain covered.
- Targeted atomicity: 1 passed.
- Targeted adversarial balance: 1 passed.
- Targeted restoration: 2 passed.

## Actual browser and artifacts

- Browser: Chrome/151.0.7922.76.
- GPU: NVIDIA GeForce GTX 970, driver 32.0.15.8157.
- Dedicated Worker heartbeat: 639.
- Browser checks: 38/38.
- Console errors: 0.
- GPU validation errors: 0.
- Maximum fallback: 0.
- Actual WebGPU pixel readback: PASS.
- Current R3 screenshots: 5.
- WASM: 1,229,366 bytes, SHA-256 `bbad837fbe7be3d21733d2318a596adec7798c45ef6fb300b54665672cd99367`.
- JS glue SHA-256: `eba3cb7e72838e9506bd8f9fed8a19492211944f2b2e9492f16e6b5321531479`.

## Verification

Final required Rust commands all passed:

- Format check: PASS.
- Clippy workspace/all-targets with `-D warnings`: PASS.
- Build workspace/all-targets: PASS.
- Test workspace/all-targets: 178 passed, 0 failed, 0 ignored.

Additional current results:

- R2 release boundary recheck: 1 passed.
- Native R3 release matrix: 1 passed.
- Native EngineHost R3 release matrix: 1 passed.
- React unit tests: 7 passed.
- React R3 matrix: 1 passed.
- Browser R3 assertion: 1 passed.
- Targeted atomicity/adversarial/restoration: 4 passed total.

The initial format, React, native-threshold/order, npm.ps1, and R2 cold-cache benchmark failures are preserved. None is represented as a pass.

## Evidence index

- `docs/PHASE_0E_R3_AUTHORIZATION.md`
- `docs/PHASE_0E_R3_CHANGE_MANIFEST.json`
- `docs/PHASE_0E_R3_METRICS.json`
- `docs/PHASE_0E_R3_MANUAL_CHECKLIST.md`
- `docs/adr/ADR-039-group-restoration-runs-v2.md`
- `docs/adr/ADR-040-worst-case-balanced-ranked-sequences.md`
- `docs/adr/ADR-041-exact-structural-work-counter-contract.md`
- `docs/verification/PHASE_0E_R3_NATIVE_STRUCTURAL_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_NATIVE_ENGINE_HOST_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_ACTUAL_DIRECT_WASM_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_REACT_MATRIX_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_BROWSER_PROOF.json`
- `docs/verification/PHASE_0E_R3_PIXEL_READBACK.json`
- `docs/verification/PHASE_0E_R3_FAILURE_ATOMICITY_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_ADVERSARIAL_SEQUENCE_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_RESTORATION_OUTPUT.txt`
- `docs/verification/PHASE_0E_R3_STATIC_AUDIT.txt`
- `docs/verification/PHASE_0E_R3_VERIFICATION.txt`
- Five `docs/verification/phase0e-r3-*.png` screenshots.

## Preservation and stop boundary

Prior Phase 0D/0E/R1/R2 docs and evidence changed: 0. Authoritative files are 19/19 byte-identical and their manifest is 18/18. Deleted baseline files: 0.

The review ZIP excludes target, node_modules, .tools, .git, IDE/cache, dist, TypeScript cache, work-only codemods, credentials, temporary files, and nested ZIPs. Phase 1 remains unstarted. Work stops after the R3 ZIP and sidecar are verified.
