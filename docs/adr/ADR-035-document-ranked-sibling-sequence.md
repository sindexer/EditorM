# ADR-035: Document Ranked Sibling Sequence

- Status: Implemented for Phase 0E-R2 review
- Date: 2026-08-11
- Refines ADR-031 and ADR-033

## Context

The R1 Document stored every parent's children in `Vec<NodeId>`. Rank lookup, noncontiguous Group planning, removal, insertion, and Ungroup restoration could therefore scan, copy, or shift a parent sequence proportional to N even when k=3.

## Decision

Each Document node owns an arena-backed implicit order-statistic treap. Every treap node stores a stable `NodeId`, deterministic priority, parent/left/right arena links, and subtree size. A hash map locates a sequence node by `NodeId`; split/merge implements rank select, rank-by-ID, insertion, and removal in expected O(log N). Removed arena slots are reused.

Group and Ungroup perform O(k log N) rank/anchor operations and allocate or copy only k-dependent data. Exact counters record examined/copied/moved entries, rank comparisons, sequence-node allocations, bytes, full scans/copies, and dense rewrites. Semantic equality ignores transient counters.

Full inorder enumeration is restricted to explicit semantic snapshot, subtree snapshot, and persistence boundaries. Serialization writes the same deterministic ordered child list as before. Reversible effects and history store the k affected placements and internal restoration record rather than a full parent sequence.

## Memory and consequences

The sequence adds one arena record and one hash-map locator per child, trading higher per-entry memory for bounded edits and stable ID lookup. Insert/remove touches treap paths rather than shifting a dense array. Transaction validation and undo/redo atomicity remain command-owned.

Rejected alternatives were a dense `Vec` hidden behind helpers, which preserves O(N) shifts; fractional order labels, which require collision/rebalance policy; and a B-tree plus a separate dense ID index, which still risks mass index rewrites.
