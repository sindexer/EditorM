# ADR-030: Direct Manipulation Transaction FSM and Final-Intent Coalescing

- Status: Accepted for Phase 0E
- Date: 2026-08-10

## Context

Pointer move bursts can exceed the Worker/GPU frame rate. Sending every event creates an unbounded request queue, while dropping arbitrary events can lose the final transform or pan delta. Each move, resize, rotate, or create drag must also become exactly one history entry and support complete Escape/pointer-cancel rollback.

## Decision

The UI exposes explicit FSM states for idle, selection, move, resize, rotate, pan, creation, and nested editing. Persistent drags use `begin_transaction`, bounded `update_transaction` requests, and one `commit_transaction`. Escape and pointer cancel clear pending intent and call `rollback_transaction`.

Pointer updates use a two-animation-frame bounded queue. At most one request is in flight and one latest intent is retained. Transform/geometry operations are absolute final intents. Pan accumulates unsent deltas so coalescing cannot discard movement. Camera-only operations remain outside persistent history and revisions.

The renderer applies each accepted Worker frame before React publishes the matching overlay state.

## Consequences

- One drag produces one Undo entry.
- Escape restores the exact pre-transaction state without a new history entry.
- Burst request count is bounded and the final transform/pan intent is preserved.
- Camera pan, zoom-around-pointer, and fit do not mutate the Document.
