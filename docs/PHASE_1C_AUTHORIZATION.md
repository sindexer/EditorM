# Phase 1C Direct Editing Authorization

## Scope

Phase 1C completes direct editing inside one active slide: deterministic selection, single and aggregate move/resize/rotate, alignment and distribution, Group/Ungroup shortcuts, and synchronized Layers and Inspector editing. It begins from the Phase 1B merge commit `8ffb5e4a90fa03cb65b08fe8bc295d347d779a1c` on branch `feat/phase1c-direct-editing`.

This authorization does not include Phase 1D, Phase 2, timeline or keyframe editing, Pen/Bezier, advanced text, media editing, effects, export, AI integration, collaboration, or multiple projects.

## Direct editing contract

- The active slide frame is an editing boundary and backdrop, not a canvas-selectable or movable object. Canvas hit testing and marquee selection exclude it, other slides, and hidden or locked descendants.
- Selection and permanent mutations remain Worker/runtime owned. React only projects state and authors typed requests.
- A move, resize, or rotation gesture opens one transaction. Every preview is recomputed from the transaction capture; commit creates one undo step and Escape rolls back the gesture.
- Aggregate resize applies one non-mirroring world transform around the selected handle's opposite anchor, or the common center while Alt is held. Shift requests uniform scale and sizes below one world unit are rejected before mutation.
- Screen-space clockwise rotation is positive. Pointer rotation is a normalized signed delta from pointer-down; Shift snaps that delta to 15-degree increments. Degree/radian conversion occurs at the Inspector/UI boundary.
- Inspector multi-edit sends one validated atomic command batch. Mixed values are presentation state derived from the Worker projection, not a second document state.

## Phase 1B hardware evidence deferral

The user explicitly carried the unexecuted Phase 1B GTX 970 proof into the single Phase 1C final Gate. Phase 1C may not claim that proof before the combined Gate executes on actual Chrome, Dedicated Worker, WASM EngineHost, and NVIDIA GTX 970 WebGPU. A CI-built WASM artifact is acceptable only when its source commit and artifact SHA are verified.

## Evidence and completion

Development runs focused module tests and Editor type/build checks. After the feature is complete, one final Gate must run the complete Rust, WASM, Editor, Preview, Chrome, Worker, and GTX 970 matrix and record commands, timestamps, environment, exit status, and raw samples under `docs/verification/`. The Phase 1C PR remains unmerged for user UI review, and is Ready for review only after `FAIL 0 / UNVERIFIED 0`.
