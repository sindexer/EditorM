# EditorM Product Roadmap

This roadmap preserves the approved development order. A requirement may be clarified inside its mapped phase, but it must not be implemented earlier without a new authorization.

## Completed baseline: Phase 0

Phase 0A through Phase 0E-R3 established finite core math, typed and reversible document mutation, scene and spatial derivation, render-model deltas, Worker-owned WASM execution, real WebGPU rendering, the professional editor shell, structural group/ungroup operations, exact ordering, bounded diagnostics, and actual hardware evidence.

Gate 0E-R3 is the approved GitHub baseline. Historical Phase 0 documents and verification artifacts remain part of the initial repository record.

## Phase 1: Direct-manipulation editor foundation

Authorized only by a separate Phase 1 instruction. Planned requirements:

- pixel-aligned rectangle corners and thin-line rendering rules;
- analytic coverage anti-aliasing and MSAA where justified;
- DPR, zoom, pan, rotation, and transform-aware rendering checks;
- configurable frame/canvas sizes, including 1920x1080 and 3840x2160 presets, with DCI 4K only as a user-selected size;
- frame boundaries, clipping, selection, direct move, and resize;
- multiple selection, alignment, distribution, and snapping;
- opacity, default fill, corner radius, and default stroke;
- fully undoable human direct manipulation.

Exit criteria must cover exact document semantics, transaction/history behavior, viewport mapping, visual correctness, accessibility, bounded work, and fresh hardware proof where GPU paths change.

## Phase 2: Vector and text editing

- Pen and Bezier tools;
- vector path editing;
- extended stroke properties;
- gradients and composite fills;
- text editing and layout;
- boolean operations.

## Effects and image phase

- drop shadow and inner shadow;
- image fills;
- image placement, replacement, and crop;
- renderer and export paths that preserve effects.

## AI-assisted editing phase

- Qwen image-model integration;
- conversational editor control;
- natural-language requests translated into typed editor commands;
- dry-run and change previews;
- deterministic execution and complete undo/redo;
- human editing of AI-created results with the same tools;
- editable document objects rather than flattened pixels;
- revision, transaction, selection, and GPU atomicity under latency, retry, duplicate, stale, and out-of-order responses.

## Project and import/export phase

- image import;
- frame-scoped export;
- multiple projects;
- per-project Document, history, selection, and viewport state;
- unsaved-change warnings;
- isolation of Worker and GPU responses during project switching.

## Gate policy

Every phase requires explicit authorization, scoped acceptance criteria, focused regression coverage, truthful verification evidence, and review approval. Review ZIPs are created only for formal gates or releases, not for ordinary pull requests.
