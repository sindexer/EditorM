# Phase 2 Vector and Text Authorization

## Authorization boundary

This document proposes the next authorized product phase after Gate 1B passed on clean `main`
commit `330476b57ba4f16963f5160e24ceb82a21d66438`. It becomes the Phase 2 authorization only after
review and merge. Until then, Phase 2 product implementation remains unauthorized.

Phase 2 is limited to the Vector and text scope already ordered in `docs/ROADMAP.md`. It does not
authorize Phase 3 pivot, layout, constraints, or components; Phase 4 effects or images; Phase 5
slides, timeline, or motion; Phase 6 broadcast semantics; Phase 7 AI behavior; or Phase 8 projects,
compatibility, and export.

## Baseline

- Gate 1B source commit: `330476b57ba4f16963f5160e24ceb82a21d66438`.
- Gate 1B run: `phase1b-20260823T062230Z-330476b57ba4-daa2b79a`.
- Gate 1B result: 22 PASS, 0 FAIL, 0 UNVERIFIED.
- Approved Phase 0E-R3 payload and immutable history remain unchanged.

## Required implementation order

Phase 2 proceeds in the following order. A later stage cannot be pulled into an earlier stage.

### Phase 2A — Path core and Pen/Bezier creation

- Versioned path geometry with stable node identity, finite anchors, and explicit straight and
  cubic Bezier segments.
- Typed, atomic commands for creating paths and editing their geometry.
- Pen and Bezier creation through the real editor, Worker, WASM runtime, scene, renderer, and GPU.
- Open and closed paths, fill behavior, stroke behavior, bounds, hit testing, selection, save/load,
  undo/redo, and deterministic command replay.
- The graphics toolbox is horizontal and directly above the editing canvas, as required by
  ADR-045. Moving the toolbox does not authorize any later-phase tool.

### Phase 2B — Path editing and extended stroke

- Direct anchor and control-handle selection and editing.
- Segment insertion, deletion, open/close, and straight/curve conversion with typed failures and
  failure atomicity.
- Extended stroke properties with an explicit schema, including cap, join, miter limit, and dash
  pattern. Any additional stroke property requires review before implementation.
- Geometry-aware bounds, hit testing, culling, and rendering without hidden full rebuilds.

### Phase 2C — Gradients and composite fills

- Versioned linear and radial gradient definitions with stable stop ordering and finite values.
- Editable gradient geometry and stops through typed, undoable commands.
- Ordered composite fills with explicit blend and opacity semantics limited to modes authorized by
  the stage review.
- CPU, hit-test, WebGPU, serialization, and browser pixel evidence must agree on color semantics.

### Phase 2D — Text

- A versioned native text object that remains selectable and editable rather than flattening to an
  image or path.
- Typed text content and style commands, deterministic shaping inputs, bounds, hit testing,
  serialization, undo/redo, Worker transport, and GPU rendering.
- Font availability, substitution, loading failure, and missing-font behavior must be explicit and
  recoverable. Font files or licenses are not committed unless separately approved.
- Rich-text ranges, text-on-path, variable-font axes, and external font services are excluded unless
  a reviewed follow-up authorization adds them.

### Phase 2E — Boolean operations

- Union, subtract, intersect, and exclude operations over eligible vector geometry.
- Deterministic output, documented fill rule, typed unsupported-input errors, failure atomicity,
  serialization, undo/redo, bounds, hit testing, and renderer agreement.
- The implementation must define whether results remain live or become editable path geometry;
  that choice requires an ADR before product code is merged.

## Cross-stage engineering requirements

- Public boundaries reject non-finite, malformed, stale, duplicate, and unsupported inputs with
  typed recoverable errors and no partial mutation.
- Document migrations preserve existing Phase 0 and Phase 1 files. Save/load/save remains stable.
- Document, scene, spatial index, render model, Worker projection, GPU buffers, overlays, and history
  remain synchronized after execute, undo, redo, transaction rollback, load, and worker restart.
- Bounded and order-statistic structures remain bounded. No full document scan, dense rewrite,
  fallback rebuild, or per-frame full path tessellation may be hidden in an interaction path.
- Product, test, evidence, and governance changes remain distinguishable in review.
- Every stage receives focused Rust, WASM, React, serialization, browser, and negative-path tests
  proportional to its surface area.

## Gate 2 evidence

Gate 2 runs only after all authorized Phase 2 stages merge. It requires:

- Rust format, Clippy, build, and workspace/all-target tests;
- a fresh pinned WASM build and ABI/EngineHost initialization;
- default editor tests, type checking, and production build;
- actual Chrome, dedicated Worker, WASM, and hardware WebGPU proof;
- browser pixel and interaction evidence for paths, stroke, gradients, text, and boolean results;
- serialization migration and save/load/save evidence;
- bounded-work and fallback-rebuild counters with thresholds defined before measurement;
- one run ID binding command, timestamps, environment, source commit, exit status, raw samples, and
  final machine-checked Gate status.

Stored JSON is never represented as a fresh execution. Hardware-dependent proof must be generated
locally on actual hardware; hosted CI may validate it but cannot claim to have executed it.

## Explicit exclusions

- Editable transform pivot/origin and pivot-aware transforms (Phase 3).
- Auto layout, constraints, components, and variants (Phase 3).
- Shadows, image fills, placement, replacement, and crop (Phase 4).
- Slides, integrated layer/timeline rows, keyframes, easing, and motion (Phase 5).
- Data binding and broadcast template semantics (Phase 6).
- AI connectivity or commands beyond a nonfunctional reserved tab (Phase 7).
- Multiple projects, import compatibility, and export (Phase 8).

## First implementation checkpoint

After this authorization merges, the next product PR is Phase 2A only. It must establish the path
schema and typed Rust/document/serialization contract before exposing Pen/Bezier UI or GPU output.
The UI and renderer follow in later Phase 2A PRs after that contract is reviewed.
