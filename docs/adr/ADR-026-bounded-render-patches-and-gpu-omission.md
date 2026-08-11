# ADR-026: Bounded Render Patches and GPU Omission

Status: Accepted for Phase 0D-R1

## Context

The Document stores finite f64 values, while the renderer consumes f32 instance records. The
original EngineHost converted changed items after persistent and derived revisions advanced.
An f64 value such as a translation of `1e100` therefore produced `ok:false` after Document,
Scene, and RenderModel had changed, with no GPU delta. RenderModel atomicity also depended on
cloning the complete model before every incremental update.

## Decision

- Finite f64 Document values remain valid independently of GPU encoding range.
- RenderModel prevalidates only affected nodes into bounded prepared patches. All fallible
  Document and Scene lookups finish before an infallible in-place commit; the whole model is
  never cloned for incremental atomicity.
- Each RenderItem records whether it is GPU encodable. An unencodable resident item keeps its
  stable slot, receives a typed `RenderEncodingDiagnostic`, is encoded as a zero record, and is
  omitted from visible slots.
- EngineHost returns a successful synchronized revision and dirty GPU payload together with the
  diagnostic. Returning to an encodable value restores the same slot incrementally.
- Protocol/request failures still return `ok:false` before runtime mutation. They expose no
  binary payload and change no Document, history, transaction, selection, Scene, RenderModel, or
  GPU-visible state.
- Exact RenderModel population counts are maintained incrementally rather than recomputed for
  responses or culling.

## Consequences

Document precision is no longer constrained by GPU f32 range, and a successful persistent
change always has an explicit GPU synchronization outcome. The renderer must understand
zero-record omission and diagnostics. Full rebuild remains permitted only for explicit full
document replacement; ordinary edits use bounded patches.

