# ADR-017 - Structural z-order resolution

Status: Accepted  
Date: 2026-08-08

## Context

Persistent child order defines sibling z-order. Rewriting a dense global index after every
move would touch unrelated Scene nodes.

## Decision

Scene keeps runtime parent/children relations matching Document. For the small candidate set
returned by Spatial Index, it constructs each candidate's structural child-index path and
sorts paths top-to-bottom. Later siblings sort above earlier siblings; descendants sort above
their container geometry; later sibling subtrees sort above earlier subtrees. NodeId is only
a deterministic final tie-breaker for impossible/inconsistent equal paths.

Reparent and reorder update the affected parent child arrays immediately. Undo/redo consume
the reverse/forward placement report, so topmost resolution follows restored persistent order.
No dense all-node z-index exists.

## Tradeoff

Candidate sorting walks ancestors and searches local child arrays. Spatial proof keeps the
candidate set small; an order-maintenance key can replace this comparator later without
changing Document meaning or Spatial Index.

