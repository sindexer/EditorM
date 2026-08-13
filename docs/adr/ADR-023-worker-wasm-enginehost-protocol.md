# ADR-023: Worker-Owned WASM EngineHost Protocol

- Status: Accepted
- Date: 2026-08-09

## Context

The browser main thread must not acquire a second Document truth or bypass typed commands,
transactions, and history.

## Decision

A `wasm32-unknown-unknown` bridge exports `EngineHost`. One Dedicated Worker owns the WASM
module and `EngineRuntime`. Protocol version 1 includes request/correlation ID, synchronized
Document/Scene/Render revisions, typed command, transaction begin/update/commit/rollback,
undo/redo, camera/viewport/DPR, hit-test, render delta, metrics, and typed errors.

Control envelopes use small JSON messages. Full instance bytes, dirty records, removed slots,
and visible slots use transferable ArrayBuffers. The main thread owns only a WebGPU renderer
and session diagnostics; it has no Document mutation API.

## Consequences

Worker heartbeat and restart are observable. Protocol drift is a typed error without mutation.
The chosen capability fallback keeps core/Scene in the Worker and runs WebGPU on the main
thread because OffscreenCanvas Worker WebGPU is not assumed. HUD and proof report that exact
placement.