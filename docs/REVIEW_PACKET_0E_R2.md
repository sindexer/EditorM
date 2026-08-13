# Phase 0E-R2 Review Packet

## Disposition and scope

This packet requests external review of the authorized Phase 0E-R2 structural corrections. It does not claim Gate approval and does not authorize or begin Phase 1.

Direct baseline: `visual_authoring_engine_phase0e_r1_review_2026-08-10.zip`, SHA-256 `07d6ba27f27c32b89474b098e0e1f3045bbd2d4511eb7951d1e7d03d3bafe3db`.

## Structural correction

- Document and Scene sibling order use arena-backed implicit order-statistic treaps with ID locators and exact work counters.
- React ProjectionStore uses ranked treaps, detached fragments, subtree sizes, and viewport rank/select instead of large-array structural edits.
- Group restoration is a versioned internal before/after-anchor record; user metadata no longer stores engine positions.
- Group/Ungroup, undo/redo, save/load/Ungroup, and sibling insert/delete/reorder/Ungroup are covered at 10K and 100K.
- Eight-overlapping-sibling evidence records Document semantic orders, live Scene ranks, topmost hit order, persistence, and React Layers order.
- Full edit-path sibling scans/copies, dense rewrites, hierarchy fallback/full rebuilds, RenderModel clones/scans, and unchanged-visual GPU uploads remain zero.

## 10K/100K results

| Layer | Operation | Work ratio | Median time ratio | Payload ratio |
| --- | --- | ---: | ---: | ---: |
| Native Release | group | 1.228 | 1.375 | n/a |
| Native Release | loaded_ungroup | 1.258 | 1.365 | n/a |
| Native Release | redo | 1.214 | 1.309 | n/a |
| Native Release | sibling_insert_delete_reorder_then_ungroup | 1.295 | 1.235 | n/a |
| Native Release | undo | 1.236 | 1.374 | n/a |
| Native Release | ungroup | 1.241 | 1.319 | n/a |
| Direct EngineHost | group | 1.228 | 0.489 | n/a |
| Direct EngineHost | redo | 1.214 | 0.458 | n/a |
| Direct EngineHost | sibling_insert_delete_reorder_then_ungroup | 1.295 | 0.507 | n/a |
| Direct EngineHost | undo | 1.236 | 0.434 | n/a |
| Direct EngineHost | ungroup | 1.241 | 0.465 | n/a |
| React ProjectionStore | group | 1.019 | 0.941 | n/a |
| React ProjectionStore | ungroup | 1.094 | 0.937 | n/a |
| Actual Chrome | group | 1.174 | 0.821 | 1.020 |
| Actual Chrome | undo | 1.208 | 0.846 | 1.025 |
| Actual Chrome | redo | 1.161 | 0.828 | 1.020 |
| Actual Chrome | ungroup | 1.215 | 0.759 | 1.025 |
| Actual Chrome | edited_ungroup | 1.206 | 0.885 | 1.025 |

All work ratios are at most 1.35 and all warm median time ratios are at most 3.0. Native Release uses 10 warm-up and 30 measured timing samples with eight actual structural executions averaged into each timing sample; direct EngineHost, React, and Chrome use at least the required 10/30 actual iterations. Raw samples, median, p95, work, allocation, IDs, and counters are preserved in the R2 evidence files.

## Verification

The R2 final orchestrator actually executed and passed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0e-wasm.ps1
npm ci --ignore-scripts
npm run build
npm test
npm run test:browser:r2
```

- Rust all-target tests: 171 passed
- Rust doctests: 1 passed
- React tests: 5 passed
- R2 browser assertion tests: 1 passed
- Counted suite total: 178 passed, 0 failed, 0 ignored, 0 skipped, 0 mocked
- Actual browser/GPU: Chrome/151.0.7922.76, NVIDIA GeForce GTX 970
- Browser proof: `all_passed=true`, 37/37 checks, fallback maximum 0, console errors 0, GPU validation errors 0

## Evidence index

- `docs/PHASE_0E_R2_AUTHORIZATION.md`
- `docs/PHASE_0E_R2_METRICS.json`
- `docs/PHASE_0E_R2_CHANGE_MANIFEST.json`
- `docs/PHASE_0E_R2_MANUAL_CHECKLIST.md`
- `docs/adr/ADR-035-document-ranked-sibling-sequence.md`
- `docs/adr/ADR-036-scene-ranked-order-and-z-model.md`
- `docs/adr/ADR-037-ui-ranked-projection-sequence.md`
- `docs/adr/ADR-038-versioned-group-restoration-anchors.md`
- `docs/verification/PHASE_0E_R2_NATIVE_STRUCTURAL_OUTPUT.txt`
- `docs/verification/PHASE_0E_R2_DIRECT_WASM_STRUCTURAL_OUTPUT.txt`
- `docs/verification/PHASE_0E_R2_REACT_PROJECTION_OUTPUT.txt`
- `docs/verification/PHASE_0E_R2_ORDER_ACCURACY_OUTPUT.txt`
- `docs/verification/PHASE_0E_R2_STATIC_AUDIT.txt`
- `docs/verification/PHASE_0E_R2_BROWSER_PROOF.json`
- `docs/verification/PHASE_0E_R2_PIXEL_READBACK.json`
- `docs/verification/PHASE_0E_R2_UI_A11Y_REPORT.md`
- `docs/verification/PHASE_0E_R2_VERIFICATION.txt`
- Five `docs/verification/phase0e-r2-*.png` screenshots

## Preservation and packaging

The R2 packager requires prior docs/evidence byte changes 0, authoritative files 19/19 byte-identical, authoritative `SHA256SUMS.txt` 18/18, deleted baseline files 0, and forbidden/path/duplicate/CRC errors 0. It records source-project and physical-review-ZIP comparisons separately and lists excluded generated artifacts instead of treating them as deletions.

`target`, `node_modules`, `.tools`, `.git`, IDE/cache output, `dist`, work-only codemods, credentials, temporary logs, and nested ZIPs are excluded. Phase 1 remains unstarted.
