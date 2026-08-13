# ADR-039: Group Restoration Runs Version 2

- Status: Implemented for Phase 0E-R3 review
- Date: 2026-08-11
- Refines: ADR-031, ADR-038, and ADR-004

## Context

A restoration record containing one before/after-anchor pair for every selected child makes Group planning repeat sibling searches and makes Ungroup planning repeatedly scan an expanding prefix. That behavior grows quadratically with the selection count k. It also leaves ambiguous behavior when a live group is edited, reparented, persisted, or loses an anchor.

## Decision

`GroupRestoration::VERSION` is 2. A record contains ordered runs. Each run stores its selected child IDs once, the nearest stable unselected anchor before the run, and the nearest stable unselected anchor after the run.

Group computes every selected rank once, sorts by rank, forms consecutive runs in one pass, and resolves anchors once per run. Ungroup validates the child set before mutation, maps current grouped children to their original run, resolves run bases through the ranked parent sequence, and advances final ranks monotonically. It does not scan an expanding planned prefix.

The deterministic policy is:

- current child order is retained inside each original run;
- original run order is retained across runs;
- a surviving before anchor is preferred, then a surviving after anchor;
- if both anchors are missing, the run restores at the current group rank;
- if the group was reparented, anchors missing in the new parent follow the same current-group-rank fallback;
- version-1 persistence records migrate to version 2 during load;
- unsupported future versions return a typed error.

Validation, transform derivation, and rank planning finish before the reversible effect mutates the Document. Undo/Redo carries the same versioned record.

## Consequences

Storage and planning are O(k) plus ranked sequence lookups, with no parent-size scan. The policy is stable under sibling insertion, deletion, reorder, group reparent, internal child reorder, save/load, and missing anchors. User metadata remains untouched.
