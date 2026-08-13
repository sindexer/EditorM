# ADR-015 - Replaceable deterministic uniform-grid spatial index

Status: Accepted  
Date: 2026-08-08

## Context

Point and rectangle hot paths require a real index. An external R-tree was considered, but
Phase 0C needs deterministic update counts, no new network-fetched critical dependency, and
simple exact removal by `NodeId` for command previews.

## Decision

`visual_authoring_spatial` defines the `SpatialIndex` abstraction and implements
`UniformGridIndex` with insert/remove/update/query-point/query-rectangle/clear/rebuild/count.
It stores only finite AABBs and NodeIds. The default logical cell is 256 units. Very large
entries use a correctness-preserving overflow bucket; extremely large rectangle queries fail
with typed `QueryTooLarge` rather than scanning every entry. Extreme finite coordinates clamp
to boundary cells and remain queryable.

Scene inserts only attached, effectively visible, valid, positive-area geometry. Index result
order is never treated as z-order. Point and rectangle production queries call this index and
then Scene applies structural document order.

## Dependency and license

The implementation uses only Rust standard-library ordered maps/sets plus local
`core_math`/`document` types and `thiserror` 2.0.16 (MIT OR Apache-2.0). No third-party spatial
library was added. The trait makes later replacement with an R-tree/BVH local to this crate;
the boundary data remains `(NodeId, finite Rect)`.

## Correctness evidence

Randomized test-only brute-force oracles compare indexed queries. Insert/update/remove,
atomic rebuild, invalid input, and BENCH-C candidate counts are automated tests.

