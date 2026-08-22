# Phase 1B Review Packet

## Disposition

This packet requests external review of Phase 1B: multiple selection, alignment, distribution, and snapping. It does not claim Gate 1A or Gate 1B approval, and it does not start Phase 2. No review ZIP was created.

Baseline: `main` merge commit `8f5a551` (Phase 1A); approved Phase 0E-R3 payload `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`; approved tag `phase-0e-r3-approved`.

## Delivered scope

- Selection is engine state with atomic validation. `selection { mode: "set" | "extend", targets }` validates every ID before changing anything, so a rejected request leaves the previous selection intact.
- `marquee_select` converts a viewport rectangle to world space, queries the spatial index, and selects the *top-level* nodes of the active root that it crosses. Hidden nodes, nodes inside locked containers, and the root itself are excluded. `additive` keeps the existing selection.
- Dragging several nodes sends one `translate_selection` request per frame. The EngineHost captures each selected node's local transform, its parent's world transform, and the selection's union bounds when the transaction opens, so every frame recomputes absolute positions from the same anchor and coalesced or replayed pointer frames cannot drift.
- Snapping is computed in the runtime. Two thin band queries, clipped to the visible world viewport, produce candidates; edges and centers are compared on each axis; the smallest correction wins with a deterministic tie-break. Guides are returned as engine data and drawn by the editor without re-deriving geometry. The threshold is expressed in viewport pixels and divided by zoom, so the snap radius is constant on screen. Alt suspends snapping for a drag frame; the toolbar toggle disables it entirely.
- Alignment and distribution are planned in the runtime from world bounds and applied as one transaction: one alignment is one undo step. Planning mutates nothing and reports how many targets it examined and how many needed no movement.
- Every new request fails atomically with a typed error: unknown IDs, a locked target, a nested target pair, fewer than two (alignment) or three (distribution) targets, a missing transaction, a non-finite delta or threshold, and unknown operation names. Failures leave no open transaction and no partial movement.
- The editor draws each selected node's outline, one dashed union rectangle for multiple selection, the rubber band, and the engine's snap guides. `Ctrl/Cmd+A` selects the top-level nodes of the active root.
- New request types are additive. The request protocol stays at version 1 and the render binary schema stays at version 2.

## Explicit exclusions

Multi-selection resize and rotate (transform handles remain single-selection), pixel-grid snapping, spacing measurements, user guides and rulers, Pen/Bezier, text, gradients, image fill, shadows, auto layout, components, motion, AI integration, multiple projects, export, and transform-correct Frame clipping.

## Decisions

- [ADR-045](adr/ADR-045-multiple-selection-and-batched-arrange.md): multiple selection, runtime-planned arrange, and batched atomic transactions.
- [ADR-046](adr/ADR-046-bounded-band-snapping.md): bounded band snapping and engine-owned alignment guides.
- [ADR-047](adr/ADR-047-transaction-anchored-drag-base.md): transaction-anchored drag base for multi-node translation.

ADR-001 through ADR-044 and all existing Phase 0 and Phase 1A evidence remain unchanged.

## Verification executed for this packet

Every command below was executed in this task's Linux container against the final source state. The complete record, with timestamps and exit statuses, is [PHASE_1B_VERIFICATION.txt](verification/PHASE_1B_VERIFICATION.txt).

