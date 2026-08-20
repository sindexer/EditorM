# Phase 1B Review Packet

## Disposition

This packet requests external review of Phase 1B: multi-slide workspace foundation and UI structure transition. Implementation and current evidence are complete, but this packet does not claim Gate 1B approval. The PR remains unmerged, Phase 1C has not started, and no review ZIP was created.

Baseline: main merge commit `8f5a5515eafa0eed8f1f9199f6b73dd51d351f0b`; approved Phase 0E-R3 payload `39085167a1b9d2ce1ba78060b3fee4d9327aaf27`; approved tag `phase-0e-r3-approved`.

## Delivered scope

- A Document can contain ordered root-level Frame Slides. The default Slide is 1920 x 1080 (16:9).
- Worker-owned `EditorSession` stores the active Slide and per-Slide Selection and Camera without serializing view state into the Document.
- Typed Slide create, duplicate, rename, reorder, delete, activate, save/load, undo, and redo operations cross the Worker-owned Rust/WASM boundary.
- Duplicate Slide performs a deep persistent subtree copy with independent stable IDs. The last Slide cannot be deleted.
- Main render, culling, hit-test, selection, object creation, and Layers projection are isolated to the active Slide.
- Slide activation is view-only: it does not change Document, Scene, or Render revision and does not enter History.
- Actual Slide thumbnails use Rust RenderModel subtree slots and WebGPU OffscreenCanvas. Work is bounded to one thumbnail render per animation frame and prioritized active, visible, then remaining.
- Thumbnail revision invalidation is per Slide. Queued and running revision changes are reconciled without orphaning work.
- The professional shell now has top menus, a horizontal tool bar, Slides at left, active Slide canvas at center, synchronized Layers and structure-only Timeline below, and contextual Inspector at right.
- Three accessible splitters resize the panels. Panel dimensions and collapse state are local workspace preferences, not Document state.
- Timeline and Layers mount the same NodeIds and share scroll position. Timeline is explicitly structure-only in Phase 1B.
- A 30-Slide x 100-primitive structural fixture proves bounded active switching, isolated single-node invalidation, subtree thumbnail culling, and bounded mounted UI rows.

## Explicit exclusions

Phase 1B does not add playhead, keyframes, easing, playback, FPS controls, motion evaluation, presentation playback, snapping, text, Pen/Bezier, gradients, image fill, export, collaboration, AI integration, or multiple projects. Phase 1C and later work have not started.

## Decision

- [ADR-045](adr/ADR-045-multislide-workspace-and-active-slide-session.md): ordered root-level Slides, Worker-owned active Slide session, view-only activation, thumbnail derivation, and structure-only Timeline boundary.

ADR-001 through ADR-044 and all historical Phase 0 evidence remain unchanged.

## Required verification

All commands below were run on Windows against the final source state:

| Validation | Result |
| --- | --- |
| `tools/cargo.ps1 fmt --all -- --check` | PASS |
| `tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings` | PASS |
| `tools/cargo.ps1 build --workspace --all-targets` | PASS |
| `tools/cargo.ps1 test --workspace --all-targets` | PASS: 195 passed, 0 failed, 0 ignored |
| checked-in and fresh pinned WASM ABI initialization | PASS: protocol v1, render schema v2 |
| Preview `npm test` / `npm run build` | PASS: 7 tests; 9 verified build assets |
| Editor `npm test` / `npm run build` | PASS: 25 tests; TypeScript and production build |
| existing R3 React complexity matrix | PASS: 1 matrix test |
| `npm run test:browser:phase1b` | PASS: fresh actual hardware run plus 5 stored-proof tests |
| scope classifier / hardware evidence validator | PASS: 9/9 cases; 1 Phase 1B proof |

The exact command and incident record is [PHASE_1B_VERIFICATION.txt](verification/PHASE_1B_VERIFICATION.txt).

## Actual Chrome, Worker, WASM, and WebGPU proof

Fresh capture: `2026-08-20T09:43:40.099Z`.

- Chrome `151.0.7922.138`, new process PID 21748, and a new temporary profile.
- NVIDIA GeForce GTX 970 actual WebGPU device, driver `32.0.15.8157`.
- Dedicated Worker owns an initialized WASM EngineHost; heartbeat advances from 1 to 3.
- Surface base `bgra8unorm`; pipeline and readback views `bgra8unorm-srgb`.
- Browser assertions: 25/25; console errors: 0; GPU validation errors: 0; maximum fallback rebuild count: 0.
- Final thumbnails: 8 captures, 7 invalidations, 3 ready, 0 pending, 0 errors, queue depth 0, Canvas2D fallback count 0.
- Add/activate, per-Slide Selection and Camera restore, cross-Slide hit/selection isolation, deep duplicate IDs, rename/reorder persistence, delete/undo/redo, isolated thumbnail invalidation, splitters, Layer/Timeline NodeId parity, contextual Inspector, and Worker/GPU/overlay sequence agreement all pass.

