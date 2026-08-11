# Review Packet - Phase 0D-R1

## 1. Decision, authorization, and baseline incident

- Date: 2026-08-10 (Asia/Seoul).
- Workspace has no Git metadata; commit and branch are N/A.
- Gate 0D remained held by `docs/PHASE_0D_EXTERNAL_REVIEW.md` for three blocking
  structural defects.
- Work was limited by `docs/PHASE_0D_R1_AUTHORIZATION.md` to those corrections and their
  proof. Phase 0E, React, Wanted Design System product UI, and later features were not started.
- The first standalone baseline `npm run test:browser` timeout was caused by no server on the
  default port 4173. `NO_SERVER_ON_4173` was observed. A source-unchanged rerun through
  `tools/phase0d-verify.ps1 -BrowserPort 4181` created a fresh actual-hardware pass, so the
  timeout was classified as a harness invocation prerequisite rather than a product regression.
  The original failure output was preserved. Details are in
  `docs/PHASE_0D_R1_BASELINE_INCIDENT.md`.

## 2. Blocking defect corrections

### Atomic Document, derived state, and GPU synchronization

Finite f64 Document values no longer fail merely because an instance cannot be represented as
f32. RenderModel records a typed encoding diagnostic, keeps the stable slot, writes a zero GPU
record, and removes that item from visible slots. Returning to an encodable value restores the
same slot. Invalid requests still return `ok:false` before mutation and expose no binary write.

The external `1e100` reproduction changed as follows:

| Behavior | Before R1 | After R1 hardware proof |
| --- | --- | --- |
| Request result | `ok:false`, `f32_conversion` | `ok:true` |
| Revisions | advanced despite failure | Document/Scene/Render all synchronized at 16 |
| GPU synchronization | no delta | 1 dirty slot, 48-byte instance upload |
| Encoding state | persistent/GPU divergence | 1 omitted item, `translation_outside_f32` |
| Recovery | not demonstrated | omitted count 0, same stable slot restored |

Protocol-error regression tests compare Document, history, transaction, selection, Scene,
RenderModel, pending binary data, and revisions before/after failure. Multi-dirty, undo/redo,
transaction preview/rollback, omission recovery, and no-partial-GPU-write cases also pass.

### Bounded RenderModel work and order maintenance

`RenderModel::apply_changes` no longer clones the complete model. It prepares only affected
items, completes fallible source reads before commit, and then performs an infallible bounded
in-place update. Exact total/renderable/encodable/omitted/allocated/free counters are maintained
incrementally and checked against a scanning oracle.

Dense whole-document render order keys were removed. Scene nodes cache sibling indexes;
hierarchy changes refresh the affected sibling list while transform/geometry/appearance/
visibility changes do no order maintenance. Candidate order paths use cached indexes in
O(depth), so sibling linear searches are zero.

### Worker response and GPU frame ordering

Every EngineHost response has a monotonically increasing `engine_sequence`. GPU application is
serialized in one generation-aware queue; response, GPU frame, revisions, and GPU metrics must
match before HUD/proof state is committed. Heartbeat changes liveness only and does not replace
the frame. Worker restart rejects pending waiters with typed `worker_restarted`, resets sequence
state, and prevents pre-restart completion from being applied.

Pointer and wheel intent is combined at animation-frame boundaries with at most one camera
request in flight. The final hardware burst observed 309 raw camera intents, 52 requests sent,
267 coalesced observations, and 257 dropped intermediate requests. A delayed-frame proof ended
with engine sequence 14 and GPU sequence 14. The restart waiter returned `worker_restarted`.

## 3. Shared native/browser contract

- `shared/render_contract.wgsl` is consumed directly by native Rust and generated into the
  browser contract module.
- `shared/render_binary_schema.json` defines schema version 1, little-endian encoding,
  48-byte instances, 52-byte dirty records, 32-byte uniforms, field offsets, and primitive
  values.
- Request protocol version and render binary schema version are independently validated.
- Build and unit tests fail if the generated browser contract drifts from the shared sources.

ADRs 026-028 record the f64/GPU omission boundary, bounded patching, cached sibling order,
sequenced frames, camera coalescing, and shared contract decisions.

## 4. Structural metrics

Final native proof used NVIDIA GeForce GTX 970 through Vulkan. Structural counters are the gate
criterion; wall time is supporting evidence and varies with machine load.

| Scenario | External Gate 0D measurement | Final R1 measurement | Required work counters |
| --- | ---: | ---: | --- |
| 10k single leaf edit | 7.4 ms | 0.148 ms | dirty 1, clone 0, full scan 0, order walk 0, upload 48 B |
| 100k single leaf edit | 96.1 ms | 0.043 ms | Document 1, RenderItem read/write 1/1, clone 0, full scan 0, upload 48 B |
| 10k fully visible cull/order | about 568.9 ms | 9.185 ms | candidates/visible 10,000/10,000, sibling search 0, 1 batch/1 draw |
| 100k load | 2.64 s | 1.425 s | explicit fixture load/full initialization only |
| 100k narrow camera cull | not supplied | 0.037 ms | candidates/read 1/1, full RenderModel scan 0, revision changes 0 |