| Validation | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo build --workspace --all-targets` | PASS |
| `cargo test --workspace --all-targets` | PASS: 214 passed, 0 failed, 0 ignored across 28 test binaries |
| `cargo build --release --target wasm32-unknown-unknown` | PASS |
| `wasm-bindgen --target web` (0.2.126) into `web/editor/public/pkg` | PASS |
| `node scripts/direct-wasm-phase1b.mjs` | PASS: 46/46 checks against the shipped WASM module |
| Editor `npm test` | PASS: 6 files, 42 tests |
| Editor `npm run build` | PASS |

New focused coverage:

- `crates/runtime/tests/phase1b_arrange.rs`: alignment edges and centers, parent-local translation under a scaled parent, equal-gap distribution, one-undo-step behaviour, and typed rejection of duplicate, nested, locked, and undersized target sets.
- `crates/runtime/tests/phase1b_snap.rs`: near-edge correction, threshold rejection, simultaneous two-axis center snapping, self-exclusion of the dragged subtree, hidden-object exclusion, zero and invalid thresholds, closest-candidate determinism, and cross-viewport alignment guides.
- `crates/wasm_bridge/tests/phase1b_multi_selection.rs`: the request surface end to end, including drift-free repeated drag frames, snapped translation, alignment as one history entry, and typed failures that leave no open transaction.
- `web/editor/tests/phase1b-contract.test.ts`: pure selection geometry plus the editor/engine wiring contract.
- `web/editor/tests/phase1b-direct-wasm.test.ts`: stored direct-WASM proof integrity, including a SHA-256 match against the WASM package this repository ships, plus the engine-only multi-drag benchmark record.
- `web/editor/scripts/browser-proof-phase1b.mjs`: the Gate 1B browser harness (real Chrome, Worker, WASM, WebGPU, composited pixel sampling, multi-drag benchmark), run by `npm run test:browser:phase1b`.
- `web/editor/tests/browser-proof-phase1b.test.ts`: validates a produced browser proof; runs from the browser command, not the default suite.
- `web/editor/tests/phase1b-gate-status.test.ts`: the honesty guard in the default suite — no GPU-dependent item may claim PASS while the hardware proof artifact is missing.

## Known limits

- A multi-node drag issues one `SetLocalTransform` per selected node per committed frame, so its cost scales with selection size. The measured 1,000-object engine result is about 239 ms per frame and remains performance debt, not a new Gate threshold.
- Snapping compares the selection's union bounds, not each node individually, so a multi-node drag snaps by the outer rectangle.
- Distribution equalizes edge-to-edge gaps only. Center-spacing distribution and spacing measurements are not implemented.
- Current primitive creation places shapes as siblings of the default Frame. The Phase 1B marquee
  `exclude_ids` behavior prevents selecting its own backdrop, but Frame containment semantics
  remain future work.

## Gate 1B status

`docs/verification/PHASE_1B_GATE_STATUS.json` is the machine-generated and machine-checked record.
`tools/run-phase1b-gate.ps1` supplies one run ID, tested commit, and branch to every proof, and
`web/editor/scripts/finalize-gate-phase1b.mjs` derives all item states, summary counts, and the Gate
conclusion. The record must not be hand-edited. `web/editor/tests/phase1b-gate-status.test.ts`
checks summary/conclusion consistency, browser proof integrity, and evidence binding in the default
suite.

The branch-preparation snapshot has two static tooling items at `PASS`; every run-bound item is
`UNVERIFIED` until the Windows runner creates one consistently bound evidence set. The generated
JSON is authoritative; this packet intentionally does not duplicate a table that could drift from
the finalizer's output.

Gate conclusion: **NOT PASSED**. Every UNVERIFIED hardware item above is implemented and asserted
by the harness; none has been executed against a GPU in the branch-preparation environment.
`docs/PHASE_1B_HARDWARE_RUN.md` now provides one repository-root Windows command and automatic
finalization rather than a hand-edited PASS/FAIL record.

## Measured multi-selection drag cost

`npm run bench:multi-drag:phase1b` measures one drag frame against the shipped WASM engine on the
1,000-object fixture (Node, no Worker hop, no GPU submission; 5 warm-up and 20 measured
iterations of 5 frames each):

| Objects dragged | Snapping | Median | p95 | Dirty slots |
| --- | --- | --- | --- | --- |
| 10 | off | 1.97 ms | 2.40 ms | 10 |
| 10 | on | 2.59 ms | 2.87 ms | 10 |
| 100 | off | 6.21 ms | 6.85 ms | 100 |
| 100 | on | 8.47 ms | 9.95 ms | 100 |
| 1,000 | off | 228.85 ms | 238.85 ms | 1,000 |
| 1,000 | on | 236.32 ms | 249.62 ms | 1,000 |

Dirty slots equal the selection size at every size, with zero full rebuilds, zero document clones,
and zero full render-model scans, so the cost is exactly the per-node command work and nothing
hidden. Ten and one hundred objects stay inside the 16.7 ms frame budget. **One thousand
simultaneously dragged objects do not**: roughly 239 ms per frame, about 14 times the
budget. Phase 1B never claimed a large-selection drag budget, so this is recorded as a measured
limit rather than a regression, and it is the number to beat if interactive large-selection
dragging is ever required.

## Findings from building the browser harness

- A rubber band started on a Frame's background also selected that Frame, because the rectangle
  tool creates shapes as siblings of a Frame rather than children of it. The band now excludes the
  container it was drawn on (`exclude_ids`), covered by a new EngineHost test and recorded in
  ADR-045. This is the only Phase 1B behaviour change in this pass.
- No other behaviour changed: this pass adds verification, not scope.

## Evidence this packet does not have

This environment has no GPU, no display, and no Chrome harness, so **no browser, Worker, WebGPU, or pixel-readback evidence was produced for Phase 1B**, and none is claimed. `docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json` records `gpu_evidence: false`.

The Phase 1B harness now exists and was executed twice in this container. Both runs failed closed
at the WebGPU device and wrote their failure artifacts:

- `docs/verification/PHASE_1B_BROWSER_FAILURE.json` - default (hardware-required) run.
- `docs/verification/PHASE_1B_BROWSER_SOFTWARE_RUN_FAILURE.json` - diagnostic run with
  `--enable-unsafe-swiftshader`, which also failed; software WebGPU is unavailable here too.
- `docs/verification/PHASE_1B_GATE_PHASE1A_RERUN.json` - the Phase 1A regression re-run attempt.

In this container `navigator.gpu` is exposed over the secure localhost origin, but the GPU process
cannot hold a WebGPU instance (`OperationError: Instance dropped in popErrorScope`), so no adapter
or device is usable and the Editor never reaches readiness. The preview server, asset MIME checks,
Worker boot, and WASM delivery all succeeded first.

Gate 1B therefore still requires, on real hardware, the commands in
[PHASE_1B_HARDWARE_RUN.md](PHASE_1B_HARDWARE_RUN.md):

- `npm run test:browser:phase1a` to confirm Phase 1A behaviour did not regress;
- `npm run test:browser:phase1b` for the Phase 1B behaviour, pixel, and benchmark evidence;
- the Windows `tools/*.ps1` verification path recorded in `PHASE_1A_VERIFICATION.txt`, since this record was produced with direct cargo and wasm-bindgen invocations on Linux instead.

The rebuilt `web/editor/public/pkg/engine_host_bg.wasm` in this branch was produced by the wasm32 release build recorded in the verification file: 1,404,367 bytes, SHA-256 `e32e22a9ad7865484fc3b0c2bc095f6ecd5238f4bc470305b52f1247a785c178`. The same values appear in the direct-WASM proof, and `web/editor/tests/phase1b-direct-wasm.test.ts` re-hashes the shipped file to confirm the proof describes it.
