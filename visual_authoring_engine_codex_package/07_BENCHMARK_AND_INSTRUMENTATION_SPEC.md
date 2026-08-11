# BENCHMARK AND INSTRUMENTATION SPEC

This applies primarily to Phase 0C-0E, but architecture should anticipate it.

## Benchmark fixtures

- BENCH-A: 1,000 rectangles
- BENCH-B: 10,000 rectangles
- BENCH-C: 100,000 rectangles distributed through infinite canvas
- BENCH-D: 10,000-node nested hierarchy stress fixture

## Required metrics

Where measurable:

- total document nodes;
- visible nodes;
- culled nodes;
- spatial candidates tested;
- geometry hit tests;
- dirty nodes;
- scene nodes recomputed;
- spatial entries updated;
- render items regenerated;
- GPU instances updated;
- draw calls;
- GPU batches;
- CPU frame time;
- GPU frame time;
- memory;
- runtime backend;
- worker/main-thread execution status.

## Structural performance acceptance

Do not reduce quality to one FPS target. The more important conditions are:

- spatial queries are active;
- culling is active;
- dirty update is active;
- batching is real;
- one-node edits do not cause unconditional full-document rebuilds;
- 100k-node documents do not imply 100k render submissions every frame when only a small subset is visible.

## Proof fixtures

### Dirty proof
With 10,000 nodes, change one isolated node and report visited/recomputed/updated counts.

### Spatial proof
Compare candidate count against total count. A point query in a 100k-node document should not geometry-test 100k nodes under normal distribution.

### Batch proof
Report visible primitive count versus draw calls/batches.

### Worker proof
Debug output must state where engine and rendering execute. A worker file existing on disk is not proof.

### WebGPU proof
Report actual backend/device initialization. A WebGPU-named class using Canvas2D is failure.
