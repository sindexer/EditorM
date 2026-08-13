# Phase 0E Authorization

## Authority

Phase 0E was resumed from the externally accepted Phase 0D-R1-P1 baseline under the user's **Phase 0E Resume Authorization - Figma Source Connection and Truncated Instruction Replacement** dated 2026-08-10.

The supplied Wanted Design System source is:

- Figma file key: `xGUOJQFvJxpZYyvHP18lLw`
- URL: `https://www.figma.com/design/xGUOJQFvJxpZYyvHP18lLw/Wanted-Design-System--Community-`
- local reference: `visual_authoring_engine_codex_package/reference/wanted-design-system/Wanted Design System (Community).fig`

Actual Figma metadata and representative component design contexts were inspected before product UI implementation. The remote source, local `.fig`, `meta.json`, thumbnail, and authoritative UI specification were cross-checked. Detailed component and token provenance is recorded in `docs/UI_COMPONENT_MAPPING.md` and `docs/UI_TOKEN_PROVENANCE.md`.

## Authorized implementation scope

This gate authorizes only the professional editor shell and direct-manipulation foundation:

- a new React and TypeScript product app under `web/editor`;
- app bar, tool rail, virtualized Layers panel, actual WebGPU canvas, non-document selection overlay, transform Inspector, status bar, collapsible Debug panel, and Wanted component showcase;
- select and Shift multi-select, Rectangle/Ellipse creation, move, resize, rotate, atomic Group/Ungroup, nested group editing, visibility/lock, numeric Inspector edits, Undo/Redo, Escape rollback, pan, zoom-around-pointer, fit, Worker restart, and 10k/100k fixtures;
- stable `NodeId` typed mutation through the Dedicated Worker-owned Rust/WASM runtime;
- Phase 0E proof, documentation, and review packaging.

The existing `web/phase0d-preview` remains a diagnostic predecessor and was not converted into or replaced by the Phase 0E product UI. Previous-stage evidence remains frozen.

## Explicitly unauthorized

Phase 1 was not authorized or started. The gate excludes marquee selection, snapping/guides, alignment/distribution, duplicate/copy/paste, numeric scrubbing, custom pivots, vector paths/text, boolean/effects, image pipelines, timeline/animation, collaboration/plugins/AI, PSD or After Effects integration, and export pipelines.

## Stop boundary

Work stops after the Gate 0E review ZIP and SHA-256 sidecar are created. This document records authorization, not Gate approval. System shutdown or restart is not part of this authorization and is not performed.
