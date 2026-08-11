# ADR-037: UI Ranked Projection Sequence

- Status: Implemented for Phase 0E-R2 review
- Date: 2026-08-11
- Refines ADR-033

## Context

R1 applied bounded protocol operations to large JavaScript arrays. `indexOf`, `filter`, `forEach`, arbitrary `splice`, and subtree flattening still made a k=3 Group/Ungroup proportional to the 10K/100K projection size even though the payload was bounded.

## Decision

The React ProjectionStore uses a deterministic implicit ranked treap for the global preorder and every parent's child order. A value-to-node map provides rank-by-ID. Split/merge supports insert, remove, range extraction, and fragment insertion without shifting a dense array. Detached subtree fragments and cached subtree sizes let structural operations move an existing block rather than re-enumerate it.

The full projection initialization is an explicit full-rebuild boundary. Incremental Group/Ungroup, attach/detach, and move operations must not call it. Layers reads only `viewport(start, count)`, which costs O(log N + visible rows), and the mounted-row cap remains independent of total document size.

UI counters independently record examined/copied/moved entries, comparisons, allocated treap nodes/bytes, full scans/copies, dense rewrites, and structural fallback/rebuild. Projection payload size and counter ratios are measured in the actual Chrome proof as well as the direct ProjectionStore test.

## Memory and consequences

The UI stores a treap node and map entry per projected order element plus subtree-size and temporarily detached-fragment records. This uses more memory than one flat array but makes k-node structural edits expected O(k log N) and avoids N-sized copies.

Rejected alternatives were retaining arrays with helper wrappers, chunked arrays with an unbounded chunk-rebuild path, and recomputing flattened preorder after every delta.
