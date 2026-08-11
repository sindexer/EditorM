# Phase 0E-R1 Structural and Evidence Correction Authorization

- Date: 2026-08-10
- Baseline ZIP: `visual_authoring_engine_phase0e_review_2026-08-10.zip`
- Baseline SHA-256: `2e1481e7b6da938872c3e75f2726fdbaeb3d28a47b8e72dc28c97a3fcac9b60c`
- Baseline entries: 210, including 209 project checksum entries
- Gate state: Phase 0E not approved; Phase 1 not authorized

## Authorized work

- Replace clone-and-fallback Group/Ungroup with validated reversible patches.
- Apply structural changes incrementally to Scene and RenderModel.
- Introduce a separate versioned UI projection schema with bounded structural operations.
- Keep Layers mounted work bounded for 10K and 100K fixtures.
- Serialize Escape, DOM pointercancel, queued intent, in-flight intent, rollback, and Worker restart with interaction generations.
- Remove browser-proof false positives and record exact non-no-op single-leaf edits.
- Complete the already authorized Phase 0E component showcase, typed numeric validation, accessibility behavior, regression tests, evidence, and R1 packaging.

## Prohibited work

Phase 1 and later features are not authorized, including marquee, snapping, guides, align/distribute, duplicate/copy/paste, scrubbing, pivot editing, vector paths, text, effects, image workflows, animation, collaboration, or export.

Existing Phase 0E and prior-stage evidence must remain byte-identical. New evidence uses Phase 0E-R1 names.

