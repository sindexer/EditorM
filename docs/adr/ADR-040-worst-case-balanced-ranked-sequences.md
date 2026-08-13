# ADR-040: Worst-Case Balanced Ranked Sequences

- Status: Implemented for Phase 0E-R3 review
- Date: 2026-08-11
- Refines: ADR-035, ADR-036, and ADR-037

## Context

An order-statistic treap whose priority is derived from a public Node ID has expected balance only for ordinary IDs. Monotonic, collision-shaped, or deliberately chosen IDs can force excessive depth. Rebuilding the whole sequence during an edit would hide the problem behind an N-sized fallback.

## Decision

Document and Scene use an arena-backed implicit order-statistic AVL sequence. React ProjectionStore uses an implicit AVL ranked sequence. External IDs are locator keys only and never influence tree shape.

Every node stores subtree size, height, parent, and stable neighbor links where applicable. Insert, remove, rank, and rank-by-ID rebalance locally. Bulk construction builds a balanced tree directly. Batch extraction prunes unaffected subtrees, reattaches an existing pivot directly when child heights already differ by at most one, and uses a verified closed-form lower bound only for a proven interpolated rank series. Other rank patterns use binary lower bounds.

No edit-time full rebuild is permitted as a balancing mechanism. Maximum depth and rotations/rebalances are measured. Canonical sibling order, not tree shape, determines semantic output.

## Consequences

Depth is worst-case logarithmic and independent of Node ID values. Monotonic, reverse, edge insertion, and collision-shaped inputs remain bounded without recursion proportional to N. Full scans, copies, dense rewrites, and fallback rebuilds remain zero on structural edit paths.
