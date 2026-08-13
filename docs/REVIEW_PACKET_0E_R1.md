# Phase 0E-R1 Review Packet

## Disposition and scope

This packet requests external review of the authorized Phase 0E-R1 corrections. It does not claim Gate approval and does not authorize or begin Phase 1.

Baseline: `visual_authoring_engine_phase0e_review_2026-08-10.zip`, SHA-256 `2e1481e7b6da938872c3e75f2726fdbaeb3d28a47b8e72dc28c97a3fcac9b60c`.

## Structural correction

- Group/Ungroup validates fully and commits bounded reversible Document patches; no full `Document::clone()` is used.
- One structural change synchronizes Scene and RenderModel without fallback or full rebuild.
- Noncontiguous original sibling positions are stored and restored exactly across undo/redo and save/load.
- Projection schema version 2 carries explicit structural operations separately from request protocol version 1.
- Structural deltas do not serialize the parent's full child array.
- React applies structural operations incrementally and mounts at most 30 Layers rows in 10K/100K proof fixtures.
- 10K and 100K both report Group Scene visits 8 and Ungroup Scene visits 6. Group/Ungroup GPU dirty slots and upload bytes are zero when world/render meaning is unchanged.

## Interaction and UI correction

- Interaction generations serialize queued/in-flight work, Escape/pointercancel rollback, and Worker restart.
- Actual DOM pointercancel proof records an in-flight request plus a scheduled request, generation increment, cleared queue, inactive transaction, unchanged history, restored transform, and converged response/GPU/overlay sequences.
- Inspector proof enters `oldX + 17` in both 10K and 100K fixtures and requires exact revision/history/UI-delta/dirty-instance changes.
- The component showcase implements the documented Button, Textfield, Select, Tabs, Segmented Control, Checkbox, Switch, Menu/Tooltip, and Badge states.
- Dialog label/focus/Escape behavior, radio semantics/roving focus, and typed non-finite numeric validation are exercised by DOM automation.

## Verification

The following commands were actually executed on Windows and passed. Raw output is in `docs/verification/PHASE_0E_R1_VERIFICATION.txt`.

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
npm run test:browser
```

The same orchestrator also ran fresh Phase 0C, Phase 0D, and Phase 0D-R1 native regressions, plus a temporary clean-install actual-hardware Phase 0D-R1 browser regression.

- Rust all-target tests: 163 passed
- Rust doctests: 1 passed
- Web/unit/browser tests including prior-stage browser regression: 10 passed
- Total: 174 passed, 0 failed, 0 ignored, 0 skipped, 0 todo, 0 cancelled
- Actual browser: Chrome 151.0.7922.76 with a new temporary profile
- Actual GPU: NVIDIA GeForce GTX 970
- Browser proof: `all_passed=true`, run-wide maximum fallback 0, console errors 0, GPU validation errors 0
- Pixel proof: actual WebGPU texture readback, all assertions true

## Evidence index

- `docs/PHASE_0E_EXTERNAL_REVIEW.md`
- `docs/PHASE_0E_R1_AUTHORIZATION.md`
- `docs/PHASE_0E_R1_METRICS.json`
- `docs/PHASE_0E_R1_CHANGE_MANIFEST.json`
- `docs/PHASE_0E_R1_MANUAL_CHECKLIST.md`
- `docs/verification/PHASE_0E_R1_VERIFICATION.txt`
- `docs/verification/PHASE_0E_R1_BROWSER_PROOF.json`
- `docs/verification/PHASE_0E_R1_PIXEL_READBACK.json`
- `docs/verification/PHASE_0E_R1_GROUP_METRICS.json`
- `docs/verification/PHASE_0E_R1_GROUP_TEST_OUTPUT.txt`
- `docs/verification/PHASE_0E_R1_POINTERCANCEL_PROOF.json`
- `docs/verification/PHASE_0E_R1_UI_AUDIT.md`
- Five `phase0e-r1-*.png` screenshots

## Packaging and preservation policy

The R1 package root is `WebEditor/` and contains a full internal `CHECKSUMS.sha256`. Packaging validation compares all baseline docs/evidence and all 19 authoritative source files byte-for-byte, validates authoritative `SHA256SUMS.txt` 18/18, checks deleted baseline files, and rejects forbidden paths, path traversal, absolute paths, case duplicates, nested ZIPs, and CRC errors.

`target`, `node_modules`, `.tools`, `.git`, IDE/cache output, `dist`, temporary logs, work-only codemods, and obsolete screenshots are excluded. Final ZIP size, entry count, and SHA-256 are reported after archive creation and independently checked against the sidecar.

