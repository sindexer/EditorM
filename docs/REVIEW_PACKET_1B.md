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
| `cargo test --workspace --all-targets` | PASS: 213 passed, 0 failed, 0 ignored across 28 test binaries |
| `cargo build --release --target wasm32-unknown-unknown` | PASS |
| `wasm-bindgen --target web` (0.2.126) into `web/editor/public/pkg` | PASS |
| `node scripts/direct-wasm-phase1b.mjs` | PASS: 46/46 checks against the shipped WASM module |
| Editor `npm test` | PASS: 5 files, 36 tests |
| Editor `npm run build` | PASS |

New focused coverage:

- `crates/runtime/tests/phase1b_arrange.rs`: alignment edges and centers, parent-local translation under a scaled parent, equal-gap distribution, one-undo-step behaviour, and typed rejection of duplicate, nested, locked, and undersized target sets.
- `crates/runtime/tests/phase1b_snap.rs`: near-edge correction, threshold rejection, simultaneous two-axis center snapping, self-exclusion of the dragged subtree, hidden-object exclusion, zero and invalid thresholds, closest-candidate determinism, and cross-viewport alignment guides.
- `crates/wasm_bridge/tests/phase1b_multi_selection.rs`: the request surface end to end, including drift-free repeated drag frames, snapped translation, alignment as one history entry, and typed failures that leave no open transaction.
- `web/editor/tests/phase1b-contract.test.ts`: pure selection geometry plus the editor/engine wiring contract.
- `web/editor/tests/phase1b-direct-wasm.test.ts`: stored direct-WASM proof integrity, including a SHA-256 match against the WASM package this repository ships.

## Known limits

- A multi-node drag issues one `SetLocalTransform` per selected node per committed frame, so its cost scales with selection size. The existing 1k/10k/100k benchmarks cover pan, zoom, selection, and structural work, not large-selection dragging; that measurement is still owed.
- Snapping compares the selection's union bounds, not each node individually, so a multi-node drag snaps by the outer rectangle.
- Distribution equalizes edge-to-edge gaps only. Center-spacing distribution and spacing measurements are not implemented.

## Evidence this packet does not have

This environment has no GPU, no display, and no Chrome harness, so **no browser, Worker, WebGPU, or pixel-readback evidence was produced for Phase 1B**, and none is claimed. `docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json` records `gpu_evidence: false`.

Gate 1B therefore still requires, on real hardware:

- `npm run test:browser:phase1a` to confirm Phase 1A behaviour did not regress;
- an equivalent Phase 1B browser proof covering rubber-band selection, multi-node drag, snap guide rendering, and the arrange toolbar, with fresh adapter, process, and profile records;
- pixel readback for the new overlay elements;
- the Windows `tools/*.ps1` verification path recorded in `PHASE_1A_VERIFICATION.txt`, since this record was produced with direct cargo and wasm-bindgen invocations on Linux instead.

The rebuilt `web/editor/public/pkg/engine_host_bg.wasm` in this branch was produced by the wasm32 release build recorded in the verification file: 1,402,869 bytes, SHA-256 `b7c77e5cb8dee4543f25588ee28b746e0d07c5be8d647c57826f8231a229c84b`. The same values appear in the direct-WASM proof, and `web/editor/tests/phase1b-direct-wasm.test.ts` re-hashes the shipped file to confirm the proof describes it.
