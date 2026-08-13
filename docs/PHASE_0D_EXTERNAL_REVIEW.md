# Phase 0D External Review

Status: **Gate 0D held**. This document records the external review received on
2026-08-09. It does not authorize Phase 0E work.

## Reviewed submission

- Archive: `visual_authoring_engine_phase0d_review_2026-08-08.zip`
- Size: 46,848,080 bytes
- ZIP entries: 131
- SHA-256: `34b1f81245beec3807dd886a7256deecaf51f168bfc4187a6e817de3288f6afe`
- Internal project checksums: 130/130 matched
- Authoritative baseline materials: 18/18 matched
- Submitted evidence: 145 Rust tests, 1 doctest, 4 web unit tests, and 1
  hardware browser proof passed

The review accepted that the submission contains real WASM in a Dedicated
Worker, real WebGPU rendering, R-tree spatial queries, and instanced drawing on
an NVIDIA GeForce GTX 970. Gate 0D remains held for the defects below.

## Blocking defects

1. A `move_node` request with `x = 1e100` returned `ok:false` and
   `f32_conversion`, but Document, Scene, and Render revisions advanced from 0
   to 1 while no GPU delta was sent. Persistent and GPU-visible state diverged.
2. A single edit performed hidden O(N) work: full `RenderModel` cloning, full
   structural-order recomputation, full renderable-count scans, and linear
   sibling-position searches during z-order comparison. Independent WASM
   measurements were 7.4 ms for a 10k single edit, 96.1 ms for a 100k single
   edit, about 568.9 ms for 10k fully visible ordering, and 2.64 s for a 100k
   load.
3. Worker responses and asynchronous GPU frame application were not serialized.
   Pointer movement emitted unbounded camera requests, so old GPU metrics could
   be paired with a newer engine response.

## Required correction boundary

Phase 0D-R1 may correct atomic derived-state handling, bounded incremental
RenderModel updates, ordering and counters, Worker/GPU frame sequencing and
camera coalescing, the shared render schema, and hardware pixel/DOM proof. It
may add only the diagnostics, tests, ADRs, scripts, and review artifacts needed
to prove those corrections.

React product UI, Wanted Design System integration, and every other Phase 0E
feature remain explicitly out of scope.
