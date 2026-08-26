# EditorM Product Roadmap

This roadmap restores and preserves the approved development order and phase numbering. Phase ordering must not change; per-phase status notes below record what has since been implemented.

## Completed baseline: Phase 0

Phase 0A through Phase 0E-R3 established finite math, typed reversible document mutation, scene/spatial derivation, render-model deltas, Worker-owned WASM execution, actual WebGPU rendering, the professional editor shell, structural group/ungroup, exact ordering, bounded diagnostics, and actual hardware evidence. Gate 0E-R3 remains the approved baseline.

## Phase 1 — Direct-manipulation editor foundation

Status: Phase 1A and Phase 1B are implemented, merged, and verified. Gate 1B passed on clean `main` commit `330476b57ba4f16963f5160e24ceb82a21d66438`; see `docs/verification/PHASE_1B_GATE_STATUS.json`. Phase 2 and later remain unapproved and unimplemented.

- analytic coverage anti-aliasing for circles and ellipses;
- MSAA only where its need is demonstrated;
- verification across DPR, zoom, pan, and rotation;
- Frame tool;
- 1920×1080 default, followed by 3840×2160, optional DCI 4K, and user-defined sizes;
- direct selection, movement, and resizing;
- multiple selection;
- alignment, distribution, and snapping;
- opacity;
- default fill;
- corner radius;
- default stroke;
- fully undoable and redoable human direct editing.

## Phase 2 — Vector and text

Status: authorization is defined in `docs/PHASE_2_AUTHORIZATION.md`. Phase 2A path schema,
rendering, hit testing, Pen/Bezier creation UI, direct-WASM proof, and actual-hardware browser
evidence merged in PR #23. Phase 2B path editing and extended stroke is now at its contract
checkpoint under `docs/PHASE_2B_AUTHORIZATION.md` and ADR-049. Gradients, text, and boolean
operations have not started.

- Pen and Bezier tools;
- path editing;
- extended stroke;
- gradients and composite fills;
- text;
- boolean operations.

## Phase 3 — Layout and components

- auto layout;
- constraints;
- editable per-object transform pivot/origin, with canvas and numeric controls;
- pivot-aware move, rotate, scale, resize, serialization, and undo/redo semantics;
- reusable components and instances;
- component properties and variants;
- structures that remain directly editable by people.

## Phase 4 — Effects and images

- drop shadow;
- inner shadow;
- image fill;
- image placement, replacement, and crop;
- rendering that preserves effects;
- native image objects that remain selectable and editable.

## Phase 5 — Slides and motion

- one document file containing an ordered collection of slides;
- a full-height slide browser at the left edge of the workspace;
- slide add, duplicate, delete, rename, reorder, and selection workflows;
- slide-local object ownership, layer order, duration, playhead, and animation state;
- keyframes;
- easing;
- one bottom timeline that begins to the right of the full-height slide browser;
- layer-tree functions integrated into timeline rows rather than a separate Layers tab;
- per-layer time bars, keyframes, visibility, locking, hierarchy, ordering, and selection;
- canvas, timeline-row, property, history, Worker, and GPU state synchronized to the selected slide;
- editable motion properties;
- explicit separation between static editor state and animation state.

## Phase 6 — Broadcast-graphics semantic structure

- semantic objects for news and broadcast graphics;
- data binding;
- templates;
- automatic generation of repeated graphics;
- structures compatible with PSD/AE production workflows.

## Phase 7 — Qwen internal-network AI editing

Qwen is first defined as an internal-network LLM command controller, not merely as a Qwen image-model integration.

- internal-network Qwen connection;
- editor control through a chat tab;
- an AI tab integrated into the right property panel, with no separate floating AI workspace required;
- natural language converted into typed editor commands;
- dry-run and change previews;
- deterministic replay;
- undo/redo for every AI change;
- human editing of AI-created results with existing tools;
- native document objects rather than flattened pixels;
- fast high-volume creation and editing;
- revision and transaction atomicity;
- defense against retry, duplicate, stale, and out-of-order responses;
- internal-network failure and reconnection handling;
- any Qwen image model separated as an image-asset provider rather than confused with the document command controller.

Internal Qwen IP addresses, tokens, and account data must never be committed. `.env.example` may contain variable names and descriptions only.

## Phase 8 — Projects, compatibility, and export

- image import;
- frame-scoped image export;
- multiple project tabs;
- per-project Document, history, selection, and viewport state;
- unsaved-change warnings;
- Worker/GPU response isolation during tab changes;
- asset and external-format compatibility.

## Future workspace layout contract

ADR-045 defines the cross-phase workspace destination without authorizing an unapproved phase. The left slide browser spans the full workspace height. The remaining workspace places a horizontal graphics toolbar above the canvas, a property/AI tab panel on the right, and a combined layer/timeline panel below the canvas and property region. The slide browser must not be shortened or covered by the timeline.

The boundaries between the slide browser and work area, canvas and property panel, canvas/property region and timeline, and timeline layer columns and time tracks are draggable splitters with bounded minimum and maximum sizes. Pane sizes, collapsed state, and selected property tab are workspace preferences rather than document undo/redo mutations.

## Gate policy

Every phase requires explicit authorization, scoped acceptance criteria, focused regression coverage, truthful verification evidence, and review approval. Review ZIPs are created only for formal gates or releases, not for ordinary pull requests.
