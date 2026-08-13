# ADR-014 - Incremental container and subtree bounds

Status: Accepted  
Date: 2026-08-08

## Context

Group/Document nodes need child-derived bounds, while Frame needs distinct own geometry and
descendant bounds. Rescanning 10,000 siblings after one leaf edit is unacceptable.

## Decision

Each Scene container owns a replaceable child-contribution aggregate. Four ordered sets track
child min-x/min-y/max-x/max-y values, with `NodeId` as deterministic tie-breaker, plus a map
for exact removal. Updating one contribution is O(log child-count), and current bounds are
read from set endpoints. A node's subtree bound is the union of its own geometry bound and
the child aggregate.

Initial build fills aggregates in reverse traversal order. Incremental work updates the
changed contribution and propagates upward only while the parent's resulting bound changes.
The structures contain runtime AABBs only and never become persistent semantic storage.

## Limitations

Visual bounds for strokes/effects are deferred. Floating values enter the ordered sets only
after finite validation and use `f64::total_cmp` for deterministic ordering.

