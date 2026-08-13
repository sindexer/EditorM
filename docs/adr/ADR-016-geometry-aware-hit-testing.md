# ADR-016 - Geometry-aware hit testing and degenerate policy

Status: Accepted  
Date: 2026-08-08

## Decision

The real point pipeline is viewport point -> Camera world conversion -> spatial point query
-> exact local geometry test -> structural z-order sort. Results expose topmost/all hits plus
candidate and exact-test counts.

Rectangle and Frame invert the cached world matrix and test their local rectangle. Ellipse
uses the normalized ellipse equation, so an AABB corner is not a hit. Translation, rotation,
non-uniform scale, reflection, nesting, and camera pan/zoom use the same path. Locked nodes
remain hittable; hidden, detached, or invalid-derived nodes never enter candidates.

## Degenerate policy

Zero-width/zero-height geometry and singular transforms are explicitly non-hittable. The
runtime does not pretend the AABB is exact geometry and does not add a screen-space tolerance
fallback at this gate. Extreme arithmetic failure returns typed camera/spatial errors or is
recorded as invalid-derived without panic.

Rectangle query is named `query_rect_candidates`: it returns AABB candidates, not an exact
marquee-selection claim.

