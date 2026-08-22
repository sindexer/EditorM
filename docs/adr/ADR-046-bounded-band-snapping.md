# ADR-046: Bounded Band Snapping and Alignment Guides

- Status: Implemented for Phase 1B review
- Date: 2026-08-22
- Follows: ADR-045

## Context

Snapping must feel like Figma: dragged edges and centers click onto nearby edges and centers, and a guide line shows what was matched. The naive implementation compares the dragged bounds against every object, which is unacceptable at the 100,000-object budget this engine already holds. The opposite shortcut, comparing only against objects overlapping the dragged rectangle, silently drops the common case of aligning with something far above or below.

## Decision

Snapping is computed in the runtime from the computed scene, never in the UI, and it never mutates state.

Candidates come from two thin band queries against the spatial index: a vertical band covering the proposed horizontal extent grown by the snap threshold, and a horizontal band covering the proposed vertical extent. Each band is clipped to the visible world viewport. Work is therefore proportional to what is near the dragged bounds within what a person can see, not to document size, and an object anywhere on screen can still produce an alignment guide.

Each axis compares three features of the proposed bounds — minimum, center, maximum — against the same three features of each candidate. Corrections beyond the threshold are discarded. The winning correction per axis is the smallest absolute one; ties prefer edge matches over center matches, then the lower guide position, then stable node identity, so the same drag always snaps the same way.

The dragged subtree is never its own snap target: the moving IDs and anything inside them are excluded by ancestry. Hidden and detached nodes are excluded. Locked objects remain valid targets, because they are visible geometry a person still aligns against.

The threshold is expressed in viewport pixels and divided by camera zoom at the request boundary, so the snap radius stays constant on screen at every zoom level. A zero threshold disables snapping without querying the index at all. A non-finite or negative threshold, or non-finite bounds, is a typed error rather than a silent no-op.

Each accepted snap returns a guide with its axis, world position, and the span of the union of the moving and matched bounds, so the editor draws exactly the segment the engine matched instead of inventing its own.

## Consequences

Snapping is testable without a browser: the engine answers a question about geometry and returns data. The UI holds only the toggle and the Alt suspend gesture. Because guides are engine output, canvas rendering, hit testing, and guides cannot drift apart.