The final native JSON contains 18/18 true checks. The browser proof independently loaded 10k
in 21 ms and sparse 100k in 5,093 ms in Worker/WASM; browser timings are not substituted for
the native structural counters.

## 5. Self-contained browser harness

`npm run test:browser` now works without an external server. It reserves an available localhost
port, starts the preview, checks `/__health`, validates the following assets, launches a new
Chrome process/profile, runs real WebGPU proof, and cleans up in `finally`:

- `/index.html`: HTTP 200, `text/html`
- `/src/worker.js`: HTTP 200, JavaScript MIME
- `/pkg/engine_host.js`: HTTP 200, JavaScript MIME
- `/pkg/engine_host_bg.wasm`: HTTP 200, `application/wasm`

Server connection/health, asset HTTP, asset MIME, WASM boot, Worker request, WebGPU
initialization, and general readiness have distinct typed diagnostics. The old behavior of only
waiting 45 seconds for a server-dependent readiness condition was removed. Unit tests cover the
asset and application-error classifications.

The final fresh proof was captured at `2026-08-09T16:06:31.661Z` on Chrome 151.0.7922.76 using
an actual NVIDIA GeForce GTX 970. Worker WASM heartbeat was at least 1. Console errors, GPU
validation errors, and fallback rebuilds were 0. All 24 browser checks passed.

## 6. Actual WebGPU pixel and DOM proof

WebGPU readback renders through the production pipeline into a GPU texture and copies one pixel
through a mapped buffer. The following assertions passed:

- rectangle interior: RGBA `(51, 148, 245, 255)`;
- ellipse center: RGBA `(245, 115, 64, 255)`;
- ellipse AABB corner: background `(9, 14, 20, 255)`;
- overlap initial/reorder/undo/redo topmost colors;
- offscreen center remains background.

Automation dispatches actual pointer down/move/up, wheel, click, select change, Move, Undo,
Redo, Fit, Reset, and Restart Worker interactions through Chrome DevTools Protocol. There is no
WebGPU mock, Canvas2D fallback, or static-image success path.

## 7. Test and verification accounting

Every required Windows command in `tools/phase0d-r1-verify.ps1` returned exit code 0. Raw output
and timestamps are in `docs/verification/PHASE_0D_R1_VERIFICATION.txt`.

- Rust unit/integration/property tests: 154 passed, 0 failed, 0 ignored.
- Compile-fail doctest: 1 passed, 0 failed, 0 ignored.
- Web unit tests: 6 passed, 0 failed, 0 skipped/todo/cancelled.
- Actual hardware browser artifact test: 1 passed, 0 failed, 0 skipped/todo/cancelled.
- Native R1 checks: 18/18 true.
- Browser R1 checks: 24/24 true.
- Pixel assertions: 8/8 true.
- Phase 0C and Phase 0D regression proofs: `all_passed=true`.

Executed command set:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 tree --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0d-wasm.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0c-proof.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-proof.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-r1-proof.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/phase0d-r1-verify.ps1
```

Web clean install, build, unit tests, and self-contained browser hardware proof were rerun inside
the final orchestration. Existing Phase 0D evidence was not deleted. Phase 0D regression scripts
now validate the freshly generated R1 browser artifact rather than treating a saved old JSON as
new execution evidence.

## 8. Artifacts and packaging

- Native metrics: `docs/PHASE_0D_R1_METRICS.json`
- Raw verification: `docs/verification/PHASE_0D_R1_VERIFICATION.txt`
- Browser proof: `docs/verification/PHASE_0D_R1_BROWSER_PROOF.json`
- Pixel readback: `docs/verification/PHASE_0D_R1_PIXEL_READBACK.json`
- Screenshot: `docs/verification/phase0d-r1-preview.png`
- Manual repeat: `docs/PHASE_0D_R1_MANUAL_CHECKLIST.md`

The review archive target is
`C:\Users\thdwl\Documents\Codex\visual_authoring_engine_phase0d_r1_review_2026-08-09.zip`.
It contains a `WebEditor/` root and an internal `CHECKSUMS.sha256`. `target`, `node_modules`,
`.tools`, IDE/cache content, nested ZIPs, credentials, and temporary server logs are excluded.
The archive SHA-256 is recorded in the adjacent `.sha256` sidecar and final handoff because an
archive cannot contain its own stable hash.

## 9. Known limits and stop boundary

- Browser WebGPU exposes adapter identity but not its internal implementation backend; the
  browser proof reports that limitation rather than inventing a backend value.
- Wall times are single-run evidence on this machine; structural visit/clone/scan/upload
  counters are the primary gate proof.
- The diagnostic preview is not a production editor UI.
- Phase 0E, React, Wanted Design System product UI, and every later system remain unstarted.

Work stops at this Gate 0D-R1 review package pending external review.
