# ADR-045: Slide Timeline Workspace and Transform Pivot

- Status: Accepted product direction; implementation deferred to authorized phases
- Date: 2026-08-23
- Follows: ADR-044

## Context

The existing editor shell proves direct manipulation around one fixture and exposes separate Layers, canvas, Toolbar, and Inspector regions. The product destination is a broadcast-graphics authoring file containing multiple slides. Selecting a slide must change the canvas and the complete temporal layer context together. A separate Layers tab would duplicate selection, hierarchy, visibility, locking, and ordering controls that belong beside each layer's time bar.

The current transform matrix can support future pivot operations, but EditorM has no stored user-editable transform pivot, pivot handle, numeric pivot control, serialization contract, or pivot-specific undo/redo proof. Earlier phase evidence explicitly excluded custom pivot editing. Without an explicit phase assignment, rotation, scaling, components, and later keyframe interpolation could acquire incompatible implicit origins.

## Decision

### Document and slide ownership

One EditorM file owns an ordered collection of stable-ID slides. Each slide owns its duration, object hierarchy, layer order, selection context, playhead, and animation state. Switching slides is atomic: the canvas projection, integrated layer/timeline rows, property projection, Worker responses, and GPU presentation must all identify the same active slide. Stale or out-of-order responses from a prior slide cannot mutate or present the newly selected slide.

Phase 5 owns the multi-slide document and motion implementation. This decision does not authorize Phase 5 work while Gate 1B or earlier authorized work remains incomplete.

### Workspace topology

The workspace is divided first into a full-height slide browser on the left and the editing workspace on the right. The slide browser continues to the bottom edge and is never clipped, covered, or shortened by the timeline.

The editing workspace contains:

1. a horizontal graphics toolbox directly above the editing canvas;
2. the selected slide's canvas in the center;
3. a right panel containing property and AI tabs; and
4. one bottom panel combining layer-tree controls and temporal tracks.

There is no separate Layers tab in the destination UI. Every timeline row is also an object layer row. Its fixed columns provide name/type, selection, visibility, locking, hierarchy, and render order; its time area provides time bars, keyframes, easing, and temporal range editing. Canvas selection and timeline-row selection are bidirectionally synchronized.

The graphics toolbox gains tools only in their authorized phases. Its fixed location does not authorize Pen, text, image, motion, or other later tools early. The property panel may reserve an AI tab in the layout, but actual Qwen connectivity, command generation, dry-run, preview, deterministic replay, and AI change application remain Phase 7 work.

### Resizable workspace panes

Users can drag visible boundaries between:

- the full-height slide browser and editing workspace;
- canvas and property/AI panel;
- canvas/property region and integrated timeline; and
- the timeline's layer columns and time-track region.

Every splitter has keyboard-accessible adjustment, bounded minimum and maximum sizes, collapse/restore behavior, and a discoverable resize cursor. Pane sizes, collapse state, and the selected property/AI tab are local workspace preferences. They are not persistent document mutations and do not enter document undo/redo history.

### Transform pivot

Phase 3 owns the first implementation of a persistent, editable per-object transform pivot/origin so it exists before Phase 5 animation. The pivot is expressed in a documented object-local normalized representation and is finite and validated at public boundaries. Changing it is a typed, atomic, undoable document command.

The canvas exposes a pivot handle for a single eligible selection, and the property panel exposes exact numeric controls. Rotation, scale, resize, bounds, hit testing, overlays, serialization, grouping, reparenting, components, and undo/redo must agree on pivot semantics. Phase 5 keyframes animate the same property rather than inventing a second animation-only anchor. Multi-selection pivot behavior requires its own authorization and must not be inferred from the single-object contract.

## Consequences

Phase 1B remains unchanged and Gate 1B must complete before later implementation begins. Phase 2 remains Vector and text. Phase 3 authorization must define pivot representation, commands, migration/default-center behavior, interaction, serialization, and evidence. Phase 5 authorization must define the multi-slide schema, integrated timeline, temporal model, slide switching isolation, and hardware/browser proof. Phase 7 authorization must define the AI tab's safe command boundary.

Future UI work must not reintroduce a separate Layers tab, place the graphics toolbox away from the top of the canvas, or allow the timeline to run beneath and truncate the slide browser without a superseding ADR.
