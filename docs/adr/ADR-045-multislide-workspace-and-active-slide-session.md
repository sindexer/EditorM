# ADR-045: Multislide Workspace and Active Slide Session

- Status: Accepted for Phase 1B implementation
- Date: 2026-08-20
- Follows: ADR-044

## Context

EditorM is becoming a professional motion-graphics and presentation editor in which one persistent file contains multiple 16:9 slides. Phase 1B needs stable slide identity, full-document undo/redo, slide-scoped interaction state, and derived WebGPU thumbnails without replacing the existing Document, hierarchy, Scene, RenderModel, Worker, or WASM ownership model.

## Decision

A Slide is a Frame that is a direct child of the Document root. Its existing NodeId is the stable slide identifier, root child order is slide order, and the Frame name is the slide name. A Frame nested below a Slide remains a normal editable Frame and is never promoted to a Slide. Only Slide commands create root-level Slide Frames; the existing Frame tool creates a nested Frame below the active Slide.

Existing root-level Frames load as Slides in their stored order. Non-Frame root children remain persistent and are exposed in a preserved-root-items section so they are never silently deleted, hidden, or rewritten. A malformed hierarchy that cannot identify at least one valid root-level Slide fails through a typed recoverable boundary. A newly created Editor document always contains one 1920x1080 Slide.

The Worker-owned Editor Session stores the active slide, selection by slide, and camera by slide. Hover, active tool, and engine-independent panel presentation are also session or local preferences rather than Document data. Activating a Slide restores its selection and camera without changing Document revision or creating history. Persistent slide create, duplicate, rename, reorder, and delete operations remain typed document commands and each produces one global history entry. History is global to the file, so undo may affect a different Slide than the currently active one.

Deleting the final Slide is rejected. Duplicate performs a deep subtree copy with fresh NodeIds and preserved hierarchy/order. Delete history restores original NodeIds, subtree content, and root order. New objects are parented below the active Slide. Scene queries, render visibility, hit testing, marquee selection, and layer projection are restricted to the active Slide subtree.

Slide thumbnails are disposable derived cache entries keyed by Slide NodeId and slide-scoped revision. They are not serialized and are never a JavaScript copy of the document. Edits invalidate only the owning Slide. The WebGPU renderer prioritizes the active Slide, then visible thumbnails, then deferred entries, with a bounded per-frame budget. Pending and typed error states are explicit; Canvas2D and static-success fallbacks are prohibited.

The Phase 1B Timeline is a synchronized layer-track shell only. It shares NodeIds, row order, selection, visibility, lock state, virtualization, and scroll position with Layers. It contains no keyframes, playback, duration, easing, timebase, auto-key, animated properties, or non-functional transport controls. Those remain in Phase 5 Motion.

## Consequences

Phase 1B reuses proven stable hierarchy and ordering structures while establishing a multislide workspace without a schema rewrite or second React-owned document. Slide switching remains cheap and reversible UI session state, while all persistent mutations stay in one deterministic history. If future duration, transition, or collaboration requirements cannot be represented safely by the top-level Frame interpretation, a later ADR must introduce a versioned Slide entity rather than silently weakening this contract.
