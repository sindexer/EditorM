# ADR-021: R-Star Tree Spatial Index

- Status: Accepted; supersedes ADR-015 for production Scene queries
- Date: 2026-08-09

## Context

A single fixed-cell grid makes correctness-preserving overflow buckets and broad queries
structurally expensive for highly varied geometry, sparse 100,000-node scenes, and extreme
zoom-out.

## Decision

`RTreeIndex` uses `rstar::RTree` with a NodeId-to-finite-Rect authority map. Insert, remove,
update, query, clear, and rebuild preserve the existing `SpatialIndex` contract. Production
`ComputedScene` uses `RTreeIndex`; result ordering still comes from Document child order, not
tree traversal. Broad finite queries return all actual intersections and do not synthesize
`QueryTooLarge`.

## Evidence

Mixed sizes, dense scenes, huge geometry, sparse 100,000-node layout, extreme zoom-out, and
repeated mutation tests match a test-only brute-force oracle. The final sparse narrow query
returned 2 candidates natively and 1 candidate in the browser proof, not 100,000.

## Consequences

`UniformGridIndex` remains available as a legacy replaceable implementation but is absent from
the production culling path.