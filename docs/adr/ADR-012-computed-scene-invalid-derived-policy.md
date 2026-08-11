# ADR-012 - Computed Scene data and invalid-derived policy

Status: Accepted  
Date: 2026-08-08

## Context

Finite local matrices can overflow during deep composition. Such a node must not make the
runtime stale or enter the spatial index with invalid bounds.

## Decision

`ComputedScene` stores only runtime-derived data keyed by persistent `NodeId`: parent/children,
root attachment, effective visibility, optional world transform, typed invalid reason, own
geometry bounds, subtree bounds, spatial membership, dirty categories, and revision. It does
not copy names, metadata, appearance payloads, selection, history, or UI state.

Initial build walks every forest root parent-first with an explicit stack and composes each
world transform once. Detached records remain present with `attached=false` and are not
indexed. Non-finite world composition, non-finite bounds, and invalid ancestor world state
are represented by `InvalidDerivedState`; affected geometry is excluded from spatial queries.

## Invariants and limitations

The Scene can be discarded and rebuilt from Document alone. Singular but finite transforms
remain representable; exact hit testing treats them as non-hittable rather than substituting
their AABB. Visual/effect bounds are intentionally absent.

