# ADR-029: Worker-Owned Document with Delta UI Projection

- Status: Accepted for Phase 0E
- Date: 2026-08-10

## Context

The React editor needs names, hierarchy, selection, bounds, transforms, visibility, and lock state. A second mutable JavaScript node graph would split authority from the Rust Document and could produce stale commands, divergent hierarchy, or full 100k-node serialization after a single edit.

## Decision

The Dedicated Worker owns the Rust/WASM `EngineHost`, Document, Scene, RenderModel, selection, history, transactions, and camera. Every persistent request uses a stable `NodeId` and a typed protocol request.

The Worker emits a read-only UI projection as either an initial full snapshot or bounded upsert/remove deltas. React stores only this projection and ephemeral presentation state. A hierarchy version changes only for structural deltas; Layers derives and virtualizes rows only when that version changes. Selection remains session state and does not create a Document node or persistent revision.

The response is published to React only after its binary render delta has been applied to the WebGPU renderer. Engine response, GPU frame, and selection overlay therefore use one sequence.

## Consequences

- A single-leaf edit does not require a full UI snapshot or full Layers serialization.
- React cannot mutate persistent state without the typed Worker boundary.
- Worker restart uses a generation boundary so stale responses and waiters cannot update the new projection.
- Debug counters expose full snapshots, delta nodes, Layers serializations, hierarchy rebuilds, mounted rows, and pointer coalescing.
