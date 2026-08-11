# ADR-041: Exact Structural Work Counter Contract

- Status: Implemented for Phase 0E-R3 review
- Date: 2026-08-11
- Refines: ADR-019 and ADR-033

## Context

A bounded algorithm cannot be reviewed if constructors, fragment adoption, node allocation, rotations, or maximum depth are omitted from counters. Native evidence also cannot be presented as actual WASM evidence.

## Decision

Document, Scene, EngineHost, and UI structural counters include:

- entries examined;
- entries copied;
- entries moved or shifted;
- rank/order comparisons;
- sequence nodes allocated;
- allocated bytes;
- rotations/rebalances;
- maximum sequence depth;
- full scans and copies;
- dense index rewrites;
- fallback/full rebuild count.

Structural constructors accept a work accumulator. Untracked bulk constructors are unavailable on edit paths. Scene child-sequence construction, UI Group child construction, and fragment adoption record their real allocation and traversal work.

The cross-layer composite work unit is:

`max(entries_examined, rank_order_comparisons) + entries_copied + entries_moved_or_shifted + sequence_nodes_allocated + ceil(allocated_bytes / 8) + tree_rebalances + full_sequence_scans + full_sequence_copies + dense_index_rewrites + fallback_or_rebuild_count`.

This unit is a declared structural accounting model, not elapsed CPU instructions. Raw counters remain present beside the composite value.

Planning work is committed only after a successful operation. A failed request leaves Document semantics, history, revision, selection, Scene, RenderModel, GPU delta, and sequence diagnostics unchanged.

Actual WASM proof must load `engine_host.js`, compile `engine_host_bg.wasm` into a `WebAssembly.Module`, instantiate it, call the glue `EngineHost.handleJson`, record artifact hashes and versions, and state that no native binary was used.

## Consequences

Ratios are comparable across native runtime, native EngineHost, actual WASM, React, and Chrome. Missing work cannot be hidden behind zero counters. Pre-fix failures remain evidence and are never relabeled as a pass.
