# Phase 0D-R1 Authorization

Status: **authorized for structural correction only**.

## Authorized work

- Preserve atomic state across Document, history, transactions, selection,
  Scene, RenderModel, and GPU-visible data when an operation fails.
- Keep valid f64 Document state independent from GPU f32 encoding. Record typed
  render-encoding diagnostics, omit unencodable resident items, and recover the
  same stable slot when values become encodable again.
- Replace full RenderModel cloning and unrelated full scans with prevalidated,
  bounded incremental patches and counters.
- Remove dense full-document structural-order recomputation and linear sibling
  searches from culling order.
- Add structural work counters and 10k/100k proof fixtures.
- Add monotonic engine/frame sequencing, serialized latest-frame application,
  camera input coalescing, typed Worker restart rejection, and stale-frame
  rejection.
- Establish a checked common WGSL/binary-layout contract and a render binary
  schema version distinct from the request protocol version.
- Add actual WebGPU pixel readback and real DOM interaction proof.
- Produce Phase 0D-R1 ADRs, metrics, verification logs, checklist, review packet,
  scripts, and a checksummed review ZIP.

## Required invariants

- An `ok:false` request changes no persistent or derived state and performs no
  partial GPU write.
- A successful Document change either supplies a synchronizing GPU delta/full
  resync or explicitly omits unencodable items with a typed diagnostic.
- A 100k single-leaf transform clones and fully scans zero RenderModels, visits
  no unrelated RenderItems, and dirties/uploads one instance.
- Camera-only culling uses spatial candidates and changes no Document, Scene, or
  Render revision.
- Heartbeat performs no render/culling payload generation and no GPU frame.
- Older or pre-restart frames cannot replace newer HUD or GPU metrics.
- Hardware proof uses real WebGPU, pixel readback, and dispatched DOM input; no
  mock or saved JSON is counted as execution.

## Explicitly prohibited

- Phase 0E implementation.
- React or Wanted Design System product UI work.
- Hiding or overwriting original Phase 0D evidence.
- Claiming unexecuted, ignored, skipped, mocked, or cached proof as passing.

Work stops after the Gate 0D-R1 review package is produced.
