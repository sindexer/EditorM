# ADR-028: Sequenced Frames and a Shared Render Contract

Status: Accepted for Phase 0D-R1

## Context

Worker responses were applied through independent asynchronous GPU operations. A slower older
frame could complete after a newer response, pairing stale GPU metrics with a newer HUD.
Pointer movement also created an unbounded stream of camera requests. Native and browser
renderers separately embedded equivalent shader and binary-layout definitions.

## Decision

- EngineHost adds a monotonically increasing `engine_sequence` to every response. The render
  binary schema has its own version, separate from request protocol version 1.
- Main-thread frame application uses one serialized, generation-aware promise queue. A frame is
  committed only when its response revision and sequence match its GPU metrics. Stale or
  pre-restart work is rejected.
- Heartbeat responses update liveness only; they do not replace the current render response or
  trigger a GPU frame.
- Worker restart rejects every pending waiter with typed `worker_restarted`, resets generation
  sequencing, and prevents old-generation completion from reaching proof/HUD state.
- Pan and zoom intent is coalesced at animation-frame boundaries with at most one camera request
  in flight. Latest accumulated intent is sent after the current request completes.
- Native Rust and browser JavaScript consume `shared/render_contract.wgsl`. The checked
  `shared/render_binary_schema.json` defines strides, offsets, endianness, and primitive values;
  the browser module is generated and checked during build/test.
- Browser proof uses actual DOM events and actual WebGPU readback rather than a mock or Canvas2D
  fallback.

## Consequences

The visible frame, response, revision, and GPU metrics have one traceable sequence. Camera input
load is bounded without losing final user intent. Contract drift becomes a build/test failure,
though changing the binary layout now requires an explicit schema-version update and generator
refresh.
