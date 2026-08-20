# ADR-042: Versioned Primitive Appearance Schema

- Status: Implemented for Phase 1A review
- Date: 2026-08-14
- Follows: ADR-041

## Context

Phase 1A makes fill, opacity, four-corner radii, and a basic stroke persistent editor data. Treating those values as transient UI or backend-only data would break undo/redo, save/load, Worker projection, and stable incremental rendering.

## Decision

Appearance owns sRGB ColorRgba fill, independent opacity, four ordered corner radii, and one solid centered Stroke. Colors and alpha are finite and constrained to [0, 1]; radii and stroke width are finite and non-negative. Rectangle and Frame preserve all four radii even though Phase 1A exposes a uniform editing control.

Persistent document format version 2 stores all appearance fields. Version 1 input migrates absent appearance fields to explicit defaults while retaining its opacity. Render binary schema version 2 carries linear fill/stroke, opacity, radii, and width. The request protocol version remains independent.

Every appearance edit is a typed command and history entry. Fill, color, radius, and opacity dirty one stable render slot. A stroke-width change additionally refreshes that node's bounds and spatial entry because centered stroke expands geometry.

## Consequences

React never becomes a second Document owner. Save/load, undo/redo, Scene, RenderModel, native wgpu, Worker/WASM, and browser WebGPU share one versioned semantic path. Gradient, image fill, shadows, and extended stroke styles remain excluded.