Screenshots:

- [Multi-slide workspace](verification/phase1b-multislide-workspace.png)
- [Layers and structure-only Timeline](verification/phase1b-layers-timeline.png)

## Structural complexity evidence

[PHASE_1B_METRICS.json](PHASE_1B_METRICS.json) records a 30-Slide x 100-primitive fixture with 3,031 Document nodes.

- Active Slide switch: Document/Scene/Render revision delta 0; Document full clones 0; Render full rebuilds 0.
- One Slide thumbnail cull: 101 RenderModel items read, 101 exact visible, full RenderModel scans 0.
- One node edit: exactly 1 Slide thumbnail revision changed; Render full rebuilds 0; full RenderModel scans 0; render items cloned 0.
- UI bounds: at most 25 mounted Layer/Timeline rows and at most 1 thumbnail render per animation frame.
- A legacy no-Slide 10k/100k EngineHost performance test initially exposed an accidental root scan in every response. The final cached Slide index removes that regression; the targeted test passes in 161.16 seconds with 100k/10k median-time ratios 0.46 to 0.53 and work ratios 1.13 to 1.28.

## Preserved failures and corrections

- [PHASE_1B_BROWSER_FAILURE_PRE_FIX_QUEUE_RACE.json](verification/PHASE_1B_BROWSER_FAILURE_PRE_FIX_QUEUE_RACE.json) preserves the first thumbnail revision race failure.
- [PHASE_1B_BROWSER_FAILURE.json](verification/PHASE_1B_BROWSER_FAILURE.json) and its byte-identical named copy [PHASE_1B_BROWSER_FAILURE_PRE_FIX_STALE_QUEUED_REVISION.json](verification/PHASE_1B_BROWSER_FAILURE_PRE_FIX_STALE_QUEUED_REVISION.json) preserve the later queued stale-revision timeout at `2026-08-20T09:27:05.078Z`.
- The first race required a changed revision to be queued behind a running old revision. The later race required an already queued ID to refresh its pending revision instead of losing its cache entry before drain.
- One full Rust workspace run before the Slide index cache correction failed the existing 100k timing bound. It was not represented as a pass. Root Slide/preserved-item classification is now cached and refreshed only when relevant root structure changes; the targeted and final full runs pass.
- An initial exact Rust test filter selected 0 tests because the module-qualified name was omitted. The corrected qualified command ran the intended test; the 0-test command is not counted as validation.

## Evidence SHA-256

- `docs/verification/PHASE_1B_BROWSER_PROOF.json` — `9f09fe10487c27198f801986a5f90329888caed927386bc4c58bba8b88a31225`
- `docs/PHASE_1B_METRICS.json` — `4eb4137631046f51eeb1c10ca17d8f0a0fc42397001847e01f5d9aa65518ebeb`
- `docs/verification/PHASE_1B_BROWSER_FAILURE.json` — `058bcd04d9e956dbdf8c9beb42a00c2729ae034a1b1b0051b8c03cb5ed8bcab6`
- `docs/verification/PHASE_1B_BROWSER_FAILURE_PRE_FIX_QUEUE_RACE.json` — `cdef37db60677609f69096e126c3aea20b2d9e4c1d1ad413aa7fd2c0c4b8d51f`
- `docs/verification/PHASE_1B_BROWSER_FAILURE_PRE_FIX_STALE_QUEUED_REVISION.json` — `058bcd04d9e956dbdf8c9beb42a00c2729ae034a1b1b0051b8c03cb5ed8bcab6`
- `docs/verification/phase1b-multislide-workspace.png` — `5733e80f32b90a0f4f61da6bdc07b29a4bb1251d6c306b955ae1d78ae4f40b0d`
- `docs/verification/phase1b-layers-timeline.png` — `5512ccd91c737ae739935891ed0bf2a96cc987556ff4e7bf3670f40c7b3c5c58`

## Scope integrity

- Phase 0 protected evidence changes: 0.
- Existing Phase 1A evidence and ADR-042 through ADR-044: unchanged.
- Phase 1C and later features: not started.
- Review ZIP: not created.

## Run locally

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/start-editor.ps1
```

The editor uses the real Dedicated Worker/WASM/WebGPU path. It has no mock, Canvas2D product fallback, or pre-started server requirement.
